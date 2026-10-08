import { createI18n } from 'vue-i18n';
import en from './locales/en.json';

export type AppLocale = 'en' | 'vi';
export const APP_LOCALES: readonly AppLocale[] = ['en', 'vi'];

/**
 * English is bundled because it is the fallback: a key missing from another
 * language is looked up here, synchronously, while a screen draws. Every other
 * language — a quarter of a megabyte each — is fetched when somebody picks it,
 * not by every window at start.
 */
const loaders: Record<Exclude<AppLocale, 'en'>, () => Promise<{ default: unknown }>> = {
  vi: () => import('./locales/vi.json'),
};

export const i18n = createI18n({
  legacy: false, // Use Composition API
  locale: 'en', // Default locale, will be overwritten by store
  fallbackLocale: 'en',
  // Typed as holding every language so `locale` accepts all of them; only
  // English is here until `loadLocale` brings another.
  messages: { en } as Record<AppLocale, typeof en>,
});

const loading = new Map<AppLocale, Promise<void>>();

/** Make this language's messages available. Safe to call any number of times. */
export function loadLocale(locale: AppLocale): Promise<void> {
  if (locale === 'en' || i18n.global.availableLocales.includes(locale)) return Promise.resolve();
  let pending = loading.get(locale);
  if (!pending) {
    pending = loaders[locale]()
      .then((mod) => { i18n.global.setLocaleMessage(locale, mod.default as typeof en); })
      .catch((err) => {
        // Not kept, so the next ask tries again.
        loading.delete(locale);
        throw err;
      });
    loading.set(locale, pending);
  }
  return pending;
}

let latestAsk = 0;

/**
 * Switch the app's language, after its messages have arrived — so a screen
 * never draws a frame of keys, or of English, on the way to Vietnamese.
 * Unknown values fall back to English rather than to a language with no text.
 */
export async function setAppLocale(locale: string): Promise<void> {
  const wanted: AppLocale = (APP_LOCALES as readonly string[]).includes(locale) ? locale as AppLocale : 'en';
  const ask = ++latestAsk;
  try {
    await loadLocale(wanted);
    // Switched again while this one loaded: the later choice stands.
    if (ask !== latestAsk) return;
  } catch {
    if (ask !== latestAsk) return;
    // The messages could not be read: English, which is always here, rather
    // than a language that would show keys.
    i18n.global.locale.value = 'en';
    return;
  }
  i18n.global.locale.value = wanted;
}

/** Every language's messages, for code that compares text across all of them. */
export function loadAllLocales(): Promise<void> {
  return Promise.all(APP_LOCALES.map(loadLocale)).then(() => undefined);
}
