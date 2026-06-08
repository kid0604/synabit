import { createI18n } from 'vue-i18n';
import en from './locales/en.json';
import vi from './locales/vi.json';

export const i18n = createI18n({
  legacy: false, // Use Composition API
  locale: 'en', // Default locale, will be overwritten by store
  fallbackLocale: 'en',
  messages: {
    en,
    vi
  }
});
