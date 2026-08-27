import { describe, it, expect } from 'vitest';
import { applyHighlights, occurrenceOfSelection } from '../useHighlights';
import type { Highlight } from '../../types/feed.types';

function article(html: string): HTMLElement {
  const root = document.createElement('div');
  root.innerHTML = html;
  document.body.appendChild(root);
  return root;
}

function highlight(text: string, occurrence = 0, id = 'h1'): Highlight {
  return {
    id,
    sourceId: 's1',
    guid: 'g1',
    text,
    occurrence,
    note: '',
    createdAt: '2026-08-24T00:00:00Z',
  };
}

describe('applyHighlights', () => {
  it('wraps the passage in a mark that knows which highlight it is', () => {
    const root = article('<p>The quick brown fox jumps over the lazy dog.</p>');

    expect(applyHighlights(root, [highlight('brown fox')])).toBe(1);

    const mark = root.querySelector('mark.feed-highlight');
    expect(mark?.textContent).toBe('brown fox');
    expect((mark as HTMLElement).dataset.highlightId).toBe('h1');
  });

  it('picks the occurrence it was told to, not the first one', () => {
    const root = article('<p>on the other hand, and on the other hand again</p>');

    applyHighlights(root, [highlight('on the other hand', 1)]);

    const mark = root.querySelector('mark') as HTMLElement;
    // Wrapping splits the paragraph, so what precedes the mark is the text the
    // reader did not select — including the first occurrence.
    expect(mark.previousSibling?.textContent).toBe('on the other hand, and ');
    expect(mark.nextSibling?.textContent).toBe(' again');
  });

  it('leaves a passage the article no longer contains unrendered', () => {
    // The publisher edited the piece, or a summary was replaced by full text.
    const root = article('<p>A completely different sentence.</p>');

    expect(applyHighlights(root, [highlight('words that are gone')])).toBe(0);
    expect(root.querySelector('mark')).toBeNull();
  });

  it('does not nest a mark inside itself when painted twice', () => {
    const root = article('<p>The quick brown fox.</p>');
    const marks = [highlight('brown fox')];

    applyHighlights(root, marks);
    applyHighlights(root, marks);

    expect(root.querySelectorAll('mark').length).toBe(1);
  });

  it('skips a passage that spans a tag boundary rather than restructuring the article', () => {
    // Wrapping this would mean rebuilding the paragraph around the link.
    const root = article('<p>Read the <a href="/x">documentation</a> carefully.</p>');

    expect(applyHighlights(root, [highlight('the documentation carefully')])).toBe(0);
  });
});

describe('occurrenceOfSelection', () => {
  it('counts how many identical passages precede the selection', () => {
    const root = article('<p>echo and echo and echo</p>');
    const text = root.firstChild!.firstChild!;

    const first = document.createRange();
    first.setStart(text, 0);
    first.setEnd(text, 4);
    expect(occurrenceOfSelection(root, first, 'echo')).toBe(0);

    // The third "echo" begins at index 18 of "echo and echo and echo".
    const third = document.createRange();
    third.setStart(text, 18);
    third.setEnd(text, 22);
    expect(occurrenceOfSelection(root, third, 'echo')).toBe(2);
  });

  it('counts across elements, because the reader does not see the tags', () => {
    const root = article('<p>echo</p><p>echo</p>');
    const second = root.children[1].firstChild!;

    const range = document.createRange();
    range.setStart(second, 0);
    range.setEnd(second, 4);

    expect(occurrenceOfSelection(root, range, 'echo')).toBe(1);
  });
});
