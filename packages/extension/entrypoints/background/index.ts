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
  const downloadedZips = new Set<string>();
  let autoDownloadZip = false;
  const pollDebounce = new Map<number, ReturnType<typeof setTimeout>>();

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

  const sendDiag = (source: string, tabId: number, detail: string) => {
    try {
      ensurePort().postMessage({
        type: 'diag_log',
        source,
        tab_id: tabId,
        detail,
      });
    } catch {}
  };

  const flushTabs = () => {
    if (tabCache.size === 0) return;
    try {
      ensurePort().postMessage({
        type: 'sync_tabs',
        browser: browserName,
        tabs: Array.from(tabCache.values()),
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

  // Réinjecte le content script dans un onglet précis (fonctionne même fenêtre minimisée !)
  const probeTab = async (tabId: number) => {
    try {
      const files =
        browser.runtime.getManifest().content_scripts?.at(0)?.js ?? ['content-scripts/content.js'];
      await browser.scripting.executeScript({
        target: { tabId },
        files,
      });
    } catch {}
  };

  const probeAllOpenAiTabs = async () => {
    try {
      const tabs = await browser.tabs.query({});
      const openIds = new Set<number>();
      for (const tab of tabs) {
        if (tab.id != null && isSupportedAiUrl(tab.url)) {
          openIds.add(tab.id);
          void probeTab(tab.id);
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

  // Listeners globaux enregistrés à chaque réveil du Service Worker MV3
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
      sendDiag(
        'CONTENT_RX',
        sender.tab.id,
        `hidden=${Boolean(msg.hidden)} status=${msg.status} zipUrl=${msg.zipDownloadUrl ?? 'none'} title=${JSON.stringify(msg.title)} snippet=${JSON.stringify(String(msg.contextSnippet ?? '').slice(0, 140))}`,
      );
      if (autoDownloadZip && msg.status === 'completed') {
        const dlKey = `${sender.tab.id}|${msg.zipDownloadUrl || msg.title || msg.contextSnippet}`;
        if (!downloadedZips.has(dlKey)) {
          downloadedZips.add(dlKey);
          const zipUrl = typeof msg.zipDownloadUrl === 'string' ? msg.zipDownloadUrl : '';
          if (zipUrl && zipUrl.startsWith('http') && browser.downloads?.download) {
            browser.downloads.download({ url: zipUrl }).catch(() => {});
          } else {
            // Déclenche le clic natif sur le bouton/artefact .zip dans l'onglet en arrière-plan
            browser.scripting
              .executeScript({
                target: { tabId: sender.tab.id },
                func: () => {
                  const target =
                    document.querySelector<HTMLElement>("a[download$='.zip'], a[href*='.zip'], button[data-testid='download-zip']") ||
                    Array.from(document.querySelectorAll('li.group\\/item span'))
                      .find((s) => s.textContent?.trim() === '· zip')
                      ?.closest('li, button, a') as HTMLElement | null;
                  target?.click();
                },
              })
              .catch(() => {});
          }
        }
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


  if (browser.webRequest?.onBeforeRequest) {
    browser.webRequest.onBeforeRequest.addListener(
      (details) => {
        if (details.tabId <= 0) return;
        if (!['image', 'stylesheet', 'font', 'script'].includes(details.type)) {
          sendDiag('NET_DONE', details.tabId, `${details.method} ${details.statusCode} [${details.type}] ${details.url}`);
        }
        if (!['image', 'stylesheet', 'font', 'script'].includes(details.type)) {
          sendDiag('NET_REQ', details.tabId, `${details.method} [${details.type}] ${details.url}`);
        }
      },
      { urls: AI_MATCH_PATTERNS },
    );
  }

  if (browser.webRequest?.onErrorOccurred) {
    browser.webRequest.onErrorOccurred.addListener(
      (details) => {
        if (details.tabId <= 0) return;
        sendDiag('NET_ERR', details.tabId, `${details.method} [${details.type}] err=${details.error} ${details.url}`);
      },
      { urls: AI_MATCH_PATTERNS },
    );
  }

  browser.tabs.onUpdated.addListener((tabId, changeInfo, tab) => {
    if (!isSupportedAiUrl(tab.url)) return;
    if (changeInfo.title || changeInfo.status || changeInfo.url) {
      sendDiag(
        'TAB_UPD',
        tabId,
        `status=${changeInfo.status ?? '-'} title=${JSON.stringify(changeInfo.title ?? tab.title ?? '')} url=${changeInfo.url ?? ''}`,
      );
    }
  });

  if (browser.downloads?.onCreated) {
    browser.downloads.onCreated.addListener((item) => {
      sendDiag('DOWNLOAD_CREATED', -1, `id=${item.id} filename=${item.filename} url=${item.url}`);
    });
  }

  // Événement réseau natif : réveille immédiatement la sonde dès qu'une requête API/SSE se termine
  // sur un onglet IA en arrière-plan ou dans une fenêtre minimisée !
  if (browser.webRequest?.onCompleted) {
    browser.webRequest.onCompleted.addListener(
      (details) => {
        if (details.tabId <= 0) return;
        
        
        

        const isStorageDownloadList = details.url.includes('/files/by-message') && details.url.includes('prefix=download');
        const isChatCompletionOrBatch =
          details.url.includes('/messages/batch') ||
          details.url.includes('/completions') ||
          details.url.includes('/api/v1/chats/');

        if (!isStorageDownloadList && !isChatCompletionOrBatch && details.type !== 'xmlhttprequest') {
          return;
        }

        // Si une requête API retourne 403 (Captcha/WAF), 429 (Rate limit) ou 5xx, alerte immédiatement
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

        const delayMs = isStorageDownloadList ? 150 : 350;
        const existing = pollDebounce.get(details.tabId);
        if (existing) clearTimeout(existing);
        pollDebounce.set(
          details.tabId,
          setTimeout(() => {
            pollDebounce.delete(details.tabId);
            void probeTab(details.tabId);
          }, delayMs),
        );
      },
      { urls: AI_MATCH_PATTERNS },
    );
  }

  // Réveil garanti toutes les 6 secondes via chrome.alarms (jamais gelé par MV3 même fenêtre minimisée)
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
