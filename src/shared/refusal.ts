/**
 * A refusal from the engine, in the reader's own language.
 *
 * # What comes over the wire
 *
 * `AppError::Refused` serialises to `{ code: "REFUSED:<name>", message, args }`
 * — see `src-tauri/src/refusal.rs`. The `message` is the engine's English; the
 * code and args are what let this say the same thing in Vietnamese.
 *
 * Everything else keeps arriving as `{ code, message }` with no args, and
 * falls through to the message, which is right: an SQL failure is a bug
 * report, not a sentence for somebody to act on.
 *
 * # Why it falls back rather than showing the code
 *
 * A refusal added in Rust and forgotten in the locales is caught by a test on
 * the Rust side (`refusal::tests::every_refusal_is_translated`). If one ever
 * slips past it anyway, English is a worse answer than Vietnamese and a far
 * better one than `query.refused.no_day_to_follow`.
 */
import { i18n } from '../i18n';
import { said } from '../utils/said';

/** What `refusalText` needs of a translator, which is very little. */
export type Translate = (key: string, args?: string[]) => string;

const MARK = 'REFUSED:';

interface Refused {
  code?: unknown;
  args?: unknown;
}

/** The name the locale files key a refusal by, or nothing. */
function nameOf(error: unknown): string | null {
  const code = (error as Refused | null | undefined)?.code;
  return typeof code === 'string' && code.startsWith(MARK) ? code.slice(MARK.length) : null;
}

/** Whether the engine refused the question, as against failing at it. */
export function wasRefused(error: unknown): boolean {
  return nameOf(error) !== null;
}

/**
 * What to put on the screen.
 *
 * The translator is a parameter so this can be tested without mounting
 * anything, and defaults to the app's own — **not** `useI18n()`, which only
 * works inside a component's `setup`. A composable is not always called from
 * one, and a refusal must not depend on where the code asking about it lives.
 */
export function refusalText(error: unknown, t: Translate = appSays): string {
  const name = nameOf(error);
  if (!name) return said(error);

  const key = `query.refused.${name}`;
  const args = (error as Refused).args;
  // `t` with a list fills `{0}`, `{1}` — the same placeholders the engine's
  // own English uses, so one template shape serves both sides.
  const translated = Array.isArray(args) ? t(key, args as string[]) : t(key);
  // vue-i18n hands the key back when it has no entry for it.
  return translated === key ? said(error) : translated;
}

/** The app's own translator, usable from anywhere. */
const appSays: Translate = (key, args) =>
  args ? (i18n.global.t as (k: string, a: string[]) => string)(key, args) : i18n.global.t(key);
