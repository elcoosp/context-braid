#!/usr/bin/env node
import fs from "node:fs";
import path from "node:path";

const ROOT = path.resolve(import.meta.dirname, "..");
const args = process.argv.slice(2);
const checkOnly = args.includes("--check");
const setIdx = args.indexOf("--set");

const rootPkgPath = path.join(ROOT, "package.json");
const rootPkg = JSON.parse(fs.readFileSync(rootPkgPath, "utf8"));
const targetVersion = setIdx !== -1 && args[setIdx + 1] ? args[setIdx + 1].replace(/^v/, "") : rootPkg.version;

if (!/^\d+\.\d+\.\d+(-[0-9A-Za-z.-]+)?$/.test(targetVersion)) {
  console.error(`✗ Invalid SemVer version: "${targetVersion}"`);
  process.exit(1);
}

const targets = [
  {
    file: "package.json",
    regex: /"version":\s*"[^"]+"/,
    replacement: `"version": "${targetVersion}"`,
  },
  {
    file: "Cargo.toml",
    regex: /^version\s*=\s*"[^"]+"/m,
    replacement: `version = "${targetVersion}"`,
  },
  {
    file: "crates/braid/Cargo.toml",
    regex: /^version\s*=\s*"[^"]+"/m,
    replacement: `version = "${targetVersion}"`,
  },
  {
    file: "packages/extension/package.json",
    regex: /"version":\s*"[^"]+"/,
    replacement: `"version": "${targetVersion}"`,
  },
  {
    file: "packages/extension/wxt.config.ts",
    regex: /version:\s*'[^']+'/,
    replacement: `version: '${targetVersion}'`,
  },
  {
    file: "store-listing.yml",
    regex: /context-braidextension-[0-9]+\.[0-9]+\.[0-9]+(-[0-9A-Za-z.-]+)?-/g,
    replacement: `context-braidextension-${targetVersion}-`,
  },
];

let drifted = [];
for (const t of targets) {
  const full = path.join(ROOT, t.file);
  const src = fs.readFileSync(full, "utf8");
  const next = src.replace(t.regex, t.replacement);
  if (src !== next) {
    drifted.push(t.file);
    if (!checkOnly) {
      fs.writeFileSync(full, next);
    }
  }
}

if (checkOnly && drifted.length > 0) {
  console.error(`✗ Version desync (expected ${targetVersion}) in: ${drifted.join(", ")}`);
  process.exit(1);
}

console.log(
  drifted.length > 0
    ? `✓ Synced version ${targetVersion} across: ${drifted.join(", ")}`
    : `✓ Version sync OK: all manifests at ${targetVersion}`
);
