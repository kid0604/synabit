/**
 * Asking Syn from the quick-entry box — the decisions, without the windows.
 *
 * The box the global hotkey opens has always taken a thought and queued it. It
 * now also takes a question and hands it to the main window's ask bar. This
 * file holds the two rules that make that safe to use without looking: which
 * of the two a keypress is, and what the main window does with a question it
 * cannot ask yet.
 */

/**
 * What Enter does in the quick-entry box.
 *
 * # Why a mode and not a `?` prefix
 *
 * A leading `?` is quicker to type, and it guesses. The box exists to catch a
 * thought before it goes, and a remarkable share of thoughts are questions —
 * "what was that book Lan mentioned?" is a capture, meant for later, not
 * something to be answered now. A rule that read the text would send those to
 * Syn and not to the inbox, and the person would find out only when the thing
 * they wrote down was not there.
 *
 * So the person says which, and says it once: Tab flips between the two, and
 * the box shows which one Enter is about to do. Tab is free — the box is a
 * single field, so there is nowhere for it to move focus to — and it is the key
 * a hand already on the keyboard reaches without looking. Capture stays the
 * default, and the box returns to it after every question, because capture is
 * what the hotkey promised.
 */
export type QuickEntryMode = 'capture' | 'ask';

export function nextMode(mode: QuickEntryMode, askAvailable: boolean): QuickEntryMode {
  if (!askAvailable) return 'capture';
  return mode === 'capture' ? 'ask' : 'capture';
}

/**
 * What Enter should do with this text, in this mode.
 *
 * `null` for nothing: an empty box, or one holding only whitespace, is not a
 * capture or a question. A mode of `ask` with Syn unavailable is a capture —
 * the mode was chosen when Syn was on, and a thought typed into the box must
 * never be dropped because a setting changed underneath it.
 */
export function quickEntryAction(
  mode: QuickEntryMode,
  text: string,
  askAvailable: boolean,
): { kind: QuickEntryMode; text: string } | null {
  const body = text.trim();
  if (!body) return null;
  return { kind: mode === 'ask' && askAvailable ? 'ask' : 'capture', text: body };
}

/**
 * What the main window does when a question arrives from the box.
 *
 * - `open`: the bar is allowed, so it opens with the question in it.
 * - `wait`: the bar is only held back by a lock. The question is left where
 *   Rust keeps it (`QuickQuestion`) and collected when the PIN is entered.
 *   Taking it now would put somebody's words into a window that is promising
 *   not to show anything, and dropping it would lose them.
 * - `capture`: there is no bar to open — no vault is chosen, or Syn is off for
 *   this one. The words become a capture instead, so they land in QuickCap
 *   rather than nowhere. The box stops offering "ask" when Syn is off, so this
 *   is the case where the setting changed between the two windows.
 */
export type QuickQuestionRoute = 'open' | 'wait' | 'capture';

export function routeQuickQuestion(state: {
  hasVault: boolean;
  synEnabled: boolean;
  allowed: boolean;
}): QuickQuestionRoute {
  if (!state.hasVault || !state.synEnabled) return 'capture';
  return state.allowed ? 'open' : 'wait';
}

/** The event the main window listens for. It carries nothing; see `QuickQuestion` in `capture.rs`. */
export const QUICK_QUESTION_EVENT = 'syn-quick-question';
