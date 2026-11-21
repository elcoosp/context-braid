import { i18n } from "@lingui/core";
import { messages as enMessages } from "./locales/en/messages";
import { messages as frMessages } from "./locales/fr/messages";
import { messages as zhCnMessages } from "./locales/zh-CN/messages";
import { messages as jaMessages } from "./locales/ja/messages";
import { messages as deMessages } from "./locales/de/messages";
import { messages as esMessages } from "./locales/es/messages";
import { messages as koMessages } from "./locales/ko/messages";

const CATALOGS: Record<string, typeof enMessages> = {
  en: enMessages,
  fr: frMessages,
  "zh-CN": zhCnMessages,
  ja: jaMessages,
  de: deMessages,
  es: esMessages,
  ko: koMessages,
};

function resolveLocale(): string {
  try {
    const raw = Intl.DateTimeFormat().resolvedOptions().locale || "en";
    if (CATALOGS[raw]) return raw;
    const lower = raw.toLowerCase();
    if (lower.startsWith("zh")) return "zh-CN";
    const base = lower.split("-")[0];
    if (CATALOGS[base]) return base;
  } catch {
    // fallback to English
  }
  return "en";
}

export function initI18n(): string {
  const locale = resolveLocale();
  i18n.load(locale, CATALOGS[locale] ?? enMessages);
  i18n.activate(locale);
  return locale;
}
