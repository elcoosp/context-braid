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
    ["iframe[src*='", "https", ":", "//", "challenges", ".cloudflare.com", "']"].join(""),
    '[data-captcha]',
  ],
  workingSelectors: [
    '[data-slot="agent-thinking"]', 'button[aria-label="Stop"]', '.terminal-loading',
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

function isVisible(el: Element | null): boolean {
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
  const lastUserBubble = userBubbles.length > 0 ? userBubbles[userBubbles.length - 1] : null;

  const alertBanners = doc.querySelectorAll('main [role="alert"], main [class*="border-red-"]');
  for (let i = alertBanners.length - 1; i >= 0; i--) {
    const el = alertBanners[i];
    if (!isVisible(el)) continue;
    if (el.closest('[data-user-bubble="true"], pre, code')) continue;
    if (!isAfterLastUserBubble(el, lastUserBubble)) continue;
    const text = (el.textContent || '').trim();
    if (text.length > 0 && text.length < 280) {
      return true;
    }
  }

  const candidates = doc.querySelectorAll('main div, main p, main span');
  for (let i = candidates.length - 1; i >= 0 && i >= candidates.length - 50; i--) {
    const el = candidates[i];
    if (!isVisible(el)) continue;
    if (el.closest('[data-user-bubble="true"], pre, code')) continue;
    if (!isAfterLastUserBubble(el, lastUserBubble)) continue;
    const text = (el.textContent || '').trim();
    if (text.length > 0 && text.length < 280 && INLINE_ERROR_PATTERNS.some((re) => re.test(text))) {
      return true;
    }
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
      const href = el.getAttribute('href') || (el as HTMLAnchorElement).href || '';
      return href.startsWith('http') || href.startsWith('blob:') ? href : '';
    }
  }
  const items = doc.querySelectorAll('li.group\\/item span');
  for (const span of items) {
    if (span.textContent?.trim() === '· zip') {
      const link = span.closest('li')?.querySelector('a[href]') as HTMLAnchorElement | null;
      const href = link?.href || '';
      return href.startsWith('http') || href.startsWith('blob:') ? href : '';
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
