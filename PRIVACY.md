# Privacy Policy — ContextBraid

**Last updated:** November 21, 2026

ContextBraid is a local-first developer productivity tool for macOS.

## 1. No Remote Data Collection
- ContextBraid **does not** operate external servers, analytics endpoints, or cloud telemetry.
- All communication between the browser extension, the Native Messaging host (`braid-host`), the local daemon (`braid`), and Raycast occurs strictly over a **local macOS Unix domain socket** (`~/Library/Application Support/ContextBraid/braid.sock`).

## 2. Browser Permissions Explained
- **`tabs` & `host_permissions` (`z.ai`, `qwen.ai`, `gemini.google.com`, `google.com/search`, `chat.deepseek.com`):** Used solely to detect active AI coding sessions, extract repository context titles/statuses, and focus the matching tab when triggered from Raycast.
- **`scripting`:** Used to inject the local status detector into supported AI agent pages when the extension starts or reloads.
- **`alarms` & `webRequest`:** Used locally to refresh agent status and detect completed responses when tabs are in the background or minimized.
- **`downloads`:** Used solely to automatically save newly generated `.zip` code archives from AI agent tabs into your local `Downloads` folder.
- **`storage`:** Stores local deduplication keys and a local 50-entry diagnostic ring buffer (`cb_autodownload_logs`) inside `browser.storage.local`. Nothing is transmitted externally.
- **`nativeMessaging`:** Connects the browser extension to the local `ContextBraid.app` Native Messaging host on your Mac.

## 3. Contact
For questions or security reports, open an issue at https://github.com/elcoosp/context-braid/issues.
