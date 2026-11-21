import { execFileSync } from 'node:child_process';
import fs from 'node:fs';
import path from 'node:path';
import { transformFileSync } from '@swc/core';

execFileSync('npx', ['lingui', 'compile', '--typescript'], { stdio: 'inherit' });
execFileSync('npx', ['tsc', '--noEmit'], { stdio: 'inherit' });

const srcDir = path.resolve('src');
const backupDir = path.resolve('.src-backup');
if (fs.existsSync(backupDir)) fs.rmSync(backupDir, { recursive: true, force: true });
fs.cpSync(srcDir, backupDir, { recursive: true });

function transformDir(dir) {
  for (const entry of fs.readdirSync(dir, { withFileTypes: true })) {
    const full = path.join(dir, entry.name);
    if (entry.isDirectory()) {
      if (entry.name === 'locales') continue;
      transformDir(full);
    } else if (entry.name.endsWith('.ts') || entry.name.endsWith('.tsx')) {
      const { code } = transformFileSync(full, {
        filename: full,
        jsc: {
          parser: { syntax: 'typescript', tsx: entry.name.endsWith('.tsx') },
          transform: { react: { runtime: 'automatic' } },
          target: 'es2022',
          experimental: { plugins: [['@lingui/swc-plugin', {}]] },
        },
        module: { type: 'es6' },
      });
      fs.writeFileSync(full, `// @ts-nocheck\n${code}`);
    }
  }
}

try {
  transformDir(srcDir);
  execFileSync('npx', ['ray', 'build', '-e', 'dist'], { stdio: 'inherit' });
} finally {
  fs.rmSync(srcDir, { recursive: true, force: true });
  fs.renameSync(backupDir, srcDir);
}
