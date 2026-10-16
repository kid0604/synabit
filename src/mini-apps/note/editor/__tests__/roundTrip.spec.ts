import { describe, it, expect, afterEach } from 'vitest';
import { Editor, Node } from '@tiptap/core';
import StarterKit from '@tiptap/starter-kit';
import Link from '@tiptap/extension-link';
import { Markdown } from 'tiptap-markdown';
import { lostOnReopen } from '../roundTrip';

/**
 * `note.roundtrip`, against a block saved the way toggles were before
 * 2026-10-15: HTML with no blank line after it, so the next paragraph is read
 * back as part of the block.
 */
const OldBox = Node.create({
  name: 'oldBox',
  group: 'block',
  atom: true,
  parseHTML: () => [{ tag: 'div[data-old-box]' }],
  renderHTML: () => ['div', { 'data-old-box': 'true' }],
  addStorage: () => ({
    markdown: {
      serialize(state: any) {
        state.write('<div data-old-box="true"></div>\n');
      },
    },
  }),
});

const LINK = 'https://chat.google.com/room/AAQAv1dGQyM/qbDhtbXAlT0/qbDhtbXAlT0?cls=10';
let editors: Editor[] = [];
afterEach(() => {
  editors.forEach((e) => e.destroy());
  editors = [];
});

const open = (content: object) => {
  const e = new Editor({
    element: document.createElement('div'),
    extensions: [StarterKit, Link, OldBox, Markdown.configure({ html: true })],
    content,
  });
  editors.push(e);
  return e;
};

const markdownOf = (e: Editor) => (e.storage as any).markdown.getMarkdown() as string;

const paragraph = (text: string, href?: string) => ({
  type: 'paragraph',
  content: [
    { type: 'text', text },
    ...(href ? [{ type: 'text', text: href, marks: [{ type: 'link', attrs: { href } }] }] : []),
  ],
});

describe('what a note would lose on opening again', () => {
  it('finds the link lost after a block saved without a blank line', () => {
    const e = open({ type: 'doc', content: [paragraph('Trước'), { type: 'oldBox' }, paragraph('Thread: ', LINK)] });
    const lost = lostOnReopen(e, e.state.doc, markdownOf(e));
    expect(lost.links).toBe(1);
    expect(lost.firstBlock).toBe(2);
  });

  it('finds nothing in a note that comes back whole', () => {
    const e = open({
      type: 'doc',
      content: [
        { type: 'heading', attrs: { level: 2 }, content: [{ type: 'text', text: 'Họp CMC' }] },
        paragraph('Xem ', LINK),
        { type: 'bulletList', content: [{ type: 'listItem', content: [paragraph('một'), paragraph('hai')] }] },
        { type: 'codeBlock', content: [{ type: 'text', text: '</details>\ntext <b>' }] },
        paragraph('**không đậm** và <thẻ>'),
      ],
    });
    expect(lostOnReopen(e, e.state.doc, markdownOf(e))).toEqual({ words: 0, links: 0, firstBlock: null });
  });

  it('counts a word as often as it occurs', () => {
    const e = open({ type: 'doc', content: [paragraph('Cam Cam')] });
    expect(lostOnReopen(e, e.state.doc, 'Cam').words).toBe(1);
  });
});
