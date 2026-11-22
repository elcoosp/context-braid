export type AgentStatus = 'idle' | 'working' | 'needs_attention' | 'completed';

export interface ExtractionRules {
  attentionSelectors: string[];
  workingSelectors: string[];
  completedSelectors: string[];
  userPromptSelectors: string[];
}

export const DEFAULT_RULES: ExtractionRules = {
  attentionSelectors: [
    '#aliyunCaptcha-float-wrapper',
    '#aliyunCaptcha-captcha-body',
    ["iframe[src*='", 'https', ':', '//', 'challenges', '.cloudflare.com', "']"].join(''),
    '[data-captcha]',
  ],
  workingSelectors: [
    '[data-slot="agent-thinking"]',
    'button[aria-label="Stop"]',
    '.terminal-loading',
    'button[aria-label*="Stop" i]',
  ],
  completedSelectors: [
    "a[download$='.zip']",
    "a[href*='.zip']",
    "button[data-testid='download-zip']",
    '[data-sandbox-file$=".zip"]',
  ],
  userPromptSelectors: [
    '[data-user-bubble="true"]',
    '.VndcI',
    '[data-message-author-role="user"]',
    '[data-role="user"]',
    'nav span[title]',
  ],
};

const GENERIC_TITLES = new Set(['z.ai', 'qwen', 'gemini', 'deepseek', 'new chat', 'google']);

export function isDirectZipDownloadUrl(rawUrl: string): boolean {
  const trimmed = (rawUrl || '').trim();
  if (!trimmed || trimmed.startsWith('blob:') || trimmed.startsWith('data:')) {
    return false;
  }
  try {
    const parsed = new URL(trimmed);
    if (parsed.protocol !== 'http:' && parsed.protocol !== 'https:') {
      return false;
    }
    const host = parsed.hostname.toLowerCase();
    const path = parsed.pathname.toLowerCase();

    // Les chemins sandbox /home/z/..., /workspace/... ou /c/... sur les domaines SPA AI
    // ne sont PAS des endpoints de téléchargement HTTP : le routeur SPA renvoie index.html (18 392 octets).
    if (host === 'z.ai' || host.endsWith('.z.ai') || host === 'qwen.ai' || host.endsWith('.qwen.ai')) {
      if (path.startsWith('/home/') || path.startsWith('/workspace/') || path.startsWith('/c/')) {
        return false;
      }
      // Sur z.ai / qwen.ai, seules les routes `/api/...` (ex: /api/v1/storage/.../download) sont des téléchargements directs
      if (!path.startsWith('/api/') && path.endsWith('.zip')) {
        return false;
      }
    }

    return true;
  } catch {
    return false;
  }
}

export function validateZipDownloadPayload(input: {
  url: string;
  mime?: string;
  firstBytes?: Uint8Array;
}): { valid: boolean; reason: string } {
  const mime = (input.mime || '').toLowerCase();
  if (mime.includes('text/html') || mime.includes('application/xhtml') || mime.includes('application/json')) {
    return { valid: false, reason: `unexpected_mime:${mime}` };
  }
  const bytes = input.firstBytes;
  if (bytes && bytes.length >= 4) {
    // Magic bytes officiels PKZIP : 0x50 0x4B 0x03 0x04 ("PK\x03\x04") ou archive vide 0x50 0x4B 0x05 0x06
    const isPkHeader =
      bytes[0] === 0x50 &&
      bytes[1] === 0x4b &&
      ((bytes[2] === 0x03 && bytes[3] === 0x04) || (bytes[2] === 0x05 && bytes[3] === 0x06));
    if (!isPkHeader) {
      const headText = new TextDecoder('utf-8', { fatal: false })
        .decode(bytes.slice(0, 64))
        .trim()
        .toLowerCase();
      if (headText.startsWith('<!doctype html') || headText.startsWith('<html')) {
        return { valid: false, reason: 'html_spa_fallback_detected' };
      }
      return { valid: false, reason: 'invalid_zip_magic_bytes' };
    }
  }
  return { valid: true, reason: 'ok' };
}

function isVisible(el: Element | null | undefined): boolean {
  if (!el) return false;
  const htmlEl = el as HTMLElement;
  if (htmlEl.hidden || htmlEl.closest('[hidden], [aria-hidden="true"]')) {
    return false;
  }
  if (typeof window !== 'undefined' && typeof window.getComputedStyle === 'function') {
    const style = window.getComputedStyle(htmlEl);
    if (style.display === 'none' || style.visibility === 'hidden' || style.opacity === '0') {
      return false;
    }
  }
  if (typeof htmlEl.offsetParent !== 'undefined') {
    return htmlEl.offsetParent !== null;
  }
  return true;
}

const INLINE_ERROR_PATTERNS: RegExp[] = [
  /oops,\s*something went wrong\.?\s*please refresh/i,
  /the service was updated while this task was running/i,
  /please send your request again/i,
];

function isAfterLastUserBubble(el: Element, lastUserBubble: Element | null): boolean {
  if (!lastUserBubble) return true;
  return Boolean(
    lastUserBubble.compareDocumentPosition(el) & Node.DOCUMENT_POSITION_FOLLOWING,
  );
}

function hasInlineRuntimeError(doc: Document): boolean {
  const userBubbles = doc.querySelectorAll('main [data-user-bubble="true"]');
  const lastUserBubble = userBubbles.length > 0 ? userBubbles.item(userBubbles.length - 1) : null;

  const alertBanners = doc.querySelectorAll('main [role="alert"], main [class*="border-red-"]');
  for (let i = alertBanners.length - 1; i >= 0; i--) {
    const el = alertBanners.item(i);
    if (!el || !isVisible(el)) continue;
    if (el.closest('[data-user-bubble="true"], pre, code')) continue;
    if (!isAfterLastUserBubble(el, lastUserBubble)) continue;
    const text = (el.textContent || '').trim();
    if (text.length > 0 && text.length < 280) {
      return true;
    }
  }

  const root =
    doc.querySelector('main [data-scope-id="turn"]:last-of-type') ??
    doc.querySelector('main') ??
    doc.body;
  if (!root) return false;

  const candidates = root.querySelectorAll('div, p, span');
  for (let i = candidates.length - 1; i >= 0 && i >= candidates.length - 40; i--) {
    const el = candidates.item(i);
    if (!el) continue;
    const text = (el.textContent || '').trim();
    if (text.length === 0 || text.length >= 280) continue;
    if (!INLINE_ERROR_PATTERNS.some((re) => re.test(text))) continue;
    if (el.closest('[data-user-bubble="true"], pre, code')) continue;
    if (!isAfterLastUserBubble(el, lastUserBubble)) continue;
    if (!isVisible(el)) continue;
    return true;
  }
  return false;
}

export function extractPageState(
  doc: Document,
  rules: ExtractionRules = DEFAULT_RULES,
): { status: AgentStatus; contextSnippet: string; zipDownloadUrl?: string } {
  const title = (doc.title || '').trim();
  const snippet = collectFastSnippet(doc, title, rules);

  for (const sel of rules.attentionSelectors) {
    const nodes = doc.querySelectorAll(sel);
    for (let i = 0; i < nodes.length; i++) {
      if (isVisible(nodes.item(i))) {
        return { status: 'needs_attention', contextSnippet: snippet };
      }
    }
  }
  if (hasInlineRuntimeError(doc)) {
    return { status: 'needs_attention', contextSnippet: snippet };
  }
  for (const sel of rules.workingSelectors) {
    if (isVisible(doc.querySelector(sel))) {
      const liveStep = extractLiveAgentStep(doc);
      return {
        status: 'working',
        contextSnippet: liveStep ? `${liveStep} — ${snippet}` : snippet,
      };
    }
  }
  const zipUrl = findCompletedZipUrl(doc, rules);
  if (zipUrl !== null) {
    return {
      status: 'completed',
      contextSnippet: snippet,
      ...(zipUrl ? { zipDownloadUrl: zipUrl } : {}),
    };
  }
  return { status: 'idle', contextSnippet: snippet };
}

function findCompletedZipUrl(doc: Document, rules: ExtractionRules): string | null {
  const loc = doc.location || (typeof window !== 'undefined' ? window.location : null);
  if (loc && loc.hostname.endsWith('google.com') && loc.pathname.startsWith('/search')) {
    return null;
  }
  for (const sel of rules.completedSelectors) {
    const el = doc.querySelector(sel);
    if (el) {
      if (el.hasAttribute('data-sandbox-file')) {
        return '';
      }
      const href = el.getAttribute('href') || (el as HTMLAnchorElement).href || '';
      return isDirectZipDownloadUrl(href) ? href : '';
    }
  }
  const items = doc.querySelectorAll('li.group\\/item span');
  for (const span of items) {
    if (span.textContent?.trim() === '· zip') {
      const link = span.closest('li')?.querySelector('a[href]') as HTMLAnchorElement | null;
      const href = link?.href || '';
      return isDirectZipDownloadUrl(href) ? href : '';
    }
  }
  return null;
}

function collectFastSnippet(doc: Document, title: string, rules: ExtractionRules): string {
  const normalizedTitle = title.toLowerCase();
  const isGeneric =
    !title ||
    GENERIC_TITLES.has(normalizedTitle) ||
    normalizedTitle.startsWith('z.ai - advanced');

  const parts: string[] = [];
  let totalLen = 0;
  const pushPart = (raw: string, maxItemLen: number) => {
    if (totalLen >= 4000) return;
    const clean = raw.trim().slice(0, maxItemLen);
    if (!clean) return;
    parts.push(clean);
    totalLen += clean.length;
  };

  if (!isGeneric && !title.includes('...')) {
    pushPart(title, 160);
  }

  for (const sel of rules.userPromptSelectors) {
    const nodes = doc.querySelectorAll(sel);
    if (nodes.length > 0) {
      for (let i = 0; i < nodes.length; i++) {
        const el = nodes.item(i);
        if (!el) continue;
        pushPart(el.getAttribute('title') || el.textContent || '', 320);
      }
      break;
    }
  }

  const codes = doc.querySelectorAll('[data-scope-id="turn"] code, main code');

  for (let i = 0; i < codes.length && totalLen < 4000; i++) {
    const el = codes.item(i);
    const txt = el?.textContent?.trim() || '';
    if (txt.length >= 3 && txt.length <= 120) {
      pushPart(txt, 120);
    }
  }
  return parts.join(' | ');
}
export function extractLiveAgentStep(doc: Document): string {
  const thinkingLabels = doc.querySelectorAll('[data-slot="agent-thinking-label"]');
  for (let i = thinkingLabels.length - 1; i >= 0; i--) {
    const el = thinkingLabels.item(i);
    if (!isVisible(el)) continue;
    const raw = (el?.textContent || '').replace(/\s+/g, ' ').trim();
    if (raw.length >= 3 && raw.length <= 120) {
      return raw;
    }
  }
  return '';
}
export interface ZipDownloadTarget {
  url: string;
  element: HTMLElement;
  key: string;
}
export function findZipDownloadTargets(doc: Document): ZipDownloadTarget[] {
  const results: ZipDownloadTarget[] = [];
  const seen = new Set<string>();
  const pagePath =
    (typeof window !== 'undefined' && window.location?.pathname) || doc.location?.pathname || '/';

  const addCandidate = (el: HTMLElement | null, rawHref: string, fallbackLabel?: string) => {
    if (!el) return;
    const trimmedHref = (rawHref || '').trim();
    const validHttpUrl = isDirectZipDownloadUrl(trimmedHref) ? trimmedHref : '';
    const label = (
      fallbackLabel ||
      el.textContent ||
      el.getAttribute('aria-label') ||
      el.getAttribute('title') ||
      ''
    )
      .replace(/\s+/g, ' ')
      .trim();
    const key = validHttpUrl || `${pagePath}::${label}::${results.length}`;
    if (seen.has(key)) return;
    seen.add(key);
    results.push({ url: validHttpUrl, element: el, key });
  };

  // Single combined querySelectorAll sweep (AGENTS.md §4.3 invariant)
  const nodes = doc.querySelectorAll('li span, a[download], a[href$=".zip"], a[href*=".zip?"]');
  for (let i = 0; i < nodes.length; i++) {
    const node = nodes.item(i);
    if (!node) continue;
    if (node.tagName === 'SPAN') {
      if (/·\s*zip/i.test(node.textContent || '')) {
        const li = node.closest('li');
        const link = li?.querySelector('a[href]') as HTMLAnchorElement | null;
        const btn = (li?.querySelector('button[title="Download file"]') ||
          li?.querySelector('button') ||
          link) as HTMLElement | null;
        const itemName = (li?.textContent || '').replace(/\s+/g, ' ').trim();
        addCandidate(btn, link?.href || link?.getAttribute('href') || '', itemName);
      }
    } else if (node.tagName === 'A' && !node.hasAttribute('data-sandbox-file')) {
      const htmlEl = node as HTMLAnchorElement;
      const rawHref = htmlEl.href || htmlEl.getAttribute('href') || '';
      if (isDirectZipDownloadUrl(rawHref)) {
        addCandidate(htmlEl, rawHref);
      }
    }
  }

  return results;
}
