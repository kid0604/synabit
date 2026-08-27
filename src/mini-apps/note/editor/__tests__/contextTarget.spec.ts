import { describe, it, expect } from 'vitest';
import { contextTargetFor } from '../contextTarget';

/** Build a fragment and hand back the element to right-click on. */
const clickTargetIn = (html: string, selector: string): Element => {
  const host = document.createElement('div');
  host.innerHTML = html;
  const el = host.querySelector(selector);
  if (!el) throw new Error(`fixture has no ${selector}`);
  return el;
};

describe('contextTargetFor', () => {
  it('answers a right-click on a link with the link', () => {
    // The bug this exists for: a link sits inside a paragraph, and the block
    // rule used to be asked first — so right-clicking a mention offered
    // "Copy Block Link" and nothing about the link.
    const el = clickTargetIn(
      '<p>họp với <a href="synabit://note/Notes/Splunk.md">Splunk Query</a> nhé</p>',
      'a'
    );

    expect(contextTargetFor(el)).toEqual({
      kind: 'link',
      href: 'synabit://note/Notes/Splunk.md',
    });
  });

  it('still answers the paragraph around a link with the block', () => {
    const el = clickTargetIn(
      '<p>họp với <a href="synabit://note/x.md">Splunk</a> nhé</p>',
      'p'
    );

    expect(contextTargetFor(el)).toEqual({ kind: 'block' });
  });

  it('lets a table cell win over the paragraph inside it', () => {
    const el = clickTargetIn('<table><tbody><tr><td><p>cell</p></td></tr></tbody></table>', 'p');

    expect(contextTargetFor(el)).toEqual({ kind: 'table' });
  });

  it('does not treat an anchor without a destination as a link', () => {
    // Tiptap leaves bare anchors around; offering to open or unlink one that
    // goes nowhere is worse than falling through to the block menu.
    const el = clickTargetIn('<p><a>not really a link</a></p>', 'a');

    expect(contextTargetFor(el)).toEqual({ kind: 'block' });
  });

  it('recognises an external link as readily as an internal one', () => {
    const el = clickTargetIn('<p><a href="https://example.com">docs</a></p>', 'a');

    expect(contextTargetFor(el)).toEqual({ kind: 'link', href: 'https://example.com' });
  });

  it('claims nothing for a click outside any of the three', () => {
    expect(contextTargetFor(clickTargetIn('<div><span>x</span></div>', 'span'))).toEqual({
      kind: 'none',
    });
    expect(contextTargetFor(null)).toEqual({ kind: 'none' });
  });
});
