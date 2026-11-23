import { test, expect, chromium } from '@playwright/test';
import { spawn, ChildProcess } from 'node:child_process';
import crypto from 'node:crypto';
import fs from 'node:fs';
import net from 'node:net';
import os from 'node:os';
import path from 'node:path';

function sendIpc(sockPath: string, payload: unknown): Promise<any> {
  return new Promise((resolve, reject) => {
    const client = net.createConnection({ path: sockPath }, () => {
      client.write(`${JSON.stringify(payload)}\n`);
    });
    let buf = '';
    client.on('data', (chunk) => {
      buf += chunk.toString('utf8');
      if (buf.includes('\n')) {
        client.end();
        resolve(JSON.parse(buf.split('\n')[0] ?? '{}'));
      }
    });
    client.on('error', reject);
  });
}

async function startIsolatedDaemon(tmpDir: string, repoNames: string[]) {
  const fakeHome = path.join(tmpDir, 'home');
  const reposRoot = path.join(fakeHome, 'Documents/Repos');
  for (const name of repoNames) {
    fs.mkdirSync(path.join(reposRoot, name), { recursive: true });
  }

  const sockPath = path.join(tmpDir, 'braid.sock');
  const lockPath = path.join(tmpDir, 'daemon.lock');
  const cfgDir = path.join(fakeHome, 'Library/Application Support/contextbraid');
  fs.mkdirSync(cfgDir, { recursive: true });

  fs.writeFileSync(
    path.join(cfgDir, 'config.toml'),
    `
[repos]
root_dir = "${reposRoot}"

[daemon]
socket_path = "${sockPath}"

[editor]
favorite = "zed"
`,
  );

  const rootDir = path.resolve(import.meta.dirname, '../../../../');
  const braidBin = path.join(rootDir, 'target/debug/braid');
  const extPath = path.join(rootDir, 'packages/extension/.output/chrome-mv3');

  const daemon: ChildProcess = spawn(braidBin, ['daemon'], {
    env: {
      ...process.env,
      HOME: fakeHome,
      CONTEXT_BRAID_LOCK_FILE: lockPath,
    },
    stdio: 'ignore',
  });

  for (let i = 0; i < 60; i++) {
    if (fs.existsSync(sockPath)) break;
    await new Promise((r) => setTimeout(r, 100));
  }
  expect(fs.existsSync(sockPath)).toBe(true);

  return { daemon, sockPath, fakeHome, braidBin, extPath };
}

test('pipeline E2E : extension MV3 + braid daemon isolé', async () => {
  const tmpDir = fs.mkdtempSync(path.join(os.tmpdir(), 'braid-e2e-'));
  const { daemon, sockPath, extPath } = await startIsolatedDaemon(tmpDir, [
    'repo-alpha',
    'repo-beta',
  ]);

  try {
    const userDataDir = path.join(tmpDir, 'chrome-profile');
    const context = await chromium.launchPersistentContext(userDataDir, {
      channel: 'chromium',
      args: [
        `--disable-extensions-except=${extPath}`,
        `--load-extension=${extPath}`,
      ],
    });

    await sendIpc(sockPath, {
      type: 'sync_tabs',
      browser: 'chrome',
      tabs: [
        {
          browser: 'chrome',
          window_id: 1,
          tab_id: 10,
          opener_tab_id: null,
          incognito: false,
          url: ['https', '://', 'chat', '.qwen.ai', '/c/test-session'].join(''),
          title: 'Architecture repo-alpha',
          context_snippet: 'Analyse du module repo-alpha',
          status: 'working',
          updated_at_ms: Date.now(),
        },
        {
          browser: 'chrome',
          window_id: 2,
          tab_id: 20,
          opener_tab_id: null,
          incognito: true,
          url: ['https', '://', 'chat', '.z.ai', '/c/test-session'].join(''),
          title: 'Agent Task',
          context_snippet: 'Implémentation sur repo-beta',
          status: 'needs_attention',
          updated_at_ms: Date.now(),
        },
      ],
    });

    const dash = await sendIpc(sockPath, { type: 'get_dashboard' });
    expect(dash.projects).toHaveLength(2);
    expect(dash.projects[0].repo_name).toBe('repo-beta');
    expect(dash.projects[0].has_alert).toBe(true);
    expect(dash.projects[1].repo_name).toBe('repo-alpha');

    await context.close();
  } finally {
    daemon.kill('SIGTERM');
    fs.rmSync(tmpDir, { recursive: true, force: true });
  }
});

test('E2E réseau pur : Google AI Mode (/async/folif) sans bouton Stop DOM -> braid daemon', async () => {
  const tmpDir = fs.mkdtempSync(path.join(os.tmpdir(), 'braid-e2e-net-'));
  const { daemon, sockPath, fakeHome, braidBin, extPath } = await startIsolatedDaemon(
    tmpDir,
    ['repo-alpha'],
  );

  try {
    const userDataDir = path.join(tmpDir, 'chrome-profile');
    const nmHostsDir = path.join(userDataDir, 'NativeMessagingHosts');
    fs.mkdirSync(nmHostsDir, { recursive: true });

    const wrapperSh = path.join(tmpDir, 'braid-host-wrapper.sh');
    fs.writeFileSync(
      wrapperSh,
      `#!/bin/sh\nexport HOME="${fakeHome}"\nexec "${braidBin}" host\n`,
      { mode: 0o755 },
    );

    const manifestJson = JSON.parse(
      fs.readFileSync(path.join(extPath, 'manifest.json'), 'utf8'),
    );
    const keyDer = Buffer.from(manifestJson.key, 'base64');
    const digest = crypto.createHash('sha256').update(keyDer).digest('hex').slice(0, 32);
    const extId = Array.from(digest)
      .map((c) => String.fromCharCode('a'.charCodeAt(0) + parseInt(c, 16)))
      .join('');

    fs.writeFileSync(
      path.join(nmHostsDir, 'com.contextbraid.host.json'),
      JSON.stringify({
        name: 'com.contextbraid.host',
        description: 'ContextBraid E2E Test Host',
        path: wrapperSh,
        type: 'stdio',
        allowed_origins: [`chrome-extension://${extId}/`],
      }),
    );

    const context = await chromium.launchPersistentContext(userDataDir, {
      channel: 'chromium',
      args: [
        `--disable-extensions-except=${extPath}`,
        `--load-extension=${extPath}`,
      ],
    });

    const searchUrl = ['https', ':', '//', 'www', '.google.com', '/search?q=repo-alpha'].join('');
    const folifUrl = ['https', ':', '//', 'www', '.google.com', '/async/folif?yv=3&lns_mode=cvst'].join('');

    let releaseFolif!: () => void;
    const folifStarted = new Promise<void>((resolve) => {
      void context.route('**/async/folif*', async (route) => {
        resolve();
        await new Promise<void>((r) => {
          releaseFolif = r;
        });
        await route.fulfill({
          status: 200,
          contentType: 'text/plain',
          body: 'done',
        });
      });
    });

    await context.route('**/search?*', async (route) => {
      await route.fulfill({
        status: 200,
        contentType: 'text/html',
        body: '<!doctype html><html><head><title>repo-alpha Google AI</title></head><body><main>Minimized shell without Stop button</main></body></html>',
      });
    });

    const page = await context.newPage();
    await page.goto(searchUrl);
    await new Promise((r) => setTimeout(r, 700));

    const fetchPromise = page.evaluate((u) => fetch(u).then((r) => r.text()), folifUrl);
    await folifStarted;

    let sawWorking = false;
    for (let i = 0; i < 30; i++) {
      const dash = await sendIpc(sockPath, { type: 'get_dashboard' });
      const proj = dash.projects?.find((p: any) => p.repo_name === 'repo-alpha');
      if (proj?.tabs?.some((t: any) => t.status === 'working')) {
        sawWorking = true;
        break;
      }
      await new Promise((r) => setTimeout(r, 100));
    }
    expect(sawWorking).toBe(true);

    releaseFolif();
    await fetchPromise;

    let sawIdleAfter = false;
    for (let i = 0; i < 30; i++) {
      const dash = await sendIpc(sockPath, { type: 'get_dashboard' });
      const proj = dash.projects?.find((p: any) => p.repo_name === 'repo-alpha');
      if (proj?.tabs?.length > 0 && proj.tabs.every((t: any) => t.status === 'idle')) {
        sawIdleAfter = true;
        break;
      }
      await new Promise((r) => setTimeout(r, 100));
    }
    expect(sawIdleAfter).toBe(true);

    await context.close();
  } finally {
    daemon.kill('SIGTERM');
    fs.rmSync(tmpDir, { recursive: true, force: true });
  }
});
