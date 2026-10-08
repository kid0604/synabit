/**
 * Every language, loaded before any test runs.
 *
 * The app fetches a language other than English when somebody picks it
 * (`loadLocale`); tests set `i18n.global.locale` directly and expect its text
 * to be there, the way it was when every language was bundled.
 */
import { i18n } from './i18n';
import vi from './i18n/locales/vi.json';

i18n.global.setLocaleMessage('vi', vi as never);
