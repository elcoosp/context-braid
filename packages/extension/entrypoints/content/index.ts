import { extractPageState, findZipDownloadTargets } from '../../src/core/extractor';
import { AI_MATCH_PATTERNS } from '../../src/core/origins';

declare global {
  interface Window {
    __contextBraidCleanup?: () => void;
  }
}

function detectBrowserName(): string {
  if (import.meta.env.BROWSER === 'firefox') return 'firefox';
  if ('brave' in navigator) return 'brave';
  if (navigator.userAgent.includes('Edg/')) return 'edge';
  return 'chrome';
}

function isContextValid(): boolean {
  try {
    return Boolean(browser?.runtime?.id);
  } catch {
    return false;
  }
}

export default defineContentScript({
  matches: AI_MATCH_PATTERNS,
  runAt: 'document_idle',
  main() {
    // Garantit un seul observer et un seul timer par onglet, même en cas de ré-injection
    window.__contextBraidCleanup?.();

    let lastSignature = '';
    let lastSentAt = 0;
    let timer: ReturnType<typeof setTimeout> | null = null;
    const browserName = detectBrowserName();

    const observer = new MutationObserver(() => { if (Date.now() - bootedAt >= 2500) scheduleCheck(); });

    
    const STORAGE_KEY = `cb_downloaded_zips:${window.location.pathname}`;
    const downloadedKeys = new Set<string>(
      JSON.parse(sessionStorage.getItem(STORAGE_KEY) || "[]"),
    );
    let baselineCaptured = false;

    const persistKeys = () => {
      try {
        sessionStorage.setItem(STORAGE_KEY, JSON.stringify([...downloadedKeys].slice(-50)));
      } catch {
        // ignore storage quota
      }
    };

    const emitAutoDownloadLog = (
      event: string,
      details: Record<string, unknown> = {},
    ) => {
      const entry = {
        ts: new Date().toISOString(),
        source: "content",
        event,
        hidden: document.hidden,
        pageUrl: window.location.href,
        ...details,
      };
      console.info("[ContextBraid:AutoDownload]", entry);
      browser.runtime.sendMessage({
        type: "AUTO_DOWNLOAD_LOG",
        entry,
      }).catch(() => {});
    };

    const checkAndAutoDownloadZips = () => {
      const targets = findZipDownloadTargets(document);
      if (!baselineCaptured) {
        for (const t of targets) {
          downloadedKeys.add(t.key);
        }
        baselineCaptured = true;
        persistKeys();
        emitAutoDownloadLog("baseline_captured", {
          ignoredCount: targets.length,
          keys: targets.map((t) => t.key),
        });
        return;
      }

      for (const t of targets) {
        if (downloadedKeys.has(t.key)) continue;
        downloadedKeys.add(t.key);
        persistKeys();
        emitAutoDownloadLog("target_detected", {
          key: t.key,
          targetUrl: t.url || "(dom-button)",
        });

        if (t.url.startsWith("http://") || t.url.startsWith("https://")) {
          browser.runtime.sendMessage({
            type: "AUTO_DOWNLOAD_ZIP",
            url: t.url,
            pageUrl: window.location.href,
            hidden: document.hidden,
          }).catch((err: unknown) => {
            emitAutoDownloadLog("send_message_failed_dom_fallback", {
              key: t.key,
              error: String(err),
            });
            t.element.click();
          });
        } else {
          emitAutoDownloadLog("dom_click_triggered", { key: t.key });
          t.element.click();
        }
      }
    };

    const evaluateAndSend = (force = false) => {
      if (!isContextValid()) {
        teardown();
        return;
      }

      const state = extractPageState(document);
      const signature = `${state.status}|${document.title}|${state.contextSnippet}|${state.zipDownloadUrl ?? ''}`;
      const now = Date.now();
      if (!force && signature === lastSignature && now - lastSentAt < 9000) return;
      lastSignature = signature;
      lastSentAt = now;

      try {
        browser.runtime.sendMessage({
          type: 'TAB_CONTEXT_UPDATE',
          browserName,
          hidden: document.hidden,
          url: window.location.href,
          title: document.title,
          ...state,
        });
      } catch {
        teardown();
      }
    };

    const onVisibilityChange = () => {
      if (!document.hidden) safeEvaluate(true);
    };

    const onRuntimeMessage = (msg: { type?: string }) => {
      if (msg?.type === 'PING_EXTRACT') {
        safeEvaluate(true);
      }
    };

    const keepaliveId = setInterval(() => evaluateAndSend(true), 12000);
    const bootTimerId = setTimeout(() => evaluateAndSend(true), 2500);

    const teardown = () => {
      observer.disconnect();
      if (timer) clearTimeout(timer);
      clearInterval(keepaliveId);
      clearTimeout(bootTimerId);
      document.removeEventListener('visibilitychange', onVisibilityChange);
      try {
        browser.runtime.onMessage.removeListener(onRuntimeMessage);
      } catch {}
      if (window.__contextBraidCleanup === teardown) {
        delete window.__contextBraidCleanup;
      }
    };

    window.__contextBraidCleanup = teardown;

    const scheduleCheck = () => {
      if (!isContextValid()) {
        teardown();
        return;
      }
      if (timer) return;
      timer = setTimeout(() => {
        timer = null;
        if ('requestIdleCallback' in window) {
          window.requestIdleCallback(() => evaluateAndSend(false), { timeout: 1000 });
        } else {
          evaluateAndSend(false);
        }
      }, 900);
    };

    const bootedAt = Date.now();
    const safeEvaluate = (force = false) => {
      const state = extractPageState(document);
      // Ignore les squelettes transitoires de chargement initial (< 2.5s après montage)
      if (state.status === 'working' && Date.now() - bootedAt < 2500) return;
      checkAndAutoDownloadZips();
      evaluateAndSend(force);
    };

    setTimeout(() => safeEvaluate(true), 2600);
    document.addEventListener('visibilitychange', onVisibilityChange);
    browser.runtime.onMessage.addListener(onRuntimeMessage);

    if (document.body) {
      observer.observe(document.body, {
        childList: true,
        subtree: true,
      });
    }
  },
});
