/**
 * Random notes, for property tests: documents in the note editor's own schema,
 * built from every block and mark it has, nested, with text full of the
 * characters markdown means something by.
 *
 * Not a fixture of cases somebody thought of. The faults found on 2026-10-15 —
 * a link after a toggle, a heading after an image — were each one block next
 * to another, which nobody had written a test for and a generator writes in
 * its first hundred tries.
 */
import fc from 'fast-check';

type Json = Record<string, unknown>;

/** Words a note holds, and words that mean something to markdown or HTML. */
const WORDS = [
  'Cam', 'golive', 'CMC', 'Phú', 'Quốc', 'nhiệt', 'độ', 'đã', 'lưu', 'TLS', 'cert', 'Minh', 'thuốc',
  '2026-10-15', '57,5°C', '🙂', 'v1.2', 'a_b', 'x*y', '#tag', '@Minh', '<b>', '</div>', '</details>',
  '<https://x.y/z>', '[x]', '[[2026-09-24]]', '![[Board]]', '\\', '`code`', '|', '~~', '&amp;', '&',
  '$x^2$', '1.', '-', '+', '>', '*', '_', '**', '#', '##', '---', '***', '$$', '{', '}', '(', ')',
  'https://a.b/c?d=1&e=2', '<!--', '-->', '[^1]', '==mark==',
];

/** Short runs of markdown's own characters, for escaping to get right. */
const symbols = fc.string({ unit: fc.constantFrom(...'ab*_#<>[]\\`|~&$!-+.:()=^'.split('')), minLength: 1, maxLength: 5 });

const word = fc.oneof({ weight: 4, arbitrary: fc.constantFrom(...WORDS) }, { weight: 1, arbitrary: symbols });
/** Words with single spaces: markdown keeps neither runs of spaces nor spaces at the ends. */
const words = fc.array(word, { minLength: 1, maxLength: 6 }).map((ws) => ws.join(' '));

const HREFS = [
  'https://example.com/a?x=1&y=2',
  'https://chat.google.com/room/AAQAv1dGQyM/qbDhtbXAlT0/qbDhtbXAlT0?cls=10',
  'synabit://note/Notes%2Fa.md',
  'synabit://person/People/l__anh_kh_i_1777890732879.md',
  'https://a.b/c_(d)',
  'https://a.sharepoint.com/%5C_layouts/Doc.aspx?file=%5B1%5D',
  'mailto:a@b.c',
];
const href = fc.constantFrom(...HREFS);

const STYLE = ['bold', 'italic', 'strike', 'underline', 'highlight'] as const;

/** The marks on one run of text. `code` stands alone, as the editor allows. */
const marks: fc.Arbitrary<Json[]> = fc.oneof(
  { weight: 3, arbitrary: fc.constant<Json[]>([]) },
  { weight: 2, arbitrary: fc.subarray([...STYLE], { minLength: 1 }).map((ms): Json[] => ms.map((type) => ({ type }))) },
  { weight: 1, arbitrary: fc.constant<Json[]>([{ type: 'code' }]) },
  {
    weight: 2,
    arbitrary: fc.tuple(href, fc.boolean()).map(([h, bold]): Json[] => [
      { type: 'link', attrs: { href: h } },
      ...(bold ? [{ type: 'bold' }] : []),
    ]),
  },
);

const run: fc.Arbitrary<Json> = fc.record({ text: words, marks }).map(({ text, marks }) => ({ type: 'text', text, ...(marks.length ? { marks } : {}) }));
const equation: fc.Arbitrary<Json> = fc.constantFrom('x^2', '\\frac{a}{b}', 'E=mc^2').map((latex) => ({ type: 'equation', attrs: { latex } }));

/** A line's content: runs of text, an equation now and then, a line break between. */
export const inline: fc.Arbitrary<Json[]> = fc
  .array(fc.oneof({ weight: 5, arbitrary: run }, { weight: 1, arbitrary: equation }), { minLength: 1, maxLength: 4 })
  .chain((parts) =>
    fc.array(fc.boolean(), { minLength: parts.length, maxLength: parts.length }).map((breaks) =>
      parts.flatMap((p, i) => (i > 0 && breaks[i] ? [{ type: 'hardBreak' }, p] : [p])),
    ),
  )
  // Two runs side by side need a space between, or their words run together.
  .map((parts) =>
    parts.flatMap((p, i) =>
      i > 0 && p.type === 'text' && parts[i - 1].type === 'text' ? [{ type: 'text', text: ' ' }, p] : [p],
    ),
  );

const paragraph = inline.map((content) => ({ type: 'paragraph', content }));
const heading = fc.tuple(fc.integer({ min: 1, max: 6 }), inline).map(([level, content]) => ({ type: 'heading', attrs: { level }, content }));

const CODE_LINES = ['const a = 1;', '```', '~~~', '<details>', '</div>', '# not a heading', '    indented', '- not a list', '| a | b |', 'echo "$HOME"', ''];
const codeBlock = fc
  .tuple(fc.constantFrom('', 'js', 'bash', 'python', 'json'), fc.array(fc.constantFrom(...CODE_LINES), { minLength: 1, maxLength: 4 }))
  .map(([language, lines]) => {
    const text = lines.join('\n').replace(/^\n+|\n+$/g, '') || 'x';
    return { type: 'codeBlock', attrs: { language: language || null }, content: [{ type: 'text', text }] };
  });

const image = fc
  .tuple(word, fc.boolean(), fc.constantFrom('left', 'right', 'center'))
  .map(([alt, sized, align]) => ({
    type: 'image',
    attrs: { src: 'assets/1777045154-image.png', alt: alt.replace(/[[\]]/g, ''), ...(sized ? { width: '300', align } : {}) },
  }));

const embeds: fc.Arbitrary<Json>[] = [
  fc.constant({ type: 'horizontalRule' }),
  fc.constant({ type: 'pdf', attrs: { src: 'Files/a.pdf', title: 'Hợp đồng' } }),
  fc.constantFrom('assets/a.mp4', 'https://www.youtube.com/watch?v=dQw4w9WgXcQ').map((src) => ({ type: 'video', attrs: { src } })),
  fc.constant({ type: 'audio', attrs: { src: 'assets/a.mp3' } }),
  fc.constant({ type: 'location', attrs: { lat: 21.03, lng: 105.85, label: 'Hà Nội', zoom: 13 } }),
  fc.constant({ type: 'whiteboard', attrs: { boardId: 'b1', boardPath: 'Boards/b1.json', title: 'Plan' } }),
  fc.constant({ type: 'imageGallery', attrs: { template: 'grid', images: [{ src: 'assets/a.png', alt: 'a' }, { src: 'assets/b.png', alt: 'b' }] } }),
];

/**
 * Known limits: what markdown cannot hold, left out here on purpose, each with
 * why. A failure the generator finds is fixed if a person could meet it; one
 * that markdown has no way to write goes here instead, narrowly.
 *
 * - `\|` inside code in a table cell. A table row is split on pipes before
 *   its cells are read, and GFM reads `\|` as an escaped pipe everywhere in
 *   the row, code included; there is no way to keep the backslash.
 */
function withinLimits(cell: Json[]): Json[] {
  return cell.map((part) =>
    part.type === 'text' && Array.isArray(part.marks) && part.marks.some((m: Json) => m.type === 'code')
      ? { ...part, text: String(part.text).replace(/\\(?=\|)/g, '') || 'x' }
      : part,
  );
}

/** A table as markdown can hold one: a header row, then rows of single paragraphs. */
const table = fc
  .tuple(fc.integer({ min: 1, max: 3 }), fc.integer({ min: 1, max: 3 }))
  .chain(([rows, cols]) =>
    fc.array(fc.array(inline.map(withinLimits), { minLength: cols, maxLength: cols }), { minLength: rows + 1, maxLength: rows + 1 }),
  )
  .map((grid) => ({
    type: 'table',
    content: grid.map((cells, r) => ({
      type: 'tableRow',
      content: cells.map((content) => ({ type: r === 0 ? 'tableHeader' : 'tableCell', content: [{ type: 'paragraph', content }] })),
    })),
  }));

/** A block, nested to a small depth: lists, quotes and toggles hold blocks. */
const { block } = fc.letrec<{ block: Json; blocks: Json[] }>((tie) => ({
  block: fc.oneof(
    { depthSize: 'small', withCrossShrink: true },
    { weight: 6, arbitrary: paragraph },
    { weight: 2, arbitrary: heading },
    { weight: 1, arbitrary: codeBlock },
    { weight: 1, arbitrary: image },
    { weight: 1, arbitrary: table },
    ...embeds.map((arbitrary) => ({ weight: 1, arbitrary })),
    { weight: 1, arbitrary: tie('blocks').map((content) => ({ type: 'blockquote', content })) },
    {
      weight: 2,
      arbitrary: fc
        .tuple(fc.constantFrom('bulletList', 'orderedList'), fc.array(fc.tuple(paragraph, tie('blocks')), { minLength: 1, maxLength: 3 }))
        .map(([type, items]) => ({
          type,
          content: items.map(([first, rest]) => ({ type: 'listItem', content: [first, ...(rest as Json[]).filter(listable)] })),
        })),
    },
    {
      weight: 1,
      arbitrary: fc.array(fc.tuple(fc.boolean(), paragraph), { minLength: 1, maxLength: 3 }).map((items) => ({
        type: 'taskList',
        content: items.map(([checked, p]) => ({ type: 'taskItem', attrs: { checked }, content: [p] })),
      })),
    },
    {
      weight: 1,
      arbitrary: fc.tuple(words, tie('blocks')).map(([summary, content]) => ({ type: 'details', attrs: { summary, open: true }, content })),
    },
  ),
  blocks: fc.array(tie('block'), { minLength: 1, maxLength: 3 }),
}));

/** What a list item may hold after its first paragraph, as markdown can nest it. */
function listable(b: Json): boolean {
  return ['paragraph', 'bulletList', 'orderedList', 'codeBlock', 'blockquote'].includes(String(b.type));
}

export const noteDoc: fc.Arbitrary<Json> = fc.array(block, { minLength: 1, maxLength: 6 }).map((content) => ({ type: 'doc', content }));
