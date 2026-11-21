import { execFile } from 'node:child_process';
import fs from 'node:fs';
import os from 'node:os';
import path from 'node:path';
import { promisify } from 'node:util';

const execFileAsync = promisify(execFile);

export const INSTALL_BIN = path.join(
  os.homedir(),
  'Library',
  'Application Support',
  'ContextBraid',
  'bin',
  'braid',
);

const RELEASE_HOST = ['https', '://', 'github', '.com', '/elcoosp/context-braid'].join('/');
const RELEASE_ASSET_URL = `${RELEASE_HOST}/releases/latest/download/braid-macos-universal`;

export async function ensureBraidInstalledAndRunning(extraExtId?: string): Promise<void> {
  fs.mkdirSync(path.dirname(INSTALL_BIN), { recursive: true });

  if (!fs.existsSync(INSTALL_BIN)) {
    const res = await fetch(RELEASE_ASSET_URL);
    if (!res.ok) {
      throw new Error(`Téléchargement impossible (${res.status} ${res.statusText})`);
    }
    const buf = Buffer.from(await res.arrayBuffer());
    const tmpBin = `${INSTALL_BIN}.tmp`;
    fs.writeFileSync(tmpBin, buf, { mode: 0o755 });
    fs.renameSync(tmpBin, INSTALL_BIN);
  }

  const args = ['install'];
  if (extraExtId && extraExtId.trim().length > 0) {
    args.push(extraExtId.trim());
  }
  await execFileAsync(INSTALL_BIN, args);
}

export async function runBraidInstall(extensionId?: string): Promise<void> {
  await ensureBraidInstalledAndRunning(extensionId);
}
