import { Extension } from '@tiptap/core';
import { Plugin, PluginKey, type EditorState } from '@tiptap/pm/state';
import { Decoration, DecorationSet } from '@tiptap/pm/view';
import type { Node as PMNode } from '@tiptap/pm/model';
import { compile, run, Problem, FDate, FError, toText, type Value } from '../../../shared/formula';
import { parseRichTable } from '../../../shared/rich-table/markdown';
import { valueOf } from '../../../shared/rich-table/formulas';
import { formatDate, formatNumber, type RichTable } from '../../../shared/rich-table/model';
import { i18n } from '../../../i18n';
import './richBlocks.css';

/**
 * A formula in the middle of a sentence (design §15, question 1):
 *
 *     Tháng này đã tiêu `=SUM(chi-tieu[Số tiền])`.
 *
 * Inline code that begins with `=` and compiles against the note's named
 * Rich Tables is shown as its value — live, so it follows the table. With the
 * cursor inside, the code is shown again, to be edited. Read anywhere else
 * the note still says exactly what it computes, in code.
 *
 * Only code that compiles is taken: an `=` in a code example that is not a
 * formula of this note stays the code it was.
 */
const key = new PluginKey<DecorationSet>('inlineFormula');

interface Span { from: number; to: number; text: string }

/** Every run of inline code whose text begins with `=`. */
function formulaSpans(doc: PMNode): Span[] {
  const spans: Span[] = [];
  doc.descendants((node, pos) => {
    if (!node.isTextblock) return true;
    let run: Span | null = null;
    node.forEach((child, offset) => {
      const at = pos + 1 + offset;
      const code = child.isText && child.marks.some((m) => m.type.name === 'code');
      if (code) {
        if (run && run.to === at) {
          run.to = at + child.nodeSize;
          run.text += child.text ?? '';
        } else {
          if (run) spans.push(run);
          run = { from: at, to: at + child.nodeSize, text: child.text ?? '' };
        }
      } else if (run) {
        spans.push(run);
        run = null;
      }
    });
    if (run) spans.push(run);
    return false;
  });
  return spans.filter((s) => s.text.trimStart().startsWith('=') && s.text.trim().length > 1);
}

/** Tables read before, by node: an unchanged node is the same object, so moving the cursor parses nothing. */
const parsedTables = new WeakMap<PMNode, ReturnType<typeof parseRichTable>>();

/** The note's named Rich Tables, by name. */
function namedTables(doc: PMNode): Record<string, RichTable> {
  const out: Record<string, RichTable> = {};
  doc.forEach((node) => {
    if (node.type.name !== 'richTable') return;
    let parsed = parsedTables.get(node);
    if (!parsed) {
      parsed = parseRichTable(node.attrs.source);
      parsedTables.set(node, parsed);
    }
    if (parsed.table.name && !parsed.readOnly) out[parsed.table.name] = parsed.table;
  });
  return out;
}

const locale = () => String(i18n.global.locale.value ?? 'en');

/** A value as a sentence would say it: a number in the format of the column it came from. */
function say(v: Value, format: string | undefined): string {
  if (v instanceof FError) return v.toString();
  if (typeof v === 'number') return formatNumber(v, locale(), format);
  if (v instanceof FDate) return formatDate({ y: v.y, m: v.m, d: v.d }, locale());
  if (typeof v === 'boolean') return v ? '✓' : '✗';
  if (Array.isArray(v)) return v.map((x) => say(x, format)).join(', ');
  return toText(v);
}

function build(state: EditorState): DecorationSet {
  const spans = formulaSpans(state.doc);
  if (!spans.length) return DecorationSet.empty;
  const tables = namedTables(state.doc);
  const schema = {
    columns: [],
    tables: Object.fromEntries(Object.entries(tables).map(([n, t]) => [n, t.columns.map((c) => c.name)])),
  };
  const source = {
    rows: 1,
    now: new Date(),
    memo: new Map(),
    cell: () => null,
    column: () => [],
    other: (name: string, column: string) => {
      const t = tables[name];
      const at = t?.columns.findIndex((c) => c.name === column) ?? -1;
      return at === -1 ? [] : t.rows.map((r) => valueOf(t.columns[at], r[at] ?? ''));
    },
  };
  const { from: cursorFrom, to: cursorTo } = state.selection;
  const decorations: Decoration[] = [];
  for (const span of spans) {
    const text = span.text.trim().slice(1);
    const compiled = compile(text, schema);
    if (compiled instanceof Problem) continue;
    // Editing it: the code, as it is.
    if (cursorFrom <= span.to && cursorTo >= span.from) {
      decorations.push(Decoration.inline(span.from, span.to, { class: 'rt-inline-formula is-editing' }));
      continue;
    }
    let value: Value;
    try {
      value = run(compiled, source, 0);
    } catch {
      value = new FError('VALUE');
    }
    // The format of the first column it reads: `SUM(chi-tieu[Số tiền])` in đồng.
    const ref = /([\p{L}_][\p{L}\p{N}_-]*)\[([^\]]+)\]/u.exec(text);
    // A count is a count of rows, not an amount of the column's money.
    const counts = /^\s*(COUNT\w*|LEN|ROW)\s*\(/i.test(text);
    const format = ref && !counts ? tables[ref[1]]?.columns.find((c) => c.name === ref[2].trim())?.format : undefined;
    decorations.push(Decoration.inline(span.from, span.to, { class: 'rt-inline-formula-source' }));
    decorations.push(Decoration.widget(span.to, () => {
      const el = document.createElement('span');
      el.className = `rt-inline-value${value instanceof FError ? ' is-error' : ''}`;
      el.textContent = say(value, format);
      el.title = `=${text}`;
      return el;
    }, { side: 1, key: `${span.from}:${text}:${say(value, format)}` }));
  }
  return DecorationSet.create(state.doc, decorations);
}

export const InlineFormula = Extension.create({
  name: 'inlineFormula',

  addProseMirrorPlugins() {
    return [
      new Plugin<DecorationSet>({
        key,
        state: {
          init: (_, state) => build(state),
          apply: (tr, old, _before, after) => (tr.docChanged || tr.selectionSet ? build(after) : old.map(tr.mapping, tr.doc)),
        },
        props: {
          decorations: (state) => key.getState(state),
        },
      }),
    ];
  },
});
