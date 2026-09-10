/**
 * Taking something out of a conversation and keeping it.
 *
 * # Why an answer needed a way out at all
 *
 * The conversation sits inside a vault app full of surfaces — Notes,
 * Whiteboard, Calendar, Tasks — and could reach none of them. Syn *writes* to
 * the vault through its tools, but the answer it hands back was a dead end:
 * read it and it is over. That is most of what makes the panel feel flat, more
 * than any missing renderer.
 *
 * This is the first of those doors, and deliberately the narrowest one: a
 * diagram becomes a note. The shape it establishes — work out a name, write the
 * markdown, hand back something that can be opened — is what a table, an image
 * or a sketch will reuse.
 *
 * # The line this must not cross
 *
 * The button is the **app's**, decided by what the block is. It is never
 * something the model asked for. Syn's answers are written after reading
 * strangers' web pages, and a page that could make a button appear is a page
 * that can act through the reader's hand. See
 * `docs/syn-the-conversation-2026-09-10.md` §2.
 */

/** Where a diagram lifted out of a conversation goes. */
export const KEPT_IN = 'Notes';

/**
 * Quotes and markdown emphasis off; punctuation left alone.
 *
 * Mermaid's own titles arrive quoted — `title "Income vs Expense"` — and a
 * heading lifted out of an answer arrives with whatever emphasis it was written
 * with. Neither belongs in a title shown in a sidebar row.
 *
 * Nothing else is stripped. An earlier version took out `/`, `:` and `?` on the
 * grounds that the title becomes a filename. It does not: `create_node_file`
 * names the file after a UUID and the title lives in the frontmatter, so
 * *Kiến trúc: Splunk* would have lost its colon to a rule that protected
 * nothing.
 *
 * Eighty characters, because a title is read in a sidebar row and a tab label.
 */
const clean = (raw: string): string =>
  raw
    .replace(/^["'`]|["'`]$/g, '')
    .replace(/[*_~]/g, '')
    .replace(/\s+/g, ' ')
    .trim()
    .slice(0, 80);

/**
 * A name for the note, from the best thing anybody actually wrote.
 *
 * In order, and each step is a real sentence somebody has typed:
 *
 * 1. **The diagram's own title.** Mermaid takes one two ways — a `title:` line
 *    in a frontmatter block, and the bare `title` of a `pie` or `xychart`. If
 *    the author named the drawing, that is the name.
 * 2. **The heading above it.** An answer that draws something usually says what
 *    it is drawing first.
 * 3. **The question.** Failing both, the conversation is called something, and
 *    that is what the person was asking about.
 *
 * Never a generated string like "Diagram 3". A note nobody can find by name is
 * a note that was not really kept.
 */
export const titleFor = (code: string, around: string, fallback: string): string => {
  const front = /^\s*---[\s\S]*?^\s*title:\s*(.+?)\s*$/m.exec(code);
  if (front?.[1]) return clean(front[1]);

  // `pie title Monthly Spending`, and `xychart-beta` with `title "Income vs
  // Expense"` on its own line.
  const bare = /^\s*(?:\w[\w-]*\s+)?title\s+(.+?)\s*$/m.exec(code);
  if (bare?.[1]) return clean(bare[1]);

  const heading = /^#{1,6}\s+(.+?)\s*$/m.exec(around);
  if (heading?.[1]) return clean(heading[1]);

  return clean(fallback);
};

/**
 * The note's body: the diagram, and nothing added.
 *
 * No "saved from a conversation on…" line. The person asked for the diagram,
 * not for a receipt — and a note that opens with provenance instead of content
 * is a note that has to be scrolled past before it can be read.
 */
export const bodyFor = (code: string): string =>
  ['```mermaid', code.trim(), '```', ''].join('\n');
