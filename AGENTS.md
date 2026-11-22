# AGENTS.md — ContextBraid Engineering & Workflow Guide

This file documents the architecture, build/deploy commands, performance invariants, and coding conventions of **ContextBraid** so any AI coding agent can work accurately without regressions.

---

## 1. Repository Architecture

ContextBraid is a local macOS workspace cockpit linking Git repositories, code editors, and browser AI agents inside Raycast:

- **`crates/braid-core/`** — Shared Rust IPC protocol (`ipc.rs`), configuration, and domain models (`ProjectWorkspace`, `TabContext`, `MergedPr`).
- **`crates/braid/`** — Native macOS Rust binary (`braid`) acting as:
  - **`braid daemon`**: Unix-socket server (`~/Library/Application Support/ContextBraid/braid.sock`), Git/editor/window scanner (`macos.rs`), sparkline PNG renderer (`chart_png.rs`), and native `UNUserNotificationCenter` dispatcher.
  - **`braid host`** (also invoked via symlink/argv0 `braid-host`): Chrome/Brave/Firefox Native Messaging bridge forwarding browser tab state to the local daemon.
- **`packages/raycast/`** — Raycast TypeScript/React extension (`@raycast/api`) with:
  - `src/index.tsx` — Main cockpit (`List` + `Detail` + Git/PR/Agent actions).
  - `src/menu-bar.tsx` — macOS `MenuBarExtra` (grouped `Active` workspaces vs idle `Repositories`, semantic status icons, truncated tab titles).
  - `src/cycle-project.ts` & `src/jump-alert.ts` — Instant no-view commands.
  - `src/bootstrap.ts` — Downloads `braid-macos-universal` from GitHub Releases and installs `ContextBraid.app` + `com.contextbraid.daemon.plist`.
  - `src/locales/{en,fr,zh-CN,ja,de,es,ko}/` — Lingui v5 message catalogs compiled via SWC (`@lingui/swc-plugin`).
- **`packages/extension/`** — WXT WebExtension (`chrome-mv3` and `firefox-mv2`) detecting AI agent tabs (`Z.ai`, `Qwen`, `Gemini`, `Google AI`, `DeepSeek`), auto-downloading newly generated `.zip` artifacts in background/minimized windows, and relaying tab context via Native Messaging.

---

## 2. Build, Lint & Test Commands (Always Run Before Committing)

### Rust Daemon & Native Host
```bash
cargo test --workspace
cargo build --release --package braid
```

### Raycast Extension (`packages/raycast`)
> **Important:** `npx ray lint` automatically runs **ESLint**, **Prettier 3.10.1**, TypeScript checks, and Raycast Store metadata validation. Always run it before committing UI/i18n changes.
```bash
cd packages/raycast
pnpm run i18n:extract
pnpm run i18n:check       # Must pass with 0 missing translations across all 7 locales
pnpm run i18n:compile
npx ray lint              # Must pass with 0 errors and 0 warnings
pnpm build
```

### Browser Extension (`packages/extension`)
```bash
pnpm --filter @context-braid/extension test:unit
pnpm --filter @context-braid/extension build
pnpm --filter @context-braid/extension build:firefox
```

---

## 3. Local macOS Binary Deployment (Critical!)

The active daemon binary is **NOT** run from `target/release/braid` or `packages/raycast/assets/`. On macOS, `launchd` (`~/Library/LaunchAgents/com.contextbraid.daemon.plist`) executes:
- `~/Library/Application Support/ContextBraid/ContextBraid.app/Contents/MacOS/braid`
- `~/Library/Application Support/ContextBraid/bin/braid`

Whenever Rust code in `crates/` changes, redeploy locally with:
```bash
cargo build --release --package braid
PLIST="\$HOME/Library/LaunchAgents/com.contextbraid.daemon.plist"
APP_BIN="\$HOME/Library/Application Support/ContextBraid/ContextBraid.app/Contents/MacOS/braid"
CLI_BIN="\$HOME/Library/Application Support/ContextBraid/bin/braid"

launchctl bootout "gui/(id -u)" "PLIST" 2>/dev/null || true
cp -f target/release/braid "\$APP_BIN"
cp -f target/release/braid "\$CLI_BIN"
chmod +x "\(APP_BIN" "\)CLI_BIN"
codesign --force --sign - "\(APP_BIN" "\)CLI_BIN"
launchctl bootstrap "gui/(id -u)" "PLIST"
```
> Always `codesign --force --sign -` after replacing the binary on Apple Silicon, otherwise macOS Gatekeeper/AMFI will `SIGKILL` the process on launch.

---

## 4. Performance & Architecture Invariants (Do Not Regress)

1. **Non-blocking Tokio runtime (`crates/braid/src/macos.rs`):**
   - Never call `.output()` on fire-and-forget UI commands (`open -a Terminal`, `afplay`, `osascript`). Always use `.spawn()` so Tokio worker threads never block on GUI launches.
2. **Deterministic PNG Sparkline Cache (`crates/braid/src/chart_png.rs`):**
   - Chart filenames are content-addressed. Keep the early `out_path.metadata().map(|m| m.len() > 0)` check before allocating the RGBA buffer to avoid redundant disk writes.
3. **Single-Pass DOM Extraction & Background Auto-Download (`packages/extension/`):**
   - Keep `findZipDownloadTargets` as a single combined `querySelectorAll` sweep in `src/core/extractor.ts`.
   - `content/index.ts` captures a baseline of pre-existing `.zip` links on mount (`2.6s` after boot) so historical conversation artifacts are never re-downloaded.
   - `FORCE_CONTEXT_PROBE` (triggered every 6s via `browser.alarms` and on `webRequest.onCompleted`) must always invoke `safeEvaluate(true)` so new ZIPs auto-download even when browser tabs are hidden or windows are minimized.
   - Background telemetry (`cb_autodownload_logs` in `browser.storage.local`) is flushed in batches every `1500ms` in `entrypoints/background/index.ts` — never revert to synchronous per-event `storage.local.set`.
4. **Reference-Stable Raycast Polling (`packages/raycast/src/index.tsx`):**
   - `usePromise` polls `get_dashboard` every `1500ms`. Always compare `JSON.stringify(next)` against `lastSnapshotRef` and return the previous array reference when unchanged to prevent idle React tree re-renders.
5. **Menu Bar Submenu Width (`packages/raycast/src/menu-bar.tsx`):**
   - Always truncate tab titles (`<= 38 chars`) with full text in `tooltip` so long Google AI / search queries never stretch the macOS menu bar popover across the screen.
   - Note that `@raycast/api`'s `MenuBarExtra.Submenu` does **not** accept a `subtitle` prop; encode compact status badges (`●`, `⚡`, `✓`, `~dirty`) into `title`.

---

## 5. Localization Rules (7 Locales)

- Supported locales: `en`, `fr`, `zh-CN`, `ja`, `de`, `es`, `ko`.
- Wrap all user-facing strings in Raycast with Lingui macros (`t` or `plural` from `@lingui/core/macro`).
- Browser extension store metadata lives in `packages/extension/public/_locales/{en,fr,zh_CN,ja,de,es,ko}/messages.json` (note `zh_CN` with underscore for WebExtensions vs `zh-CN` with hyphen for Lingui).
- After adding or modifying any `t` string, run `pnpm run i18n:extract --clean`, translate all 7 `.po` files, and verify `pnpm run i18n:check` outputs `0 missing`.

---

## 6. Debugging & Telemetry

- **Daemon logs:** `/tmp/braidd.log` and `/tmp/braidd.err.log` (or `./scripts/live-logs.sh`).
- **Extension auto-download logs:** Open the ContextBraid Service Worker DevTools console in `brave://extensions` / `chrome://extensions` and run:
  ```js
  chrome.storage.local.get('cb_autodownload_logs', (d) => console.table(d.cb_autodownload_logs));
  ```
- **Reloading after extension build:** Reloading the extension (`↻`) on `brave://extensions` automatically re-injects the updated content script into all open AI tabs via `probeAllOpenAiTabs()` — no need to refresh open browser tabs.

## Icon Asset Single Source of Truth
- `docs/logo.png` (1024x1024 transparent PNG) is the single source of truth for all Raycast icons, browser extension icons, Store icons, and macOS notification `AppIcon.icns`.
- Run `pnpm icons:sync` after any change to `docs/logo.png`.
