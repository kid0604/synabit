import { describe, it, expect, afterEach, vi } from 'vitest';
import { Editor } from '@tiptap/core';
import StarterKit from '@tiptap/starter-kit';
import Link from '@tiptap/extension-link';
import { Markdown } from 'tiptap-markdown';

// The node view is the screen's; what is saved is the markdown.
vi.mock('../../nodes/DetailsNodeView.vue', () => ({ default: {} }));
vi.mock('../../nodes/LocationNodeView.vue', () => ({ default: {} }));
vi.mock('../../nodes/WhiteboardNodeView.vue', () => ({ default: {} }));
vi.mock('../../nodes/SynVisualNodeView.vue', () => ({ default: {} }));
vi.mock('../../nodes/ImageGalleryNode.vue', () => ({ default: {} }));
vi.mock('@tiptap/vue-3', async (real) => ({ ...(await real<object>()), VueNodeViewRenderer: () => null }));

import { DetailsExtension } from '../../extensions/DetailsExtension';
import { PdfExtension } from '../../PdfExtension';
import { LocationExtension } from '../../LocationExtension';
import { AudioExtension } from '../../AudioExtension';
import { WhiteboardExtension } from '../../WhiteboardExtension';
import { SynVisualExtension } from '../../SynVisualExtension';
import { VideoExtension } from '../../VideoExtension';
import { ImageGallery } from '../../extensions/ImageGallery';
import { HtmlBlockBreaks, separateHtmlBlocks } from '../../extensions/HtmlBlockBreaks';
import { CustomImage } from '../../extensions/CustomImage';

/**
 * What follows a block written as HTML is still its own paragraph once saved.
 *
 * A toggle was saved as `…</details>` and a newline, with no blank line after.
 * In markdown an HTML block runs until a blank line, so the next paragraph
 * became part of it: "Thread về việc rà soát domain, TLS cert: <https://…>"
 * was kept as raw HTML, where `<https://…>` is no link but an unknown tag, and
 * the device that opened the file next showed the text with the link gone.
 * Put on a line of its own after a blank line, the link was its own paragraph
 * and came through — which is what made it look like a sync fault.
 */

const LINK = 'https://chat.google.com/room/AAQAv1dGQyM/qbDhtbXAlT0/qbDhtbXAlT0?cls=10';

let editors: Editor[] = [];
afterEach(() => {
  editors.forEach((e) => e.destroy());
  editors = [];
});

const open = (content: string | object) => {
  const editor = new Editor({
    element: document.createElement('div'),
    extensions: [
      StarterKit, Link, DetailsExtension, PdfExtension, LocationExtension, AudioExtension,
      WhiteboardExtension, SynVisualExtension, VideoExtension, ImageGallery, CustomImage, HtmlBlockBreaks, Markdown.configure({ html: true }),
    ],
    content,
  });
  editors.push(editor);
  return editor;
};

const markdownOf = (e: Editor) => (e.storage as any).markdown.getMarkdown() as string;

describe('a paragraph after a toggle', () => {
  it('keeps its link when the note is opened again', () => {
    const typed = open({
      type: 'doc',
      content: [
        {
          type: 'details',
          attrs: { summary: 'Toggle', open: true },
          content: [{ type: 'paragraph', content: [{ type: 'text', text: 'Vợ đi du lịch Phú Quốc' }] }],
        },
        {
          type: 'paragraph',
          content: [
            { type: 'text', text: 'Thread về việc rà soát domain, TLS cert: ' },
            { type: 'text', text: LINK, marks: [{ type: 'link', attrs: { href: LINK } }] },
          ],
        },
      ],
    });
    const saved = markdownOf(typed);
    expect(saved).toMatch(/<\/details>\n\n/);

    // The other device: the file, opened.
    const opened = open(saved);
    const link = opened.view.dom.querySelector('a');
    expect(link?.getAttribute('href')).toBe(LINK);
    expect(opened.getText()).toContain('Thread về việc rà soát domain, TLS cert:');
    expect(markdownOf(opened)).toBe(saved);
  });
});

/** Every other block saved as HTML, followed by the same paragraph. */
describe('a paragraph after any block saved as HTML', () => {
  const blocks: [string, object][] = [
    ['pdf', { type: 'pdf', attrs: { src: 'Files/a.pdf', title: 'A' } }],
    ['location', { type: 'location', attrs: { lat: 21.03, lng: 105.85, label: 'Hà Nội' } }],
    ['audio', { type: 'audio', attrs: { src: 'assets/a.mp3' } }],
    ['whiteboard', { type: 'whiteboard', attrs: { boardId: 'b1', boardPath: 'Boards/b1.json', title: 'Plan' } }],
    ['video', { type: 'video', attrs: { src: 'assets/a.mp4' } }],
    ['gallery', { type: 'imageGallery', attrs: { template: 'grid', images: [{ src: 'assets/a.png', alt: 'a' }] } }],
  ];

  it.each(blocks)('%s', (_name, block) => {
    const typed = open({
      type: 'doc',
      content: [
        block,
        {
          type: 'paragraph',
          content: [
            { type: 'text', text: 'Xem: ' },
            { type: 'text', text: LINK, marks: [{ type: 'link', attrs: { href: LINK } }] },
          ],
        },
      ],
    });
    const saved = markdownOf(typed);
    expect(saved).toMatch(/>\n\nXem: /);
    expect(open(saved).view.dom.querySelector('a')?.getAttribute('href')).toBe(LINK);
  });
});

/** A note saved before the fix opens with its link, and is saved fixed. */
describe('a note written the old way', () => {
  const OLD =
    '<details class="synabit-details">\n<summary>Toggle</summary>\n<div class="details-content">\n\nVợ đi du lịch Phú Quốc\n\n\n</div>\n</details>\n' +
    `Thread về việc rà soát domain, TLS cert: <${LINK}>`;

  it('opens with the link, and saves with the blank line', () => {
    const opened = open(OLD);
    expect(opened.view.dom.querySelector('a')?.getAttribute('href')).toBe(LINK);
    expect(markdownOf(opened)).toMatch(/<\/details>\n\nThread về việc/);
  });

  it('is opened the same however many notes were opened before it', () => {
    for (let i = 0; i < 5; i++) open(OLD);
    expect(markdownOf(open(OLD))).toBe(markdownOf(open(OLD)));
  });

  it('leaves alone what is not text after a closed block', () => {
    const nested = '<details>\n<div class="details-content">\n\nx\n\n</div>\n</details>\n';
    expect(separateHtmlBlocks(nested)).toBe(nested);
    const tagAfter = '</div>\n<div data-type="whiteboard"></div>';
    expect(separateHtmlBlocks(tagAfter)).toBe(tagAfter);
    const code = '```html\n</details>\ntext\n```\n';
    expect(separateHtmlBlocks(code), 'a code block is what it says').toBe(code);
    expect(separateHtmlBlocks('</details>\n<https://a.b/c>')).toBe('</details>\n\n<https://a.b/c>');
    expect(separateHtmlBlocks('<video src="a.mp4" controls="true"></video>\nXem')).toBe('<video src="a.mp4" controls="true"></video>\n\nXem');
  });
});

/** An image is a block: what follows it keeps its own line, and its kind. */
describe('a block after an image', () => {
  it.each([
    ['plain', { src: 'assets/a.png', alt: 'image.png' }],
    ['with attributes', { src: 'assets/a.png', alt: 'image.png', width: '300', align: 'left' }],
  ])('%s: a heading stays a heading', (_name, attrs) => {
    const typed = open({
      type: 'doc',
      content: [
        { type: 'image', attrs },
        { type: 'heading', attrs: { level: 2 }, content: [{ type: 'text', text: 'Unix Time' }] },
      ],
    });
    const saved = markdownOf(typed);
    expect(saved).toMatch(/\n\n## Unix Time/);
    const reopened = open(saved);
    expect(reopened.state.doc.lastChild?.type.name).toBe('heading');
    expect(markdownOf(reopened)).toBe(saved);
  });
});

/**
 * A visual kept from the chat is one line pointing at its node, and it stays
 * that line, with the paragraph after it its own, through a save. Without its
 * extension the editor would not know the block, and would drop it.
 */
describe('a kept visual in a note', () => {
  const ID = '0123456789abcdef0123456789abcdef';
  const BLOCK = `<div data-type="syn-visual" data-visual="${ID}"></div>`;

  it('survives a save, and so does what follows it', () => {
    const md = markdownOf(open(`${BLOCK}\n\nCon lắc đơn, chu kỳ theo chiều dài.`));
    expect(md).toContain(BLOCK);
    expect(md).toContain('\n\nCon lắc đơn, chu kỳ theo chiều dài.');
    expect(markdownOf(open(md))).toBe(md);
  });

  it('keeps only an id in the attribute', () => {
    const md = markdownOf(open(`<div data-type="syn-visual" data-visual="${ID}&quot; onload=&quot;x"></div>`));
    expect(md).toContain(`data-visual="${ID}"`);
    expect(md).not.toContain('onload');
  });
});
