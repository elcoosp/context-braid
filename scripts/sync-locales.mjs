#!/usr/bin/env node
import fs from "node:fs";
import path from "node:path";

const ROOT = path.resolve(import.meta.dirname, "..");
const CANONICAL_LOCALES = ["de", "en", "es", "fr", "ja", "ko", "zh-CN"].sort();

function fail(msg) {
  console.error(`✗ Locale sync error: ${msg}`);
  process.exit(1);
}

// 1. Check Rust YAML locales directory & key parity
const rustLocalesDir = path.join(ROOT, "crates/braid/locales");
const rustFiles = fs
  .readdirSync(rustLocalesDir)
  .filter((f) => f.endsWith(".yml"))
  .map((f) => f.replace(/\.yml$/, ""))
  .sort();

if (JSON.stringify(rustFiles) !== JSON.stringify(CANONICAL_LOCALES)) {
  fail(`Rust locales [${rustFiles}] != canonical [${CANONICAL_LOCALES}]`);
}

const parseYamlKeys = (file) => {
  const keys = [];
  for (const line of fs.readFileSync(file, "utf8").split("\n")) {
    const m = line.match(/^([a-zA-Z0-9_.-]+):\s*(.*)$/);
    if (m && m[1] !== "_version") {
      const key = m[1];
      const val = m[2] ?? "";
      if (val.trim() === "" || val.trim() === '""') {
        fail(`Empty translation for key "${key}" in ${path.relative(ROOT, file)}`);
      }
      keys.push(key);
    }
  }
  return keys.sort();
};

const baseRustKeys = parseYamlKeys(path.join(rustLocalesDir, "en.yml"));
for (const loc of CANONICAL_LOCALES) {
  const keys = parseYamlKeys(path.join(rustLocalesDir, `${loc}.yml`));
  if (JSON.stringify(keys) !== JSON.stringify(baseRustKeys)) {
    const missing = baseRustKeys.filter((k) => !keys.includes(k));
    const extra = keys.filter((k) => !baseRustKeys.includes(k));
    fail(`crates/braid/locales/${loc}.yml key mismatch (missing: [${missing}], extra: [${extra}])`);
  }
}

// 2. Check Rust init_system_locale() handles every canonical locale
const macosRs = fs.readFileSync(path.join(ROOT, "crates/braid/src/macos.rs"), "utf8");
for (const loc of CANONICAL_LOCALES) {
  if (!macosRs.includes(`"${loc}"`)) {
    fail(`crates/braid/src/macos.rs does not reference locale "${loc}"`);
  }
}

// 3. Check Raycast Lingui locale directories
const raycastLocalesDir = path.join(ROOT, "packages/raycast/src/locales");
const raycastLocales = fs
  .readdirSync(raycastLocalesDir, { withFileTypes: true })
  .filter((d) => d.isDirectory())
  .map((d) => d.name)
  .sort();

if (JSON.stringify(raycastLocales) !== JSON.stringify(CANONICAL_LOCALES)) {
  fail(`Raycast locales [${raycastLocales}] != canonical [${CANONICAL_LOCALES}]`);
}

// 4. Check Browser Extension _locales (zh_CN -> zh-CN)
const extLocalesDir = path.join(ROOT, "packages/extension/public/_locales");
const extLocales = fs
  .readdirSync(extLocalesDir, { withFileTypes: true })
  .filter((d) => d.isDirectory())
  .map((d) => d.name.replace("_", "-"))
  .sort();

if (JSON.stringify(extLocales) !== JSON.stringify(CANONICAL_LOCALES)) {
  fail(`Extension _locales [${extLocales}] != canonical [${CANONICAL_LOCALES}]`);
}

// 5. Check store-listing.yml locales
const storeYml = fs.readFileSync(path.join(ROOT, "store-listing.yml"), "utf8");
for (const loc of CANONICAL_LOCALES) {
  if (!new RegExp(`^    ${loc}:`, "m").test(storeYml)) {
    fail(`store-listing.yml missing storeListing.locales entry for "${loc}"`);
  }
}

console.log(
  `✓ Locale sync OK: [${CANONICAL_LOCALES.join(", ")}] aligned across Rust (${baseRustKeys.length} keys), Raycast, Extension, and store-listing.yml`
);
