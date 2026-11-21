import { AI_MATCH_PATTERNS } from '../../src/core/origins';

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
  const ALARM_NAME = 'contextbraid-poll-bg-tabs';
  const browserName = detectRuntimeBrowser();
  let port: ReturnType<typeof browser.runtime.connectNative> | null = null;
  const tabCache = new Map<number, Record<string, unknown>>();
  const pollDebounce = new Map<number, ReturnType<typeof setTimeout>>();
  const downloadedZips = new Set<string>();
  let autoDownloadZip = false;

  const handleDaemonResponse = async (resp: unknown) => {
    const msg = resp as {
      type?: string;
      auto_download_zip?: boolean;
      commands?: Array<{ action: string; window_id: number; tab_id: number }>;
    };
    if (msg?.type === 'sync_config' && typeof msg.auto_download_zip === 'boolean') {
      autoDownloadZip = msg.auto_download_zip;
      return;
    }
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
      port = browser.runtime.connectNative(HOST_NAME);
      port.onMessage.addListener(handleDaemonResponse);
      port.onDisconnect.addListener(() => {
        port = null;
      });
    }
    return port;
  };

  const flushTabs = () => {
    if (tabCache.size === 0) return;
    const now = Date.now();
    const freshTabs = Array.from(tabCache.values()).map((t) => ({
      ...t,
      updated_at_ms: now,
    }));
    try {
      ensurePort().postMessage({
        type: 'sync_tabs',
        browser: browserName,
        tabs: freshTabs,
      });
    } catch {
      port = null;
    }
  };

  const notifyFocused = (windowId?: number, tabId?: number) => {
    if (windowId == null || tabId == null) return;
    try {
      ensurePort().postMessage({
        type: 'notify_tab_focused',
        browser: browserName,
        window_id: windowId,
        tab_id: tabId,
      });
    } catch {
      port = null;
    }
  };

  const triggerZipDownload = (tabId: number, zipUrl: string, dedupKey: string) => {
    if (!autoDownloadZip || downloadedZips.has(dedupKey)) return;
    downloadedZips.add(dedupKey);
    if (zipUrl && zipUrl.startsWith('http') && browser.downloads?.download) {
      browser.downloads.download({ url: zipUrl }).catch(() => {});
    } else {
      browser.scripting
        .executeScript({
          target: { tabId },
          func: () => {
            const target =
              document.querySelector<HTMLElement>(
                "a[download$='.zip'], a[href*='.zip'], button[data-testid='download-zip']",
              ) ||
              (Array.from(document.querySelectorAll('li.group\\/item span'))
                .find((s) => s.textContent?.trim() === '· zip')
                ?.closest('li, button, a') as HTMLElement | null);
            target?.click();
          },
        })
        .catch(() => {});
    }
  };

  // Interroge directement l'API storage z.ai (`/api/v1/storage/chats/<id>/files/by-message?prefix=download`)
  // pour détecter tout `.zip` dès sa création serveur, même si la fenêtre est minimisée et que React n'a pas encore repeint le DOM !
  const probeZaiServerStorage = async (tabId: number, tabUrl?: string) => {
    if (!tabUrl || !tabUrl.includes('chat.z.ai/c/')) return;
    const chatId = tabUrl.split('/c/')[1]?.split(/[?#]/)[0];
    if (!chatId || chatId.length < 8) return;

    try {
      const apiUrl = ['https', '://', 'chat', '.z.ai', '/api/v1/storage/chats/', chatId, '/files/by-message', '?prefix=download&limit=20'].join('');
      const res = await fetch(apiUrl, { credentials: 'include' });
      if (!res.ok) return;
      const bodyText = await res.text();
      if (!bodyText.toLowerCase().includes('.zip')) return;

      let zipDownloadUrl = '';
      try {
        const parsed = JSON.parse(bodyText);
        const flat = JSON.stringify(parsed);
        const urlMatch = flat.match(/https?:\/\/[^"'\s]+\.zip[^"'\s]*/i);
        if (urlMatch) {
          zipDownloadUrl = urlMatch[0];
        }
      } catch {}

      const existing = tabCache.get(tabId);
      if (existing) {
        const dedupKey = `${tabId}|${zipDownloadUrl || chatId}`;
        triggerZipDownload(tabId, zipDownloadUrl, dedupKey);
        if (existing.status !== 'completed') {
          tabCache.set(tabId, {
            ...existing,
            status: 'completed',
            updated_at_ms: Date.now(),
          });
          flushTabs();
        }
      }
    } catch {}
  };

  const probeTab = async (tabId: number, tabUrl?: string) => {
    try {
      const files =
        browser.runtime.getManifest().content_scripts?.at(0)?.js ?? ['content-scripts/content.js'];
      await browser.scripting.executeScript({
        target: { tabId },
        files,
      });
    } catch {}
    await probeZaiServerStorage(tabId, tabUrl);
  };

  const probeAllOpenAiTabs = async () => {
    try {
      const tabs = await browser.tabs.query({});
      const openIds = new Set<number>();
      for (const tab of tabs) {
        if (tab.id != null && isSupportedAiUrl(tab.url)) {
          openIds.add(tab.id);
          void probeTab(tab.id, tab.url);
        }
      }
      for (const cachedId of tabCache.keys()) {
        if (!openIds.has(cachedId)) {
          tabCache.delete(cachedId);
        }
      }
      flushTabs();
    } catch {}
  };

  browser.tabs.onActivated.addListener((activeInfo) => {
    notifyFocused(activeInfo.windowId, activeInfo.tabId);
  });

  browser.windows.onFocusChanged.addListener(async (windowId) => {
    if (windowId === browser.windows.WINDOW_ID_NONE) return;
    try {
      const [activeTab] = await browser.tabs.query({ active: true, windowId });
      if (activeTab?.id != null) {
        notifyFocused(windowId, activeTab.id);
      }
    } catch {}
  });

  browser.tabs.onRemoved.addListener((tabId) => {
    tabCache.delete(tabId);
    flushTabs();
  });

  browser.runtime.onMessage.addListener((msg, sender) => {
    if (msg?.type === 'TAB_CONTEXT_UPDATE' && sender.tab?.id != null) {
      if (msg.status === 'completed') {
        const zipUrl = typeof msg.zipDownloadUrl === 'string' ? msg.zipDownloadUrl : '';
        const dlKey = `${sender.tab.id}|${zipUrl || msg.title || msg.contextSnippet}`;
        triggerZipDownload(sender.tab.id, zipUrl, dlKey);
      }
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

if (browser.webRequest?.onCompleted) {
browser.webRequest.onCompleted.addListener(
(details) => {
if (details.tabId <= 0) return;
const isZaiCompletionOrStorage =
details.url.includes('/files/by-message') ||
details.url.includes('/messages/batch') ||
details.url.includes('/api/chat/continue') ||
details.url.includes('/api/chat/completions');
const isQwenApi =
details.url.includes('chat.qwen.ai/api/v2/chats/') ||
details.url.includes('chat.qwen.ai/api/v2/library/list');
if (!isZaiCompletionOrStorage && !isQwenApi && details.type !== 'xmlhttprequest') {
return;
}
if (
details.url.includes('/api/') &&
(details.statusCode === 403 || details.statusCode === 429 || details.statusCode >= 500)
) {
const existingTab = tabCache.get(details.tabId);
if (existingTab) {
tabCache.set(details.tabId, {
...existingTab,
status: 'needs_attention',
updated_at_ms: Date.now(),
});
flushTabs();
}
}
const existing = pollDebounce.get(details.tabId);
if (existing) clearTimeout(existing);
pollDebounce.set(
details.tabId,
setTimeout(() => {
pollDebounce.delete(details.tabId);
const cachedUrl = tabCache.get(details.tabId)?.url as string | undefined;
void probeTab(details.tabId, cachedUrl);
}, 250),
);
},
{ urls: AI_MATCH_PATTERNS },
);
}
if (browser.alarms) {
browser.alarms.create(ALARM_NAME, { periodInMinutes: 0.1 });
browser.alarms.onAlarm.addListener((alarm) => {
if (alarm.name === ALARM_NAME) {
void probeAllOpenAiTabs();
}
});
}
browser.runtime.onInstalled.addListener(() => {
void probeAllOpenAiTabs();
});
void probeAllOpenAiTabs();
});
