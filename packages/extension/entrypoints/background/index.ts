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
      h === ['gemini', 'google', 'com'].join('.') ||
      (h === ['www', 'google', 'com'].join('.') && u.pathname.startsWith('/search')) ||
      h === ['chat', 'deepseek', 'com'].join('.')
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
  const knownChatZips = new Map<string, Set<string>>();
  let autoDownloadZip = false;
  if (browser.storage?.local) {
    browser.storage.local.get('autoDownloadZip').then((res) => {
      if (typeof res?.autoDownloadZip === 'boolean') {
        autoDownloadZip = res.autoDownloadZip;
      }
    }).catch(() => {});
  }

  const handleDaemonResponse = async (resp: unknown) => {
    const msg = resp as {
      type?: string;
      auto_download_zip?: boolean;
      commands?: Array<{ action: string; window_id: number; tab_id: number }>;
    };
    if (msg?.type === 'sync_config' && typeof msg.auto_download_zip === 'boolean') {
      autoDownloadZip = msg.auto_download_zip;
      browser.storage?.local?.set({ autoDownloadZip: msg.auto_download_zip }).catch(() => {});
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
      browser.downloads.download({ url: zipUrl, saveAs: false }).catch(() => {});
      return;
    }
    browser.scripting
      .executeScript({
        target: { tabId },
        func: () => {
          // 1. Carte "Generated files" z.ai : `<li>` contenant `· zip` -> `<button title="Download file">`
          const zipItems = Array.from(document.querySelectorAll('li')).filter((li) =>
            Array.from(li.querySelectorAll('span')).some((s) => s.textContent?.trim() === '· zip'),
          );
          if (zipItems.length > 0) {
            const latestLi = zipItems.at(-1);
            const dlBtn = latestLi?.querySelector<HTMLElement>('button[title="Download file"]');
            if (dlBtn) {
              dlBtn.click();
              return;
            }
          }

          // 2. Lien artefact inline dans la bulle assistant : `a[data-sandbox-file$='.zip']`
          const sandboxLinks = document.querySelectorAll<HTMLElement>(
            "a[data-sandbox-file$='.zip'], a[data-sandbox-file*='.zip']",
          );
          if (sandboxLinks.length > 0) {
            sandboxLinks.item(sandboxLinks.length - 1)?.click();
            return;
          }

          // 3. Fallback générique
          document
            .querySelector<HTMLElement>("a[download$='.zip'], a[href*='.zip'], button[data-testid='download-zip']")
            ?.click();
        },
      })
      .catch(() => {});
  };

  // Interroge l'API storage & subagents z.ai DANS le contexte de l'onglet (avec localStorage.token)
  // pour détecter tout `.zip` dès sa création serveur même lorsque la fenêtre est minimisée !
  const probeZaiServerStorage = async (tabId: number, tabUrl?: string) => {
    const resolvedUrl = tabUrl || (tabCache.get(tabId)?.url as string | undefined) || '';
    if (!resolvedUrl.includes('chat.z.ai/c/')) return;
    const chatId = new URL(resolvedUrl).pathname.match(/^\/c\/([^/?#]+)/)?.at(1);
    if (!chatId || chatId.length < 8) return;

    try {
      const execResults = await browser.scripting.executeScript({
        target: { tabId },
        func: async (cid: string) => {
          const token = localStorage.getItem('token') || '';
          const headers: Record<string, string> = token ? { Authorization: `Bearer ${token}` } : {};
          const [subRes, storageRes] = await Promise.all([
            fetch(`/api/v1/chats/subagents?chat_id=${cid}`, { credentials: 'include', headers })
              .then((r) => (r.ok ? r.json() : null))
              .catch(() => null),
            fetch(`/api/v1/storage/chats/${cid}/files/by-message?prefix=download&limit=20`, {
              credentials: 'include',
              headers,
            })
              .then((r) => (r.ok ? r.text() : ''))
              .catch(() => ''),
          ]);
          let signedZipUrl = '';
          try {
            const parsed = JSON.parse(storageRes);
            const files = (parsed?.data?.messages || []).flatMap((m: { files?: Array<{ key?: string; url?: string; download_url?: string }> }) => m.files || []);
            const zipEntries = files.filter((f) => String(f.key || '').toLowerCase().endsWith('.zip'));
            const lastZip = zipEntries.at(-1);
            if (lastZip) {
              if (lastZip.download_url || lastZip.url) {
                signedZipUrl = String(lastZip.download_url || lastZip.url);
              } else if (lastZip.key) {
                const dlRes = await fetch(
                  `/api/v1/storage/chats/${cid}/files/download?key=${encodeURIComponent(lastZip.key)}`,
                  { credentials: 'include', headers },
                );
                if (dlRes.ok) {
                  const dlText = await dlRes.text();
                  const m = dlText.match(/https?:\/\/[^"'\s]+/i);
                  if (m) signedZipUrl = m.at(0) ?? '';
                }
              }
            }
          } catch {}
          return { subRes, storageRes, signedZipUrl };
        },
        args: [chatId],
      });

      const payload = execResults?.at(0)?.result as

        | { subRes: { data?: Array<{ description?: string; subagent_type?: string }> } | null; storageRes: string; signedZipUrl?: string }
        | undefined;
      if (!payload) return;

      const { subRes, storageRes, signedZipUrl } = payload;
      const existing = tabCache.get(tabId);
      if (!existing) return;

      if (existing.status === 'working' && Array.isArray(subRes?.data) && subRes.data.length > 0) {
        const latestSub = subRes.data.at(-1);
        const subDesc = String(latestSub?.description || latestSub?.subagent_type || '').trim();
        const curSnippet = String(existing.context_snippet || '');
        if (subDesc && (curSnippet.startsWith('Thinking') || !curSnippet.includes(' — '))) {
          const base = curSnippet.includes(' — ') ? curSnippet.split(' — ').slice(1).join(' — ') : curSnippet;
          tabCache.set(tabId, {
            ...existing,
            context_snippet: `${subDesc} — ${base}`,
            updated_at_ms: Date.now(),
          });
          flushTabs();
        }
      }

      if (storageRes && storageRes.toLowerCase().includes('.zip')) {
        const zipKeys = Array.from(
          new Set(
            (storageRes.match(/[A-Za-z0-9_./-]+\.zip/gi) ?? []).map((k) => k.toLowerCase()),
          ),
        );
        let seenForChat = knownChatZips.get(chatId);
        let newlyCreatedZips: string[] = [];

        if (!seenForChat) {
          seenForChat = new Set(zipKeys);
          knownChatZips.set(chatId, seenForChat);
          // Si l'onglet est déjà `working` lors du premier scan, le `.zip` vient d'être produit par la tâche en cours !
          if (existing.status === 'working') {
            newlyCreatedZips = zipKeys;
          }
        } else {
          newlyCreatedZips = zipKeys.filter((k) => !seenForChat!.has(k));
          for (const k of newlyCreatedZips) {
            seenForChat.add(k);
          }
        }

        if (newlyCreatedZips.length > 0) {
          let zipDownloadUrl = signedZipUrl || '';
          const urlMatch = storageRes.match(/https?:\/\/[^"'\s]+\.zip[^"'\s]*/i);
          if (urlMatch) {
            zipDownloadUrl = urlMatch.at(0) ?? '';
          }
          const latestZipKey = newlyCreatedZips.at(-1) ?? chatId;
          const dedupKey = `${tabId}|${zipDownloadUrl || latestZipKey}`;
          triggerZipDownload(tabId, zipDownloadUrl, dedupKey);
          const latestCached = tabCache.get(tabId) ?? existing;
          tabCache.set(tabId, {
            ...latestCached,
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
