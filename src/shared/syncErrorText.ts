/**
 * What a failed sync or pairing says to the person, rather than what the
 * backend said to us.
 *
 * Both screens used to show the raw rejection: "E2EE key not set up. Please
 * set up encryption first.", "Command p2p_pair_initiate not found",
 * "deserialize error: …". The first is English in a Vietnamese app, the second
 * is about our code rather than their devices, and none says what to do.
 *
 * The raw text is still logged by the caller — it is what a bug report needs.
 * This picks the sentence to show: a known failure gets its own, anything else
 * the caller's general "could not sync" / "could not pair".
 *
 * Returns an i18n key, not text, so it can be tested without a locale and the
 * caller translates in whatever language is current.
 */
import { said } from '../utils/said';

const KNOWN: ReadonlyArray<[RegExp, string]> = [
  // The command is not in this build at all (device pairing on some platforms).
  [/command\s+\S+\s+not\s+found|unknown command|not (?:yet )?(?:implemented|supported)/i, 'shell.sync_errors.unavailable'],
  [/e2ee key not set up|encryption (?:key )?(?:is )?not set up|set up encryption/i, 'shell.sync_errors.no_key'],
  [/no sync adapter|not connected|no server configured/i, 'shell.sync_errors.not_connected'],
  [/offline|network|connection (?:refused|reset|closed)|could not connect|failed to connect|dns|unreachable|timed? ?out/i, 'shell.sync_errors.offline'],
  [/upgrade required|protocol version|incompatible/i, 'shell.sync_errors.upgrade'],
  [/quota|storage (?:is )?full|too large/i, 'shell.sync_errors.quota'],
  [/(?:invalid|wrong|unknown|expired).{0,20}code|code.{0,20}(?:invalid|expired|not found)/i, 'shell.sync_errors.bad_code'],
];

export function syncErrorKey(error: unknown, fallback: string): string {
  const text = said(error);
  for (const [pattern, key] of KNOWN) if (pattern.test(text)) return key;
  return fallback;
}
