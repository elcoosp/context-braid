import { extractPageState } from '../../src/core/extractor';
import { AI_MATCH_PATTERNS } from '../../src/core/origins';

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
    let lastSignature = '';
    let lastSentAt = 0;
    let timer: ReturnType<typeof setTimeout> | null = null;
    const browserName = detectBrowserName();

    const observer = new MutationObserver(() => scheduleCheck());

    const teardown = () => {
      observer.disconnect();
      if (timer) clearTimeout(timer);
      clearInterval(keepaliveId);
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
          url: window.location.href,
          title: document.title,
          ...state,
        });
      } catch {
        teardown();
      }
    };

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
      }, 800);
    };

    const keepaliveId = setInterval(() => evaluateAndSend(true), 10000);

    evaluateAndSend(true);
    setTimeout(() => evaluateAndSend(true), 2500);
    document.addEventListener('visibilitychange', () => {
      if (!document.hidden) evaluateAndSend(true);
    });

    observer.observe(document.body, {
      childList: true,
      subtree: true,
    });
  },
});
