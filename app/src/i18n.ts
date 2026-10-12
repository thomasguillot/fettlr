import i18next from "i18next";
import { initReactI18next } from "react-i18next";

import en from "./locales/en/common.json";
import enGB from "./locales/en-GB/common.json";
import fr from "./locales/fr/common.json";

const SUPPORTED_LANGUAGES = ["en", "en-GB", "fr"] as const;
export type SupportedLanguage = (typeof SUPPORTED_LANGUAGES)[number];

const STORAGE_KEY = "fettlr-language";

function isSupportedLanguage(value: string | null): value is SupportedLanguage {
  return value !== null && (SUPPORTED_LANGUAGES as readonly string[]).includes(value);
}

function detectLanguage(): SupportedLanguage {
  const forced = new URLSearchParams(window.location.search).get("lang");
  if (isSupportedLanguage(forced)) return forced;

  const stored = window.localStorage.getItem(STORAGE_KEY);
  if (isSupportedLanguage(stored)) return stored;

  const browser = navigator.language;
  if (isSupportedLanguage(browser)) return browser;
  if (browser.toLowerCase().startsWith("fr")) return "fr";
  return "en";
}

export function setLanguage(language: SupportedLanguage): void {
  window.localStorage.setItem(STORAGE_KEY, language);
  void i18next.changeLanguage(language);
}

void i18next.use(initReactI18next).init({
  lng: detectLanguage(),
  fallbackLng: "en",
  defaultNS: "common",
  resources: {
    en: { common: en },
    "en-GB": { common: enGB },
    fr: { common: fr },
  },
  interpolation: {
    escapeValue: false,
  },
  returnNull: false,
});

document.documentElement.lang = i18next.language;
i18next.on("languageChanged", (language) => {
  document.documentElement.lang = language;
});

export default i18next;
