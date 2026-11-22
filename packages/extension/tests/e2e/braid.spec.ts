import { test, expect, chromium } from '@playwright/test';
import { spawn, ChildProcess } from 'node:child_process';
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

test('pipeline E2E : extension MV3 + braidd isolé', async () => {
  const tmpDir = fs.mkdtempSync(path.join(os.tmpdir(), 'braid-e2e-'));
  const reposDir = path.join(tmpDir, 'repos');
  fs.mkdirSync(path.join(reposDir, 'repo-alpha'), { recursive: true });
  fs.mkdirSync(path.join(reposDir, 'repo-beta'), { recursive: true });

  const sockPath = path.join(tmpDir, 'braid.sock');
  const cfgPath = path.join(tmpDir, 'config.toml');
  fs.writeFileSync(
    cfgPath,
    `
[repos]
root = "${reposDir}"
ignore = [".git"]

[daemon]
socket_path = "${sockPath}"

[shortcuts]
cycle_project_windows = "Option+Tab"
open_dashboard = "Option+Space"
jump_next_alert = "Option+A"
`,
  );

  const rootDir = path.resolve(import.meta.dirname, '../../../../');
  const braiddBin = path.join(rootDir, 'target/debug/braidd');
  const extPath = path.join(rootDir, 'packages/extension/.output/chrome-mv3');

  const daemon: ChildProcess = spawn(braiddBin, [], {
    env: { ...process.env, BRAID_CONFIG_PATH: cfgPath },
    stdio: 'ignore',
  });

  try {
    // Attend que le socket Unix soit prêt
    for (let i = 0; i < 30; i++) {
      if (fs.existsSync(sockPath)) break;
      await new Promise((r) => setTimeout(r, 100));
    }
    expect(fs.existsSync(sockPath)).toBe(true);

    // Charge la vraie extension MV3 dans Chromium
    const userDataDir = path.join(tmpDir, 'chrome-profile');
    const context = await chromium.launchPersistentContext(userDataDir, {
      channel: 'chromium',
      args: [
        `--disable-extensions-except=${extPath}`,
        `--load-extension=${extPath}`,
      ],
    });

    // Simule la synchronisation de 2 onglets (Qwen + z.ai avec Captcha sur repo-beta)
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
    // repo-beta doit remonter en position 0 car status = needs_attention
    expect(dash.projects[0].repo_name).toBe('repo-beta');
    expect(dash.projects[0].has_alert).toBe(true);
    expect(dash.projects[1].repo_name).toBe('repo-alpha');

    await context.close();
  } finally {
    daemon.kill('SIGTERM');
    fs.rmSync(tmpDir, { recursive: true, force: true });
  }
});
