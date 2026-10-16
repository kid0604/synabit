import { createDocument, type Editor } from '@tiptap/core';
import type { Node as PMNode } from '@tiptap/pm/model';

/**
 * What a note would lose between being saved and being opened again —
 * `note.roundtrip`. See `shared/invariants`.
 *
 * The markdown is read back the way opening a note reads it (the editor's own
 * markdown parser, then its schema) and compared with the document it was
 * written from: every word and every link's address should still be there.
 * Counted as many times as they occur, so a link written twice and kept once
 * is a loss. Formatting, and anything the reading adds, is not compared — a
 * toggle's summary comes back as text, which costs nothing.
 *
 * The case it was written for: a paragraph after a toggle, saved with no
 * blank line between, came back as raw HTML with its link gone.
 */
/**
 * The document as the note's markdown, before local asset paths are made
 * relative: what saving writes, and what the audit reads back.
 */
export function savedMarkdown(editor: Editor): string {
  return (editor.storage as any).markdown
    .getMarkdown()
    .replace(/<span[^>]*data-transclusion="([^"]+)"[^>]*>.*?<\/span>/g, (_m: string, target: string) => `![[${target}]]`);
}

export interface Losses {
  words: number;
  links: number;
  /** The first top-level block that lost something, or `null`. */
  firstBlock: number | null;
}

interface Held {
  words: Map<string, number>;
  links: Map<string, number>;
}

const add = (m: Map<string, number>, k: string) => m.set(k, (m.get(k) ?? 0) + 1);

/** The words and link addresses in `node` and everything inside it. */
function held(node: PMNode): Held {
  const out: Held = { words: new Map(), links: new Map() };
  const visit = (n: PMNode) => {
    if (n.isTextblock) {
      for (const w of n.textContent.split(/\s+/)) if (w) add(out.words, w);
    }
    if (n.isText) {
      for (const mark of n.marks) {
        if (mark.type.name === 'link' && mark.attrs.href) add(out.links, String(mark.attrs.href));
      }
    }
  };
  visit(node);
  node.descendants((n) => {
    visit(n);
    return true;
  });
  return out;
}

/** How many of `before`'s occurrences `after` lacks, and which keys. */
function missing(before: Map<string, number>, after: Map<string, number>): { count: number; keys: Set<string> } {
  let count = 0;
  const keys = new Set<string>();
  before.forEach((n, k) => {
    const short = n - (after.get(k) ?? 0);
    if (short > 0) {
      count += short;
      keys.add(k);
    }
  });
  return { count, keys };
}

/**
 * What `doc` would lose saved as `markdown` and opened again, read with
 * `editor`'s parser and schema. `doc` is the document as it was when the
 * markdown was written, not as it is now.
 */
export function lostOnReopen(editor: Editor, doc: PMNode, markdown: string): Losses {
  const parser = (editor.storage as any).markdown?.parser;
  const html: string = parser ? parser.parse(markdown) : markdown;
  const reopened = createDocument(html, editor.schema);

  const before = held(doc);
  const after = held(reopened);
  const words = missing(before.words, after.words);
  const links = missing(before.links, after.links);

  let firstBlock: number | null = null;
  if (words.count || links.count) {
    doc.forEach((block, _offset, index) => {
      if (firstBlock !== null) return;
      const h = held(block);
      const touches = [...h.words.keys()].some((w) => words.keys.has(w)) || [...h.links.keys()].some((l) => links.keys.has(l));
      if (touches) firstBlock = index;
    });
  }
  return { words: words.count, links: links.count, firstBlock };
}
