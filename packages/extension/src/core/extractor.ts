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

function isVisible(el: Element | null): boolean {
  if (!el) return false;
  const htmlEl = el as HTMLElement;
  if (typeof htmlEl.offsetParent !== 'undefined') {
    return htmlEl.offsetParent !== null;
  }
  return true;
}

function hasInlineRuntimeError(doc: Document): boolean {
  const candidates = doc.querySelectorAll('main [role="alert"], main .text-red-500, main div, main p');
  for (let i = candidates.length - 1; i >= 0 && i >= candidates.length - 40; i--) {
    const el = candidates[i];
    // Ignore user prompt bubbles or code blocks
    if (el.closest('[data-user-bubble="true"], pre, code')) continue;
    const text = (el.textContent || '').trim();
    if (
      text.length > 0 &&
      text.length < 220 &&
      /oops,\s*something went wrong\.?\s*please refresh/i.test(text)
    ) {
      return true;
    }
  }
  return false;
}

export function extractPageState(
  doc: Document,
  rules: ExtractionRules = DEFAULT_RULES,
): { status: AgentStatus; contextSnippet: string } {
  const title = (doc.title || '').trim();
  const snippet = collectFastSnippet(doc, title, rules);

  for (const sel of rules.attentionSelectors) {
    if (doc.querySelector(sel)) {
      return { status: 'needs_attention', contextSnippet: snippet };
    }
  }
  if (hasInlineRuntimeError(doc)) {
    return { status: 'needs_attention', contextSnippet: snippet };
  }
  if (hasCompletedZip(doc, rules)) {
    return { status: 'completed', contextSnippet: snippet };
  }
  for (const sel of rules.workingSelectors) {
    if (isVisible(doc.querySelector(sel))) {
      return { status: 'working', contextSnippet: snippet };
    }
  }
  return { status: 'idle', contextSnippet: snippet };
}

function hasCompletedZip(doc: Document, rules: ExtractionRules): boolean {
  const loc = doc.location || (typeof window !== 'undefined' ? window.location : null);
  if (loc && loc.hostname.endsWith('google.com') && loc.pathname.startsWith('/search')) {
    return false;
  }
  for (const sel of rules.completedSelectors) {
    if (doc.querySelector(sel)) return true;
  }
  const items = doc.querySelectorAll('li.group\\/item span');
  for (const span of items) {
    if (span.textContent?.trim() === '· zip') return true;
  }
  return false;
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
