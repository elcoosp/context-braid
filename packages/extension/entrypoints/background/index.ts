function detectRuntimeBrowser(): string {
  if (import.meta.env.BROWSER === 'firefox') return 'firefox';
  const ua = navigator.userAgent;
  if (ua.includes('Brave') || 'brave' in navigator) return 'brave';
  if (ua.includes('Edg/')) return 'edge';
  return 'chrome';
}

function isSupportedAiUrl(rawUrl?: string): boolean {
  if (!rawUrl) return false;
  try {
    const u = new URL(rawUrl);
    if (u.protocol !== 'https:') return false;
    const h = u.hostname;
    return (
      h === 'z.ai' ||
      h.endsWith('.z.ai') ||
      h === 'qwen.ai' ||
      h.endsWith('.qwen.ai') ||
      h === '://google.com' ||
      (h === '://google.com' && u.pathname.startsWith('/search')) ||
      h === '://deepseek.com'
    );
  } catch {
    return false;
  }
}

export default defineBackground(() => {
  const HOST_NAME = 'com.contextbraid.host';
  const browserName = detectRuntimeBrowser();
  let port: ReturnType<typeof browser.runtime.connectNative> | null = null;
  const tabCache = new Map<number, Record<string, unknown>>();

  console.info('[ContextBraid][BG] Boot', {
    extId: browser.runtime.id,
    browserName,
  });

  const reinjectOpenTabs = async () => {
    try {
      const files =
        browser.runtime.getManifest().content_scripts?.at(0)?.js ?? ['content-scripts/content.js'];
      const tabs = await browser.tabs.query({});
      const matching = tabs.filter((t) => t.id != null && isSupportedAiUrl(t.url));
      console.info('[ContextBraid][BG] Open tabs scan', {
        totalTabs: tabs.length,
        matchingTabs: matching.map((t) => ({ id: t.id, incognito: t.incognito, url: t.url })),
      });
      for (const tab of matching) {
        if (tab.id != null) {
          browser.scripting
            .executeScript({
              target: { tabId: tab.id },
              files,
            })
            .catch((err) => console.warn('[ContextBraid][BG] executeScript skip:', err));
        }
      }
    } catch (err) {
      console.warn('[ContextBraid][BG] reinjectOpenTabs error:', err);
    }
  };

  browser.runtime.onInstalled.addListener(() => {
    reinjectOpenTabs();
  });

  const handleDaemonResponse = async (resp: unknown) => {
    const msg = resp as {
      type?: string;
      commands?: Array<{ action: string; window_id: number; tab_id: number }>;
    };
    if (msg?.type === 'browser_command' && Array.isArray(msg.commands)) {
      for (const cmd of msg.commands) {
        if (cmd.action === 'focus_tab' && cmd.tab_id > 0) {
          try {
            await browser.tabs.get(cmd.tab_id);
            if (cmd.window_id > 0) {
              await browser.windows.update(cmd.window_id, { focused: true, state: 'normal' });
            }
            await browser.tabs.update(cmd.tab_id, { active: true });
          } catch {}
        }
      }
    }
  };

  const ensurePort = () => {
    if (!port) {
      console.info('[ContextBraid][BG] Connecting native host:', HOST_NAME);
      port = browser.runtime.connectNative(HOST_NAME);
      port.onMessage.addListener(handleDaemonResponse);
      port.onDisconnect.addListener(() => {
        const err = browser.runtime.lastError;
        console.error('[ContextBraid][BG] Native host disconnected:', err?.message ?? 'unknown');
        port = null;
      });
    }
    return port;
  };

  const flushTabs = () => {
    if (tabCache.size === 0) return;
    try {
      ensurePort().postMessage({
        type: 'sync_tabs',
        browser: browserName,
        tabs: Array.from(tabCache.values()),
      });
    } catch (err) {
      console.error('[ContextBraid][BG] postMessage error:', err);
      port = null;
    }
  };

  browser.runtime.onMessage.addListener((msg, sender) => {
    if (msg?.type === 'TAB_CONTEXT_UPDATE' && sender.tab?.id != null) {
      console.info('[ContextBraid][BG] RX content update', {
        tabId: sender.tab.id,
        incognito: sender.tab.incognito,
        status: msg.status,
        title: msg.title,
      });
      tabCache.set(sender.tab.id, {
        browser: (msg.browserName as string) || browserName,
        window_id: sender.tab.windowId ?? -1,
        tab_id: sender.tab.id,
        opener_tab_id: sender.tab.openerTabId ?? null,
        incognito: sender.tab.incognito ?? false,
        url: msg.url,
        title: msg.title,
        context_snippet: msg.contextSnippet,
        status: msg.status,
        updated_at_ms: Date.now(),
      });
      flushTabs();
    }
  });

  browser.tabs.onRemoved.addListener((tabId) => {
    tabCache.delete(tabId);
    flushTabs();
  });

  reinjectOpenTabs();
  setInterval(flushTabs, 1500);
});
