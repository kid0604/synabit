/**
 * What must always hold, and how a breach is written down.
 *
 * # Why rules and not bugs
 *
 * Each fault found on 2026-10-15 broke a rule that should never break: a note
 * lost its link between saving and opening (`note.roundtrip`), a conversation
 * froze the page for good (`ui.page-alive`), Syn said "Đã lưu vào QuickCap"
 * with nothing called (`syn.claims-match-tools`). Each had left a trace before
 * anybody noticed — eleven notes already damaged, a hundred blocked-page
 * warnings, three run files with no tool call — that nobody read. A rule
 * checked everywhere it can break finds the next fault of its kind too, which
 * a test for the last one does not.
 *
 * The rules are in `contracts/invariants.json`, which Rust reads as well. A breach is
 * one log line, `[invariant] {json}`, so the app's own log is where every
 * check while it runs ends up, and `npm run audit` reads it back.
 */
import catalog from '../../../contracts/invariants.json';
import { logger } from '../../utils/logger';

export type InvariantId =
  | 'note.roundtrip'
  | 'sync.converges'
  | 'ui.responsive'
  | 'ui.page-alive'
  | 'ui.no-uncaught-error'
  | 'syn.claims-match-tools';

export interface Invariant {
  id: InvariantId;
  layer: string;
  rule: string;
  checked: string[];
}

export const INVARIANTS = catalog.invariants as Invariant[];

/**
 * One breach. `subject` is what it happened to — a note's id, a run's id, a
 * route — and `detail` is counts and names, never what anybody wrote: the log
 * is read by whoever is debugging, not by the person whose notes they are.
 */
export interface Violation {
  invariant: InvariantId;
  subject?: string;
  detail?: Record<string, string | number | boolean | null>;
}

export const LOG_MARK = '[invariant]';

/** Write a breach to the app's log, where the audit finds it. */
export function reportViolation(v: Violation): void {
  logger.warn(`${LOG_MARK} ${JSON.stringify(v)}`);
}

/** A breach back out of a log line, or `null` for any other line. */
export function parseViolation(line: string): Violation | null {
  const at = line.indexOf(LOG_MARK);
  if (at < 0) return null;
  try {
    const v = JSON.parse(line.slice(at + LOG_MARK.length).trim());
    return v && typeof v.invariant === 'string' ? (v as Violation) : null;
  } catch {
    return null;
  }
}

const CLAIM_WORDS: string[] = catalog.claims.words;
/** How much of an answer is read for a claim: where "Đã lưu" goes. */
const CLAIM_WINDOW = 300;

/**
 * Whether an answer says it did something. Rust holds the same rule over the
 * same words (`invariants::claims_action`); a test keeps them agreeing.
 */
export function claimsAction(answer: string): boolean {
  // Before any code fence: words after a diagram describe the diagram.
  const head = answer.split('```')[0].slice(0, CLAIM_WINDOW).toLowerCase();
  return CLAIM_WORDS.some((word) => {
    let from = 0;
    for (;;) {
      const at = head.indexOf(word, from);
      if (at < 0) return false;
      const before = at === 0 ? '' : head[at - 1];
      if (!before || !/[\p{L}\p{N}]/u.test(before)) return true;
      from = at + 1;
    }
  });
}

/**
 * Whether a tool's result is an answer rather than a refusal — as Rust's
 * `engine::tool_succeeded`: an error, a refusal or a dry run did nothing.
 */
export function toolWentThrough(resultPreview: string): boolean {
  try {
    const v = JSON.parse(resultPreview);
    return !(v && typeof v === 'object' && ('error' in v || 'refused' in v || 'planned' in v));
  } catch {
    return true;
  }
}

/**
 * An answer that says it did something with nothing behind it — the
 * `syn.claims-match-tools` breach, read off the message itself so a screen can
 * say so under it. Telegram does the same before sending (`telegram::service`).
 */
export function claimsWithNothingDone(message: { role: string; content: string; tool_calls_log?: { result_preview: string }[] | null }): boolean {
  if (message.role !== 'assistant') return false;
  const wentThrough = (message.tool_calls_log ?? []).some((c) => toolWentThrough(c.result_preview));
  return !wentThrough && claimsAction(message.content);
}
