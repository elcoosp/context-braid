import { i18n } from "@lingui/core";
import { getPreferenceValues } from "@raycast/api";
import { messages as messagesEn } from "./locales/en/messages";
import { messages as messagesFr } from "./locales/fr/messages";

export type SupportedLocale = "en" | "fr";

i18n.load({
  en: messagesEn,
  fr: messagesFr,
});

export function detectLocale(): SupportedLocale {
  try {
    const prefs = getPreferenceValues<{ language?: string }>();
    if (prefs.language === "fr" || prefs.language === "en") {
      return prefs.language;
    }
  } catch {
    // fallback to English locale
  }
  const sys = Intl.DateTimeFormat().resolvedOptions().locale.toLowerCase();
  return sys.startsWith("fr") ? "fr" : "en";
}

i18n.activate(detectLocale());

export { i18n };
