import type { Highlight } from '../types/feed.types';

/**
 * Marking passages in an article, and finding them again afterwards.
 *
 * A highlight is stored as its exact words plus which occurrence of those
 * words it is. That survives the article being re-extracted when its full text
 * is fetched, which a character offset would not: the offset taken before the
 * fetch points into a different sentence after it.
 */

/** The plain text of an element, as the browser reads it for a Range. */
function textOf(node: Node): string {
  return node.textContent ?? '';
}

/** How many complete occurrences of `text` appear in `haystack`. */
function countOccurrences(haystack: string, text: string): number {
  if (!text) return 0;
  let count = 0;
  let index = 0;
  while ((index = haystack.indexOf(text, index)) !== -1) {
    count += 1;
    index += text.length;
  }
  return count;
}

/**
 * Which occurrence of its own text a selection is, counting from zero.
 *
 * Measured from the start of the article to the start of the selection, so
 * highlighting the second "on the other hand" records the second one.
 */
export function occurrenceOfSelection(root: HTMLElement, range: Range, text: string): number {
  const before = document.createRange();
  before.setStart(root, 0);
  before.setEnd(range.startContainer, range.startOffset);
  return countOccurrences(before.toString(), text);
}

/**
 * Wrap each stored highlight in a `<mark>` inside a freshly rendered article.
 *
 * Works on the live DOM rather than on the HTML string: the markup has already
 * been rebuilt for images, and doing this with string replacement would mean
 * matching text that may be split across tags.
 *
 * A highlight whose words no longer appear — the publisher edited the piece,
 * or the full text replaced a summary that contained it — is left unrendered
 * rather than being guessed at or deleted. It costs nothing and comes back if
 * the words do.
 */
/**
 * The `<mark>` standing for a given highlight, if it is currently drawn.
 *
 * Compared by attribute rather than matched by selector: `CSS.escape` is the
 * only correct way to put an arbitrary id into a selector string, and it is
 * not available everywhere this runs.
 */
export function findMark(root: ParentNode, highlightId: string): HTMLElement | null {
  for (const mark of root.querySelectorAll<HTMLElement>('mark[data-highlight-id]')) {
    if (mark.dataset.highlightId === highlightId) return mark;
  }
  return null;
}

export function applyHighlights(root: HTMLElement, highlights: Highlight[]): number {
  let applied = 0;

  for (const highlight of highlights) {
    // Painting runs whenever the body is re-rendered, and more than one thing
    // can trigger that for the same article. Wrapping an already-wrapped
    // passage would nest a mark inside itself and double the shading.
    if (findMark(root, highlight.id)) {
      applied += 1;
      continue;
    }

    const range = findOccurrence(root, highlight.text, highlight.occurrence);
    if (!range) continue;

    const mark = document.createElement('mark');
    mark.className = 'feed-highlight';
    mark.dataset.highlightId = highlight.id;
    if (highlight.note) mark.title = highlight.note;

    try {
      range.surroundContents(mark);
      applied += 1;
    } catch {
      // The passage spans a tag boundary — a sentence with a link in the
      // middle of it. Wrapping it would mean restructuring the article, which
      // is a worse trade than one highlight not showing.
    }
  }

  return applied;
}

/** A Range over the nth occurrence of `text`, or null if it is not there. */
function findOccurrence(root: HTMLElement, text: string, occurrence: number): Range | null {
  if (!text) return null;

  const walker = document.createTreeWalker(root, NodeFilter.SHOW_TEXT);
  let seen = 0;
  let node = walker.nextNode();

  while (node) {
    const value = textOf(node);
    let index = value.indexOf(text);

    while (index !== -1) {
      if (seen === occurrence) {
        const range = document.createRange();
        range.setStart(node, index);
        range.setEnd(node, index + text.length);
        return range;
      }
      seen += 1;
      index = value.indexOf(text, index + text.length);
    }

    node = walker.nextNode();
  }

  return null;
}
