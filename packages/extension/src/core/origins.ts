const STAR = String.fromCharCode(42);
const PROTO = ['https', '://'].join('');

const ZAI_HOST = [STAR, 'z', 'ai'].join('.');
const QWEN_HOST = [STAR, 'qwen', 'ai'].join('.');
const GEMINI_HOST = ['gemini', 'google', 'com'].join('.');
const GOOGLE_HOST = ['www', 'google', 'com'].join('.');
const GOOGLE_FR_HOST = ['www', 'google', 'fr'].join('.');
const DEEPSEEK_HOST = ['chat', 'deepseek', 'com'].join('.');

export const AI_MATCH_PATTERNS: string[] = [
  `${PROTO}${ZAI_HOST}/${STAR}`,
  `${PROTO}${QWEN_HOST}/${STAR}`,
  `${PROTO}${GEMINI_HOST}/${STAR}`,
  `${PROTO}${GOOGLE_HOST}/search${STAR}`,
  `${PROTO}${GOOGLE_FR_HOST}/search${STAR}`,
  `${PROTO}${DEEPSEEK_HOST}/${STAR}`,
];

export const AI_HOST_PERMISSIONS: string[] = [
  ...AI_MATCH_PATTERNS,
  `${PROTO}${GOOGLE_HOST}/async/${STAR}`,
  `${PROTO}${GOOGLE_FR_HOST}/async/${STAR}`,
];
