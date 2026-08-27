/**
 * Reading `@what to find|what to call it` out of a mention query.
 *
 * A link's text and the note it points at are separate things. A note titled
 * "Công ty cổ phần ABC" is worth calling "công ty cũ" in the middle of a
 * sentence, and a document that spells out the registered name every time it
 * refers to something is a worse document.
 *
 * Nothing downstream needs teaching. `[Nhãn](synabit://note/path)` has always
 * carried free text as its label, and renaming the target leaves that label
 * alone unless the label *was* the old title — see `rename_links_in_text` on
 * the Rust side, and the two tests there that hold it to that. This only gives
 * the same thing a way in from the keyboard.
 */
export interface MentionQuery {
  /** What to look the note up by. */
  search: string;
  /** What the link should read as, or empty to use the note's title. */
  alias: string;
}

export function splitMentionQuery(raw: string): MentionQuery {
  // The *first* bar splits, so an alias may itself contain one.
  const at = raw.indexOf('|');
  if (at === -1) return { search: raw.trim(), alias: '' };
  return {
    search: raw.slice(0, at).trim(),
    alias: raw.slice(at + 1).trim(),
  };
}
