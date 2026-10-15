import { describe, it, expect, afterEach, vi } from 'vitest';
import { Editor } from '@tiptap/core';
import StarterKit from '@tiptap/starter-kit';
import Link from '@tiptap/extension-link';
import { Markdown } from 'tiptap-markdown';

// The node view is the screen's; what is saved is the markdown.
vi.mock('../../nodes/DetailsNodeView.vue', () => ({ default: {} }));
vi.mock('../../nodes/LocationNodeView.vue', () => ({ default: {} }));
vi.mock('../../nodes/WhiteboardNodeView.vue', () => ({ default: {} }));
vi.mock('../../nodes/ImageGalleryNode.vue', () => ({ default: {} }));
vi.mock('@tiptap/vue-3', async (real) => ({ ...(await real<object>()), VueNodeViewRenderer: () => null }));

import { DetailsExtension } from '../../extensions/DetailsExtension';
import { PdfExtension } from '../../PdfExtension';
import { LocationExtension } from '../../LocationExtension';
import { AudioExtension } from '../../AudioExtension';
import { WhiteboardExtension } from '../../WhiteboardExtension';
import { VideoExtension } from '../../VideoExtension';
import { ImageGallery } from '../../extensions/ImageGallery';
import { HtmlBlockBreaks, separateHtmlBlocks } from '../../extensions/HtmlBlockBreaks';

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
      WhiteboardExtension, VideoExtension, ImageGallery, HtmlBlockBreaks, Markdown.configure({ html: true }),
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
