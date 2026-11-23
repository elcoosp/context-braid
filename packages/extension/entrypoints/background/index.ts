import { isAiGenerationRequest, isDirectZipDownloadUrl, validateZipDownloadPayload } from '../../src/core/extractor';
import { AI_HOST_PERMISSIONS, AI_MATCH_PATTERNS } from '../../src/core/origins';

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
  const activeNetworkRequests = new Map<number, Set<string>>();

  const markNetworkRequestStart = (tabId: number, requestId: string) => {
    let set = activeNetworkRequests.get(tabId);
    if (!set) {
      set = new Set();
      activeNetworkRequests.set(tabId, set);
    }
    set.add(requestId);
    const existing = tabCache.get(tabId);
    if (existing && existing.status !== 'working') {
      tabCache.set(tabId, {
        ...existing,
        status: 'working',
        updated_at_ms: Date.now(),
      });
      flushTabs();
    }
  };

  const markNetworkRequestEnd = (tabId: number, requestId: string) => {
    const set = activeNetworkRequests.get(tabId);
    if (!set) return;
    set.delete(requestId);
    if (set.size === 0) {
      activeNetworkRequests.delete(tabId);
      const existing = tabCache.get(tabId);
      if (existing && existing.status === 'working') {
        tabCache.set(tabId, {
          ...existing,
          status: 'idle',
          updated_at_ms: Date.now(),
        });
        flushTabs();
      }
    }
  };
  const downloadedZips = new Set<string>();
  const lastTabZipDownloadAt = new Map<number, number>();
  const knownChatZips = new Map<string, Set<string>>();
  const lastServerProbeAt = new Map<number, number>();
  let autoDownloadZip = false;

  const persistDownloadedZips = () => {
    const recent = Array.from(downloadedZips).slice(-200);
    browser.storage?.local?.set({ downloadedZips: recent }).catch(() => {});
  };

  if (browser.storage?.local) {
    browser.storage.local.get(['autoDownloadZip', 'downloadedZips']).then((res) => {
      if (typeof res?.autoDownloadZip === 'boolean') {
        autoDownloadZip = res.autoDownloadZip;
      }
      if (Array.isArray(res?.downloadedZips)) {
        for (const item of res.downloadedZips) {
          if (typeof item === 'string') downloadedZips.add(item);
        }
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

  interface WindowViewMeta {
    activeTabId: number;
    visible: boolean;
    bounds: { left: number; top: number; width: number; height: number } | null;
  }
  const windowViews = new Map<number, WindowViewMeta>();

  const refreshWindowViews = async () => {
    try {
      const [wins, activeTabs] = await Promise.all([
        browser.windows.getAll({ populate: false }),
        browser.tabs.query({ active: true }),
      ]);
      const activeByWin = new Map<number, number>();
      for (const at of activeTabs) {
        if (typeof at.windowId === 'number' && typeof at.id === 'number') {
          activeByWin.set(at.windowId, at.id);
        }
      }
      windowViews.clear();
      for (const w of wins) {
        if (typeof w.id !== 'number') continue;
        const activeTabId = activeByWin.get(w.id) ?? -1;
        const visible = w.state !== 'minimized' && activeTabId > 0;
        const bounds =
          typeof w.left === 'number' &&
          typeof w.top === 'number' &&
          typeof w.width === 'number' &&
          typeof w.height === 'number'
            ? { left: w.left, top: w.top, width: w.width, height: w.height }
            : null;
        windowViews.set(w.id, { activeTabId, visible, bounds });
      }
    } catch {
      // Keep last known window views on transient query errors
    }
  };

  const flushTabs = () => {
    if (tabCache.size === 0) return;
    void refreshWindowViews().then(() => {
      if (tabCache.size === 0) return;
      const now = Date.now();
      const freshTabs = Array.from(tabCache.values()).map((t) => {
        const winId = typeof t.window_id === 'number' ? t.window_id : -1;
        const tabId = typeof t.tab_id === 'number' ? t.tab_id : -1;
        const view = windowViews.get(winId);
        const isVisibleActiveTab = Boolean(view && view.visible && view.activeTabId === tabId);
        return {
          ...t,
          is_focused: isVisibleActiveTab,
          window_bounds: view?.bounds ?? null,
          updated_at_ms: now,
        };
      });
      try {
        ensurePort().postMessage({
          type: 'sync_tabs',
          browser: browserName,
          tabs: freshTabs,
        });
      } catch {
        port = null;
      }
    });
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
    const now = Date.now();
    const normalizedKey = dedupKey.toLowerCase().trim();
    if (!autoDownloadZip || downloadedZips.has(normalizedKey)) return;
    if (now - (lastTabZipDownloadAt.get(tabId) ?? 0) < 30_000) return;
    lastTabZipDownloadAt.set(tabId, now);
    downloadedZips.add(normalizedKey);
    persistDownloadedZips();

    if (zipUrl && isDirectZipDownloadUrl(zipUrl) && typeof browser.downloads?.download === 'function') {
      triggerBackgroundZipDownload(zipUrl, { source: 'triggerZipDownload', tabId });
      return;
    }

    browser.scripting
      .executeScript({
        target: { tabId },
        func: () => {
          // 1. Si une modale d'aperçu est déjà ouverte avec un `.zip`, cliquer sur son bouton `aria-label="Download"`
          const modalDlBtn = document.querySelector<HTMLElement>('button[aria-label="Download"]');
          if (modalDlBtn) {
            modalDlBtn.click();
            return 'clicked_modal_download';
          }

          // 2. Liste "Generated files" z.ai : `<li>` contenant `· zip` -> `<button title="Download file">`
          const zipItems = Array.from(document.querySelectorAll('li')).filter((li) =>
            Array.from(li.querySelectorAll('span')).some((s) => /·\s*zip/i.test(s.textContent || '')),
          );
          if (zipItems.length > 0) {
            const latestLi = zipItems.at(-1);
            const dlBtn = latestLi?.querySelector<HTMLElement>('button[title="Download file"]');
            if (dlBtn) {
              dlBtn.click();
              return 'clicked_list_download_button';
            }
          }

          // 3. Fallback générique uniquement sur de vrais attributs `download` (jamais `a[data-sandbox-file]` qui ouvre l'aperçu !)
          const direct = document.querySelector<HTMLElement>("a[download$='.zip'], button[data-testid='download-zip']");
          if (direct) {
            direct.click();
            return 'clicked_direct_download';
          }

          return 'not_ready_yet';
        },
      })
      .then((res) => {
        // Si React n'avait pas encore monté le `<li>` `· zip` (ex: fenêtre minimisée), retenter une fois après 1.5s
        if (res?.at(0)?.result === 'not_ready_yet') {
          setTimeout(() => {
            browser.scripting
              .executeScript({
                target: { tabId },
                func: () => {
                  const zipItems = Array.from(document.querySelectorAll('li')).filter((li) =>
                    Array.from(li.querySelectorAll('span')).some((s) => /·\s*zip/i.test(s.textContent || '')),
                  );
                  const dlBtn = zipItems.at(-1)?.querySelector<HTMLElement>('button[title="Download file"]');
                  dlBtn?.click();
                },
              })
              .catch(() => {});
          }, 1500);
        }
      })
      .catch(() => {});
  };

  // Interroge l'API storage & subagents z.ai DANS le contexte de l'onglet (avec localStorage.token)
  // pour détecter tout `.zip` dès sa création serveur même lorsque la fenêtre est minimisée !
    const pendingZipRetries = new Map<string, ReturnType<typeof setTimeout>>();
  const awaitingZipTabs = new Map<number, { chatId: string; baselineCount: number; attempts: number }>();
  const countDownloadedForTab = (tabId: number) =>
    Array.from(downloadedZips).filter((k) => k.startsWith(`${tabId}|http`)).length;

  const scheduleBackgroundZipRetry = (tabId: number, chatId: string, attempt = 0) => {
    const retryKey = `${tabId}|${chatId}`;
    const state = awaitingZipTabs.get(tabId) ?? {
      chatId,
      baselineCount: countDownloadedForTab(tabId),
      attempts: 0,
    };
    state.chatId = chatId;
    state.attempts = attempt;
    awaitingZipTabs.set(tabId, state);

    if (pendingZipRetries.has(retryKey) || attempt >= 6) {
      if (attempt >= 6) awaitingZipTabs.delete(tabId);
      return;
    }
    const delays = Array.from({ length: 6 }, (_, i) => (i + 1) * 2500);
    const timer = setTimeout(() => {
      pendingZipRetries.delete(retryKey);
      const cached = tabCache.get(tabId);
      if (!cached) {
        awaitingZipTabs.delete(tabId);
        return;
      }
      void probeZaiServerStorage(tabId, typeof cached.url === 'string' ? cached.url : undefined, true).then(() => {
        const cur = awaitingZipTabs.get(tabId);
        if (!cur) return;
        if (countDownloadedForTab(tabId) > cur.baselineCount) {
          awaitingZipTabs.delete(tabId);
        } else {
          scheduleBackgroundZipRetry(tabId, chatId, attempt + 1);
        }
      });
    }, delays[attempt] ?? 5000);
    pendingZipRetries.set(retryKey, timer);
  };

  const probeZaiServerStorage = async (tabId: number, tabUrl?: string, bypassThrottle = false) => {
    const resolvedUrl = tabUrl || (tabCache.get(tabId)?.url as string | undefined) || '';
    if (!resolvedUrl.includes('chat.z.ai/c/')) return;
    const now = Date.now();
    if (!bypassThrottle && now - (lastServerProbeAt.get(tabId) ?? 0) < 6000) return;
    lastServerProbeAt.set(tabId, now);
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
            const zipEntries = files.filter((f: { key?: string }) => String(f.key || '').toLowerCase().endsWith('.zip'));
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
      const existing = tabCache.get(tabId) ?? {
        browser: detectRuntimeBrowser(),
        window_id: 0,
        tab_id: tabId,
        url: resolvedUrl,
        title: '',
        status: 'idle',
        context_snippet: '',
        incognito: false,
        updated_at_ms: Date.now(),
      };

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
          if (zipDownloadUrl) {
            const dedupKey = `${tabId}|${zipDownloadUrl}`;
            triggerZipDownload(tabId, zipDownloadUrl, dedupKey);
          } else {
            scheduleBackgroundZipRetry(tabId, chatId);
          }
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
      await browser.tabs.sendMessage(tabId, { type: 'PING_EXTRACT' });
    } catch {
      try {
        const manifestScripts = browser.runtime.getManifest().content_scripts?.at(0)?.js;
        await browser.scripting.executeScript({
          target: { tabId },
          files: (manifestScripts && manifestScripts.length > 0 ? manifestScripts : ['content-scripts/content.js']) as never,
        });
      } catch {}
    }
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
    flushTabs();
  });

  browser.windows.onFocusChanged.addListener(async (windowId) => {
    if (windowId === browser.windows.WINDOW_ID_NONE) return;
    try {
      const [activeTab] = await browser.tabs.query({ active: true, windowId });
      if (activeTab?.id != null) {
        notifyFocused(windowId, activeTab.id);
      }
      flushTabs();
    } catch {}
  });

  browser.tabs.onRemoved.addListener((tabId) => {
    activeNetworkRequests.delete(tabId);
    tabCache.delete(tabId);
    flushTabs();
  });

  
const recentDownloadUrls = new Map<string, number>();
const LOG_STORAGE_KEY = "cb_autodownload_logs";

const pendingAutoDownloadLogs: Record<string, unknown>[] = [];
let logFlushTimer: ReturnType<typeof setTimeout> | null = null;

async function flushAutoDownloadLogs() {
  logFlushTimer = null;
  if (pendingAutoDownloadLogs.length === 0 || !browser.storage?.local) return;
  const batch = pendingAutoDownloadLogs.splice(0, pendingAutoDownloadLogs.length);
  try {
    const data = await browser.storage.local.get(LOG_STORAGE_KEY);
    const list = Array.isArray(data?.[LOG_STORAGE_KEY]) ? data[LOG_STORAGE_KEY] : [];
    list.push(...batch);
    await browser.storage.local.set({ [LOG_STORAGE_KEY]: list.slice(-50) });
  } catch {
    // ignore storage errors
  }
}

function appendAutoDownloadLog(entry: Record<string, unknown>) {
  console.info("[ContextBraid:AutoDownload:BG]", entry);
  pendingAutoDownloadLogs.push(entry);
  if (!logFlushTimer) {
    logFlushTimer = setTimeout(() => {
      void flushAutoDownloadLogs();
    }, 1500);
  }
}

async function verifyRemoteZipBeforeDownload(url: string): Promise<{ valid: boolean; reason: string }> {
  if (!isDirectZipDownloadUrl(url)) {
    return { valid: false, reason: 'blocked_spa_internal_path' };
  }
  try {
    const controller = new AbortController();
    const timer = setTimeout(() => controller.abort(), 5000);
    const res = await fetch(url, {
      method: 'GET',
      credentials: 'include',
      headers: { Range: 'bytes=0-63' },
      signal: controller.signal,
    });
    clearTimeout(timer);
    if (!res.ok) {
      return { valid: false, reason: `http_status_${res.status}` };
    }
    const mime = res.headers.get('content-type') || '';
    const buf = new Uint8Array(await res.arrayBuffer());
    return validateZipDownloadPayload({ url, mime, firstBytes: buf });
  } catch {
    return { valid: true, reason: 'cors_or_network_bypass' };
  }
}

function triggerBackgroundZipDownload(
  url: string,
  meta: { pageUrl?: string; hidden?: boolean; source?: string; tabId?: number } = {},
) {
  if (!isDirectZipDownloadUrl(url)) {
    void appendAutoDownloadLog({
      ts: new Date().toISOString(),
      source: "background",
      event: "rejected_non_zip_url",
      url,
      ...meta,
    });
    return;
  }
  const now = Date.now();
  const prev = recentDownloadUrls.get(url);
  if (prev && now - prev < 60_000) {
    void appendAutoDownloadLog({
      ts: new Date().toISOString(),
      source: "background",
      event: "duplicate_skipped",
      url,
      ...meta,
    });
    return;
  }
  recentDownloadUrls.set(url, now);

  if (typeof browser.downloads?.download !== 'function') {
    void appendAutoDownloadLog({
      ts: new Date().toISOString(),
      source: "background",
      event: "downloads_api_unavailable",
      url,
      ...meta,
    });
    return;
  }

  void verifyRemoteZipBeforeDownload(url).then((check) => {
    if (!check.valid) {
      void appendAutoDownloadLog({
        ts: new Date().toISOString(),
        source: "background",
        event: "rejected_invalid_zip_payload",
        reason: check.reason,
        url,
        ...meta,
      });
      return;
    }

    void browser.downloads
      .download({
        url,
        conflictAction: "uniquify",
        saveAs: false,
      })
    .then((downloadId) => {
      void appendAutoDownloadLog({
        ts: new Date().toISOString(),
        source: "background",
        event: "download_started",
        downloadId,
        url,
        ...meta,
      });
    })
      .catch((err: unknown) => {
        void appendAutoDownloadLog({
          ts: new Date().toISOString(),
          source: "background",
          event: "download_failed",
          error: String(err),
          url,
          ...meta,
        });
      });
  });
}

if (browser.downloads?.onChanged) {
  browser.downloads.onChanged.addListener((delta) => {
    const mime = (delta.mime?.current || '').toLowerCase();
    if (mime.includes('text/html') || mime.includes('application/xhtml')) {
      void appendAutoDownloadLog({
        ts: new Date().toISOString(),
        source: 'background',
        event: 'cancelled_html_download',
        downloadId: delta.id,
        mime,
      });
      browser.downloads.cancel(delta.id).catch(() => {});
      browser.downloads.erase({ id: delta.id }).catch(() => {});
    }
  });
}

browser.runtime.onMessage.addListener((msg, sender) => {
    if (msg?.type === 'AUTO_DOWNLOAD_LOG' && msg.entry) {
    void appendAutoDownloadLog(msg.entry);
    return;
  }
  if (msg?.type === 'AUTO_DOWNLOAD_ZIP' && typeof msg.url === 'string') {
    triggerBackgroundZipDownload(msg.url, {
      pageUrl: typeof msg.pageUrl === 'string' ? msg.pageUrl : undefined,
      hidden: typeof msg.hidden === 'boolean' ? msg.hidden : undefined,
    });
    return;
  }
  if (msg?.type === 'TAB_CONTEXT_UPDATE' && sender.tab?.id != null) {
      if (msg.status === 'completed') {
        const zipUrl = typeof msg.zipDownloadUrl === 'string' ? msg.zipDownloadUrl : '';
        const tabUrl = typeof msg.url === 'string' ? msg.url : '';
        const chatId = tabUrl.match(/\/c\/([^/?#]+)/)?.at(1) || '';
        const dlKey = `${sender.tab.id}|${zipUrl || chatId || msg.title || msg.contextSnippet}`;
        triggerZipDownload(sender.tab.id, zipUrl, dlKey);
      }
      const hasActiveNetTurn = (activeNetworkRequests.get(sender.tab.id)?.size ?? 0) > 0;
      const resolvedStatus =
        hasActiveNetTurn && msg.status !== 'needs_attention' ? 'working' : msg.status;
      tabCache.set(sender.tab.id, {
        browser: (msg.browserName as string) || browserName,
        window_id: sender.tab.windowId ?? -1,
        tab_id: sender.tab.id,
        opener_tab_id: sender.tab.openerTabId ?? null,
        incognito: sender.tab.incognito ?? false,
        url: msg.url,
        title: msg.title,
        context_snippet: msg.contextSnippet,
        status: resolvedStatus,
        updated_at_ms: Date.now(),
      });
      flushTabs();
    }
  });

if (browser.webRequest?.onBeforeRequest) {
browser.webRequest.onBeforeRequest.addListener(
(details) => {
if (details.tabId <= 0) return;
if (isAiGenerationRequest(details.method, details.url)) {
markNetworkRequestStart(details.tabId, details.requestId);
}
},
{ urls: AI_HOST_PERMISSIONS },
);
}
if (browser.webRequest?.onErrorOccurred) {
browser.webRequest.onErrorOccurred.addListener(
(details) => {
if (details.tabId <= 0) return;
markNetworkRequestEnd(details.tabId, details.requestId);
},
{ urls: AI_HOST_PERMISSIONS },
);
}
if (browser.webRequest?.onCompleted) {
browser.webRequest.onCompleted.addListener(
(details) => {
if (details.tabId <= 0) return;
markNetworkRequestEnd(details.tabId, details.requestId);
const isZaiCompletionOrStorage =
details.url.includes('/files/by-message') ||
details.url.includes('/messages/batch') ||
details.url.includes('/api/chat/continue') ||
details.url.includes('/api/chat/completions') ||
details.url.includes('/api/v1/chats/?page=');
const isQwenApi =
details.url.includes('/api/v2/chat/completions') ||
details.url.includes('chat.qwen.ai/api/v2/chats/') ||
details.url.includes('chat.qwen.ai/api/v2/library/list');
const isGoogleAi =
details.url.includes('/async/folif') ||
details.url.includes('/async/folic');
if (!isZaiCompletionOrStorage && !isQwenApi && !isGoogleAi && details.type !== 'xmlhttprequest') {
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
{ urls: AI_HOST_PERMISSIONS },
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
