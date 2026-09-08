/**
 * What the user is looking at, read at the instant they ask.
 *
 * # Why this is read rather than published
 *
 * The obvious design is a store every mini-app writes into as its state
 * changes. That means touching twelve apps, keeping twelve publishers correct
 * forever, and a store that is wrong whenever one of them forgets — and it
 * buys nothing, because focus is only ever wanted at one moment: the moment a
 * question is asked.
 *
 * So nothing publishes. The bar reads the screen when it opens. The app id and
 * the open node come from `App.vue`, which already tracks both for the
 * back/forward stack; the selection comes from the DOM, which knows it for
 * every app at once — including the ones nobody has written a publisher for,
 * and including the ones that do not exist yet.
 *
 * # Why the reading has to happen first
 *
 * Focusing an input collapses the document selection. By the time the bar's
 * textarea has the caret, `window.getSelection()` is empty — so the capture
 * runs in the keydown handler, before anything is shown. Getting this backwards
 * produces a feature that works in every test and never once in the app.
 */

/** What travels with a question. Mirrors `syn::focus::Focus` in Rust. */
export interface SynFocus {
  /** The mini-app id, as `appRegistry.ts` spells it. */
  app: string;
  /** The open node's vault-relative path, when the app has one. */
  node?: string;
  /**
   * What that node is called, when the path does not say.
   *
   * In a vault whose note files are named by uuid, the path and the title
   * share nothing: `Notes/4e0bc181-e384-40d2-….md` tells the person reading
   * the bar precisely nothing about which note Syn can see. Sent as well as
   * the path, not instead of it — the path is what the tools take.
   */
  node_title?: string;
  /** Whatever is highlighted, anywhere on screen. */
  selection?: string;
  /**
   * The open thread this question belongs to, as its vault-relative path.
   *
   * Chosen by the person, not read off the screen — which is why it is passed
   * into `buildFocus` rather than gathered by `captureFocus`. The backend
   * looks the body up and renders it as its own prompt section; see
   * `src-tauri/src/syn/thread.rs`.
   */
  thread?: string;
}

/**
 * How much selected text is read before it is cut.
 *
 * The backend caps this too, at `focus::MAX_SELECTION_CHARS`, and that cap is
 * the one that matters — it is what protects the prompt. This one is larger on
 * purpose: it is a guard against moving megabytes across the IPC boundary for
 * a select-all, and cutting it to exactly the prompt's cap here would mean the
 * backend could never tell the user how much it left out.
 */
export const MAX_SELECTION_READ = 20_000;

/**
 * Text selected inside a form field, which the document selection cannot see.
 *
 * `window.getSelection()` returns nothing for a selection inside `<input>` or
 * `<textarea>` in most engines, so a person highlighting half of a task title
 * would be told there was nothing on screen. Read from the element instead.
 */
function selectionInsideField(active: Element | null): string | null {
  if (!(active instanceof HTMLInputElement) && !(active instanceof HTMLTextAreaElement)) {
    return null;
  }
  const { selectionStart, selectionEnd, value } = active;
  if (selectionStart === null || selectionEnd === null || selectionStart === selectionEnd) {
    return null;
  }
  return value.slice(selectionStart, selectionEnd);
}

/**
 * Everything highlighted on screen, or `undefined` for nothing.
 *
 * Split from the DOM globals so the rules can be tested without a browser. The
 * rules are: a field's own selection wins when the caret is in one, the
 * document selection otherwise, whitespace is not a selection, and nothing is
 * ever longer than `MAX_SELECTION_READ`.
 */
export function readSelection(active: Element | null, documentSelection: string | null): string | undefined {
  const raw = selectionInsideField(active) ?? documentSelection ?? '';
  const trimmed = raw.trim();
  if (!trimmed) return undefined;
  return trimmed.length > MAX_SELECTION_READ ? trimmed.slice(0, MAX_SELECTION_READ) : trimmed;
}

/**
 * Where the question is being asked from.
 *
 * An object rather than four positional strings, for the reason `DriveRequest`
 * is one: three of these are `string | undefined` and sit next to each other,
 * so any two could be swapped without the compiler noticing.
 */
export interface Where {
  /** The mini-app id, as `appRegistry.ts` spells it. */
  app: string;
  /** The open node's vault-relative path. */
  node?: string;
  /** What that node is called, when the path does not say. */
  nodeTitle?: string;
  /** The thread the question belongs to. */
  thread?: string;
}

/**
 * The focus to send with a question, or `undefined` when there is nothing to
 * say.
 *
 * `undefined` rather than an empty object, because the backend renders no
 * section for an empty focus and an object full of blanks would have to be
 * checked in two places instead of one.
 */
export function buildFocus(where: Where, selection: string | undefined): SynFocus | undefined {
  const app = where.app?.trim() ?? '';
  const { node, nodeTitle, thread } = where;
  if (!app && !node && !selection && !thread) return undefined;
  return {
    app,
    ...(node ? { node } : {}),
    // Only alongside a path. A title with nothing to address is a name the
    // tools cannot act on and the bar would show in place of one that works.
    ...(node && nodeTitle ? { node_title: nodeTitle } : {}),
    ...(selection ? { selection } : {}),
    ...(thread ? { thread } : {}),
  };
}

/**
 * Read the live screen. Call this from a keydown handler, before showing
 * anything — see the note at the top of this file.
 */
export function captureFocus(where: Where): SynFocus | undefined {
  const selection = readSelection(
    document.activeElement,
    window.getSelection()?.toString() ?? null,
  );
  return buildFocus(where, selection);
}

/**
 * A one-line description of what was captured, for the bar to show.
 *
 * The bar says what it is about to send. Not a courtesy: the difference
 * between "Syn is ignoring me" and "Syn did not have the paragraph" is
 * invisible unless the screen says which one happened, and a person who cannot
 * see what was picked up cannot learn to select before asking.
 */
export function describeFocus(focus: SynFocus | undefined): { node?: string; chars?: number } {
  if (!focus) return {};
  // The title when there is one, because this line is read by a person. The
  // path still travels to the model, which needs something it can address.
  const node = focus.node_title ?? focus.node;
  return {
    ...(node ? { node } : {}),
    ...(focus.selection ? { chars: focus.selection.length } : {}),
  };
}
