import { createI18n } from "vue-i18n";
import { messages, type LocaleCode } from "./messages";

const storageKey = "rush-patch-locale";
const fallbackLocale: LocaleCode = "zh-CN";
const supportedLocales = Object.keys(messages) as LocaleCode[];

function isLocale(value: string | null): value is LocaleCode {
  return supportedLocales.includes(value as LocaleCode);
}

function initialLocale(): LocaleCode {
  const stored = typeof localStorage === "undefined" ? null : localStorage.getItem(storageKey);
  if (isLocale(stored)) return stored;

  const browserLocale = typeof navigator === "undefined" ? "" : navigator.language;
  return browserLocale.toLowerCase().startsWith("en") ? "en-US" : fallbackLocale;
}

export const localeOptions = [
  { label: "中文", value: "zh-CN" },
  { label: "English", value: "en-US" },
] satisfies Array<{ label: string; value: LocaleCode }>;

export const i18n = createI18n({
  legacy: false,
  locale: initialLocale(),
  fallbackLocale,
  messages,
});
export const currentLocale = i18n.global.locale;

export function setLocale(locale: LocaleCode) {
  currentLocale.value = locale;
  localStorage.setItem(storageKey, locale);
}

export function translate(key: string, params?: Record<string, number | string>) {
  return i18n.global.t(key, params ?? {});
}

export type { LocaleCode };
