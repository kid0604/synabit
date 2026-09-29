/**
 * A Safe error, in the reader's language.
 *
 * Rust sends `{ code: "SAFE:<code>", message }`. A known code has its own
 * sentence under `safe.errors`; anything else — a disk error, a damaged file —
 * falls back to the message inside `safe.errors.failed`, which is a bug report
 * rather than a sentence to act on, and says so.
 */
import { useI18n } from 'vue-i18n';
import { errorText } from '../../shared/errorText';
import { safeCode } from './api';

const KNOWN = new Set([
  'locked', 'no_safe', 'already_exists', 'wrong_password', 'needs_secret_key', 'bad_secret_key',
  'password_too_short', 'not_found', 'no_title', 'unknown_field', 'bad_totp',
  'import_unknown', 'encrypted_bitwarden', 'needs_export_password', 'wrong_export_password',
  'bad_handle', 'handle_taken', 'no_destination', 'request_gone', 'bad_recipe', 'keychain', 'clipboard',
]);

export function useSafeError() {
  const { t } = useI18n();
  return (error: unknown): string => {
    const code = safeCode(error);
    if (code && KNOWN.has(code)) return t(`safe.errors.${code}`);
    return t('safe.errors.failed', { msg: errorText(error) });
  };
}
