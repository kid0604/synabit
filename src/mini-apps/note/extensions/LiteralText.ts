import { Node } from '@tiptap/core';

/**
 * Text, saved so it reads back as the same characters.
 *
 * tiptap-markdown writes `<` and `>` as entities so a typed `<b>` stays text,
 * but leaves `&` alone — and markdown decodes an entity wherever it is not in
 * code. So "&amp;", "&lt;" or "&nbsp;" typed in a note came back as "&", "<"
 * and a space: found by the note property test (`roundTrip.property.spec`), in
 * its second try, as highlighted "&amp; a". An `&` that would start an entity
 * is written as `&amp;`; any other `&` stays as typed, since it reads back
 * as itself.
 *
 * A `$` is written `\$`: the editor reads `$…$` as an equation, so "giá $5
 * và $10" typed as text came back as an equation `5 và `. In a table, `|` is
 * written `\|` for the same reason.
 *
 * Code is written by its own mark, and markdown decodes nothing inside it.
 */
const STARTS_AN_ENTITY = /&(?=#[0-9]+;|#[xX][0-9a-fA-F]+;|[a-zA-Z][a-zA-Z0-9]*;)/g;

export function escapeText(text: string): string {
  return text.replace(STARTS_AN_ENTITY, '&amp;').replace(/</g, '&lt;').replace(/>/g, '&gt;');
}

/**
 * Marks that open where a line starts, kept whole inside a quote.
 *
 * tiptap-markdown notes where a mark opens (`markString`) before writing it,
 * and the write then puts the line's prefix — `> ` in a quote — in front.
 * Its whitespace trimming, working from the noted place, overwrote the `>`:
 * "a⏎*b*" in a quote was saved as ` **b*`, and a bold second paragraph in a
 * quoted list as `****a**`, each coming back with the formatting gone and a
 * stray `*` — found by the note property test. In a list the prefix is spaces,
 * which the trimming skips, so only quotes broke.
 *
 * The fix is one line in tiptap-markdown's state, which it does not export:
 * write whatever the line is waiting for before the mark is noted. The class
 * is reached through the first state this serializer is handed.
 */
const FIXED = Symbol('markOpeningFixed');

export function fixSerializerState(state: any): void {
  const proto = Object.getPrototypeOf(state);
  if (!proto || proto[FIXED] || typeof proto.markString !== 'function') return;
  const noted = proto.markString;
  proto.markString = function (this: any, mark: any, open: boolean, parent: any, index: number) {
    // `write()` with nothing to write only closes the last block and puts in
    // the line's prefix — what the mark's own text would do a moment later.
    if (open) this.write();
    return noted.call(this, mark, open, parent, index);
  };

  // In a table, a `|` written as it is ends the cell, wherever it is: a table
  // row is split on pipes before a cell is read, so one inside a code span
  // split the cell and the table came back as text (found by the note
  // property test). Code spans, link addresses and HTML are written this way
  // and none of them is escaped; markdown-it takes `\|` back to `|` there.
  const write = proto.text;
  proto.text = function (this: any, text: string, escape = true) {
    if (this.inTable && !escape) text = text.replace(/\\?\|/g, (m) => (m === '|' ? '\\|' : m));
    return write.call(this, text, escape);
  };

  // A bullet list and a task list both write `- `, so one right after the
  // other was read back as one list, and the bullet items gained checkboxes
  // (found by the note property test). A list that follows a list of the other
  // kind takes the other marker, `*`, which CommonMark reads as a new list.
  // `closed` is the block just written, until the next one writes anything.
  const renderList = proto.renderList;
  proto.renderList = function (this: any, node: any, delim: string, firstDelim: (i: number) => string) {
    const prev = this.closed;
    let marker = '-';
    if (prev && prev !== node && DASHED.has(prev.type.name) && DASHED.has(node.type.name) && prev.type.name !== node.type.name) {
      marker = markers.get(prev) === '*' ? '-' : '*';
    }
    if (DASHED.has(node.type.name)) markers.set(node, marker);
    const first = marker === '-' ? firstDelim : (i: number) => firstDelim(i).replace(/^-/, marker);
    return renderList.call(this, node, delim, first);
  };

  // A block written as HTML, or a table, needs a blank line after it even in
  // a tight list, where blocks are otherwise a line apart: an embed in a quote
  // in a list took the next list into its HTML (found by the note property
  // test), as a toggle took the next paragraph before (`HtmlBlockBreaks`).
  const flushClose = proto.flushClose;
  proto.flushClose = function (this: any, size = 2) {
    return flushClose.call(this, this.closed && NEEDS_A_BLANK_LINE.has(this.closed.type.name) ? Math.max(size, 2) : size);
  };
  proto[FIXED] = true;
}

/** Blocks after which markdown needs a blank line to end them. */
const NEEDS_A_BLANK_LINE = new Set([
  'details', 'pdf', 'location', 'audio', 'video', 'whiteboard', 'imageGallery', 'image', 'table', 'richTable', 'richChart',
]);

/** Lists written with `- `. */
const DASHED = new Set(['bulletList', 'taskList']);
/** The marker each list was written with, while the save is going on. */
const markers = new WeakMap<object, string>();

/**
 * Whether the next text begins a line: the start of a block, or after a line
 * break (Shift+Enter), with the line's prefix (`> `, list indent) already out
 * or still to come.
 *
 * prosemirror-markdown escapes what would start a block — `1. `, `- `, `# ` —
 * only at the start of a block. After a line break, "1. Cam" came back as a
 * list (found by the note property test); a person typing their own numbering
 * after Shift+Enter is exactly who writes that.
 */
function atLineStart(state: any): boolean {
  if (state.atBlockStart) return true;
  const out: string = state.out;
  return out.endsWith('\n') || (!!state.delim && out.endsWith(`\n${state.delim}`));
}

/**
 * What `esc` leaves that still starts a block: `1)` lists; `1.` and `+` with
 * nothing after them, which start an empty list item (a paragraph of "1." was
 * read back as one, and the text was gone); and a line of `=` under text.
 */
function startOfLine(escaped: string): string {
  return escaped
    .replace(/^(\s*\d+)([.)])(\s|$)/, '$1\\$2$3')
    .replace(/^(\s*)\+$/, '$1\\+')
    .replace(/^(\s*)(=+)(\s*)$/, '$1\\$2$3');
}

/**
 * A markdown-it plugin applied to the editor's parser once, however often it
 * is asked for.
 *
 * tiptap-markdown runs every extension's parse setup on every parse, against
 * the same instance, and its task lists' setup is `md.use(taskListPlugin)`:
 * each note opened added the rule again, 52 copies after 50 parses, every one
 * run on every parse after — a session grew slower and larger the more notes
 * it opened (found when the note property test ran out of memory). The
 * equation's setup did the same until it was guarded. One guard here covers
 * any setup that does it next.
 */
const USED = Symbol('pluginsUsed');

export function usePluginsOnce(md: any): void {
  if (!md || md[USED]) return;
  const seen = new Set<unknown>();
  const use = md.use;
  md.use = function (this: any, plugin: unknown, ...options: unknown[]) {
    if (seen.has(plugin)) return this;
    seen.add(plugin);
    return use.call(this, plugin, ...options);
  };
  md[USED] = true;
}

export const LiteralText = Node.create({
  name: 'text',
  group: 'inline',

  onCreate() {
    usePluginsOnce((this.editor.storage as any).markdown?.parser?.md);
    // Fixed before the first save, not during it: one line of text through
    // the serializer is enough to be handed its state.
    const serializer = (this.editor.storage as any).markdown?.serializer;
    const { schema } = this.editor;
    serializer?.serialize(schema.topNodeType.create(null, schema.nodes.paragraph.create(null, schema.text('x'))));
  },

  addStorage() {
    return {
      markdown: {
        serialize(state: any, node: any, parent: any, index: number) {
          fixSerializerState(state);
          if (node.marks.some((m: any) => m.type.name === 'code')) {
            state.text(node.text);
            return;
          }
          // `\$` and `\|` written as they are: `state.text` would escape the
          // backslash. A `|` only means something in a table, where one typed
          // in a cell ended the cell, and the table came back as text.
          // A heading's last `#`s, after a space or alone, close it in markdown
          // and are dropped: a heading "##" came back empty (found by the note
          // property test). Written escaped, after the rest.
          let text: string = node.text;
          let closing = '';
          if (parent?.type.name === 'heading' && index === parent.childCount - 1) {
            const m = /(^|\s)(#+)\s*$/.exec(text);
            if (m) {
              text = text.slice(0, m.index + m[1].length);
              closing = m[2].replace(/#/g, '\\#');
            }
          }
          const special = state.inTable ? /([$|])/ : /([$])/;
          escapeText(text)
            .split(special)
            .forEach((part, i) => {
              if (part === '$' || (part === '|' && state.inTable)) state.write(`\\${part}`);
              else if (i === 0 && part && atLineStart(state)) state.write(startOfLine(state.esc(part, true)));
              else if (part) state.text(part);
            });
          if (closing) state.write(closing);
        },
        parse: {
          // markdown-it reads text.
        },
      },
    };
  },
});
