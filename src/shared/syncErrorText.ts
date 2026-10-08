/**
 * What a failed sync says to the person, rather than what the backend said to us.
 *
 * The sync status used to show the raw rejection: "E2EE key not set up. Please
 * set up encryption first.", "deserialize error: …". The first is English in a
 * Vietnamese app, the second is about our code rather than their files, and
 * neither says what to do.
 *
 * The raw text is still logged by the caller — it is what a bug report needs.
 * This picks the sentence to show: a known failure gets its own, anything else
 * the caller's general "could not sync".
 *
 * Returns an i18n key, not text, so it can be tested without a locale and the
 * caller translates in whatever language is current.
 */
import { said } from '../utils/said';

const KNOWN: ReadonlyArray<[RegExp, string]> = [
  [/e2ee key not set up|encryption (?:key )?(?:is )?not set up|set up encryption/i, 'shell.sync_errors.no_key'],
  [/no sync adapter|not connected|no server configured/i, 'shell.sync_errors.not_connected'],
  [/offline|network|connection (?:refused|reset|closed)|could not connect|failed to connect|dns|unreachable|timed? ?out/i, 'shell.sync_errors.offline'],
  [/upgrade required|protocol version|incompatible/i, 'shell.sync_errors.upgrade'],
  [/quota|storage (?:is )?full|too large/i, 'shell.sync_errors.quota'],
];

export function syncErrorKey(error: unknown, fallback: string): string {
  const text = said(error);
  for (const [pattern, key] of KNOWN) if (pattern.test(text)) return key;
  return fallback;
}
