import { Icon, Image } from '@raycast/api';

export const BRAND_ICONS: Record<string, Image.ImageLike> = {
  zed: 'brand-zed.png',
  zai: 'brand-zai.png',
  qwen: 'brand-qwen.png',
  gemini: 'brand-gemini.png',
  deepseek: 'brand-deepseek.png',
  browser: Icon.Globe,
  alert: 'badge-alert.png',
  completed: 'badge-completed.png',
};

export function browserBadge(browserName: string): { icon: Image.ImageLike; label: string } {
  const b = browserName.toLowerCase();
  if (b === 'brave') return { label: 'Brave', icon: 'brand-brave.png' };
  if (b === 'chrome') return { label: 'Chrome', icon: 'brand-chrome.png' };
  if (b === 'firefox') return { label: 'Firefox', icon: 'brand-firefox.png' };
  if (b === 'editor' || b === 'zed') return { label: 'Zed', icon: 'brand-zed.png' };
  return { label: browserName, icon: Icon.Globe };
}
