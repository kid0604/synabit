import { Node, mergeAttributes } from '@tiptap/core';
import { NodeSelection, Plugin, PluginKey, type EditorState, type Transaction } from '@tiptap/pm/state';
import type { Node as PMNode } from '@tiptap/pm/model';
import { closeHistory } from '@tiptap/pm/history';
import { VueNodeViewRenderer } from '@tiptap/vue-3';
import { i18n } from '../../../i18n';
import './richBlocks.css';
import RichTableNodeView from '../nodes/RichTableNodeView.vue';
import { richTablePlugin } from '../../../shared/rich-table/markdownIt';
import { emptyTable } from '../../../shared/rich-table/model';
import { parseRichTable, serializeRichTable } from '../../../shared/rich-table/markdown';
import { computeTable, tablesRead, noteTables, nameInSource, type NoteTable } from '../../../shared/rich-table/formulas';
import type { RichTable } from '../../../shared/rich-table/model';
import { rebase } from '../../../shared/rich-table/merge';

const editsKey = new PluginKey<Edit[]>('richTableEdits');

/**
 * How many Rich Tables sit where their lines cannot be written back as one:
 * inside a list or a quote, whose lines carry `- ` or `> `. At the top of the
 * note, or in a Details block there (whose inside is written as plain
 * Markdown), a table is where it can live.
 */
function misplaced(doc: PMNode, name: string): number {
  let count = 0;
  const walk = (node: PMNode, ok: boolean) => {
    node.forEach((child) => {
      if (child.type.name === name) {
        if (!ok) count++;
        return;
      }
      if (child.isTextblock || child.isAtom) return;
      walk(child, ok && child.type.name === 'details');
    });
  };
  walk(doc, true);
  return count;
}

/** One recorded change to one table's source, by someone in this editor or by the compute plugin. */
interface Edit { before: string; after: string; computed: boolean }

/** The sources of a document's Rich Tables, in order. */
function sources(doc: PMNode, name: string): string[] {
  const out: string[] = [];
  doc.forEach((n) => { if (n.type.name === name) out.push(n.attrs.source); });
  return out;
}

/**
 * The source a step of the history left a table at, from the source it
 * started from: the edit, then the formulas it caused to be recomputed —
 * one event in the history, two transactions here.
 */
function after(edits: Edit[], before: string): string | null {
  const edit = [...edits].reverse().find((e) => !e.computed && e.before === before);
  if (!edit) return null;
  let at = edit.after;
  for (const e of edits) if (e.computed && e.before === at) at = e.after;
  return at;
}

/** The source a history step started from, given the one it left: `after`, backwards. */
function before(edits: Edit[], result: string): string | null {
  let at = result;
  const computed = [...edits].reverse().find((e) => e.computed && e.after === at);
  if (computed) at = computed.before;
  const edit = [...edits].reverse().find((e) => !e.computed && e.after === at);
  return edit ? edit.before : null;
}

/**
 * Set on a transaction to have every Rich Table's formulas checked and, where
 * stale, rewritten — what a table does once when it is shown, so that
 * `TODAY()` is today and a value edited outside the editor is caught up.
 */
export const RICH_TABLE_REFRESH = 'richTableRefresh';
const computeKey = new PluginKey('richTableCompute');

/**
 * Bring every formula in the note up to date, in one transaction, or `null`
 * when all of them are. Tables are read lazily: one with no formulas, read by
 * no formula, is never parsed.
 */
function recompute(state: EditorState, name: string, refresh: boolean): Transaction | null {
  const found: { node: PMNode; pos: number }[] = [];
  state.doc.forEach((node, pos) => { if (node.type.name === name) found.push({ node, pos }); });
  if (!found.length) return null;
  if (!found.some((f) => /type:\s*formula/.test(f.node.attrs.source))) return null;

  const entries: NoteTable[] = found.map((f) => {
    const result = parsedNode(f.node);
    return {
      table: result.readOnly ? null : result.table,
      name: result.readOnly ? result.table.name ?? nameInSource(f.node.attrs.source) : result.table.name,
    };
  });
  const { byName, held } = noteTables(entries);
  const tables = entries.map((e) => e.table);
  const withFormulas = found
    .map((_, i) => i)
    .filter((i) => !held.has(i) && tables[i]?.columns.some((c) => c.type === 'formula'));
  if (!withFormulas.length) return null;

  const now = new Date();
  const changed = new Set<number>();
  const reads = new Map<number, Set<string>>();
  // Tables may read each other: a table is computed again only when one it
  // reads changed in the pass before, and a few passes settle a chain (a
  // ring never reaches here — `held`).
  let pending = new Set(withFormulas);
  for (let pass = 0; pass < 3 && pending.size; pass++) {
    const changedNow = new Set<string>();
    for (const i of pending) {
      const table = tables[i]!;
      if (!reads.has(i)) reads.set(i, tablesRead(table));
      const others: Record<string, RichTable> = {};
      for (const other of reads.get(i)!) {
        const j = byName.get(other);
        const t = j === undefined || j === i ? null : tables[j];
        if (t) others[other] = t;
      }
      const next = computeTable(table, others, now).table;
      if (next === table) continue;
      // Opening a note is not an edit: a clock that moved is shown, but not
      // written — or every open would change the file, minute by minute.
      if (refresh && onlyClocks(table, next)) continue;
      tables[i] = next;
      changed.add(i);
      if (next.name) changedNow.add(next.name);
    }
    pending = new Set(withFormulas.filter((i) => [...(reads.get(i) ?? [])].some((n) => changedNow.has(n))));
  }
  if (!changed.size) return null;
  const tr = state.tr;
  for (const i of changed) {
    tr.setNodeMarkup(found[i].pos, undefined, { ...found[i].node.attrs, source: serializeRichTable(tables[i]!) });
  }
  return tr.setMeta(computeKey, true);
}

/** Whether every cell that changed is in a column whose formula reads the time of day. */
function onlyClocks(before: RichTable, after: RichTable): boolean {
  const clock = before.columns.map((c) => c.type === 'formula' && /\bNOW\s*\(/i.test(c.expr ?? ''));
  for (let r = 0; r < after.rows.length; r++) {
    if (after.rows[r] === before.rows[r]) continue;
    for (let c = 0; c < after.rows[r].length; c++) {
      if (!clock[c] && (after.rows[r][c] ?? '') !== (before.rows[r]?.[c] ?? '')) return false;
    }
  }
  return true;
}

/** Tables read before, by node: an unchanged node is the same object, so a keystroke elsewhere parses nothing. */
const parsedNodes = new WeakMap<PMNode, ReturnType<typeof parseRichTable>>();
function parsedNode(node: PMNode): ReturnType<typeof parseRichTable> {
  let result = parsedNodes.get(node);
  if (!result) {
    result = parseRichTable(node.attrs.source);
    parsedNodes.set(node, result);
  }
  return result;
}

declare module '@tiptap/core' {
  interface Commands<ReturnType> {
    richTable: {
      /** A new Rich Table with these columns and three empty rows. */
      insertRichTable: (columnNames: string[]) => ReturnType;
      /**
       * Replace the source of the Rich Table at `pos`, as one step of its own
       * in the history — so undo takes back one edit, not every edit made in
       * the same half-second.
       */
      setRichTableSource: (pos: number, source: string) => ReturnType;
      /** Several Rich Tables at once, as one step of the history. */
      setRichTableSources: (edits: { pos: number; source: string }[]) => ReturnType;
      /**
       * Check every Rich Table's formulas and write those that are stale,
       * outside the undo history: nothing anybody did, only time passing.
       */
      refreshRichTables: () => ReturnType;
      /** Put the cursor in the first cell of the Rich Table that is selected, or just before the cursor. */
      enterRichTable: () => ReturnType;
    };
  }
}

/**
 * A Rich Table: a block of its own, beside the ordinary table and not
 * instead of it. The node holds the Markdown it came from — the pipe table
 * and the `<!-- rich-table` comment — and writes that back untouched until
 * somebody edits it, so opening and saving a note never rewrites a table.
 * `docs/rich-table-2026-10-05.md`.
 */
export interface RichTableOptions {
  /** Notes whose title matches, for a Note column's picker. The editor that mounts this provides it. */
  searchNotes?: (query: string) => Promise<{ id: string; title: string }[]>;
}

export const RichTableExtension = Node.create<RichTableOptions>({
  name: 'richTable',

  addOptions() {
    return { searchNotes: undefined };
  },
  group: 'block',
  atom: true,
  selectable: true,
  draggable: false,

  addAttributes() {
    return {
      source: {
        default: '',
        parseHTML: (el) => el.getAttribute('data-source') ?? '',
        renderHTML: (attrs) => ({ 'data-source': attrs.source }),
      },
    };
  },

  parseHTML() {
    return [{ tag: 'div[data-type="rich-table"]' }];
  },

  renderHTML({ HTMLAttributes }) {
    return ['div', mergeAttributes(HTMLAttributes, { 'data-type': 'rich-table' })];
  },

  addNodeView() {
    return VueNodeViewRenderer(RichTableNodeView as any, {
      // Everything inside the grid is the grid's: its keys, its clicks, its
      // copy and paste. ProseMirror sees only what happens on the frame.
      stopEvent: ({ event }) => {
        if (event.type.startsWith('drag') || event.type === 'drop') return false;
        const target = event.target as Element | null;
        return !!target?.closest?.('[data-rt-interactive]');
      },
    });
  },

  addCommands() {
    return {
      insertRichTable:
        (columnNames: string[]) =>
        ({ commands }) =>
          commands.insertContent({
            type: this.name,
            attrs: { source: serializeRichTable(emptyTable(columnNames)) },
          }),
      setRichTableSource:
        (pos: number, source: string) =>
        ({ tr, dispatch }) => {
          const node = tr.doc.nodeAt(pos);
          if (node?.type.name !== this.name) return false;
          if (dispatch) {
            closeHistory(tr);
            tr.setNodeMarkup(pos, undefined, { ...node.attrs, source });
          }
          return true;
        },
      setRichTableSources:
        (edits: { pos: number; source: string }[]) =>
        ({ tr, dispatch }) => {
          if (!edits.every((e) => tr.doc.nodeAt(e.pos)?.type.name === this.name)) return false;
          if (dispatch) {
            closeHistory(tr);
            for (const e of edits) tr.setNodeMarkup(e.pos, undefined, { ...tr.doc.nodeAt(e.pos)!.attrs, source: e.source });
          }
          return true;
        },
      enterRichTable:
        () =>
        ({ state, view }) => {
          const { selection } = state;
          let pos: number | null = null;
          if (selection instanceof NodeSelection && selection.node.type.name === this.name) pos = selection.from;
          else {
            const before = selection.$from.nodeBefore;
            if (before?.type.name === this.name) pos = selection.from - before.nodeSize;
          }
          if (pos === null) return false;
          const dom = view.nodeDOM(pos);
          if (!(dom instanceof HTMLElement)) return false;
          dom.dispatchEvent(new CustomEvent('rich-table-enter', { detail: 'top' }));
          return true;
        },
      refreshRichTables:
        () =>
        ({ tr, dispatch }) => {
          if (dispatch) tr.setMeta(RICH_TABLE_REFRESH, true).setMeta('addToHistory', false);
          return true;
        },
    };
  },

  addKeyboardShortcuts() {
    // Selected as a block — arrived at with an arrow key, or left with Esc —
    // the table is entered with Enter or another arrow toward it.
    const enter = (from: 'top' | 'bottom') => () => {
      const { selection } = this.editor.state;
      if (!(selection instanceof NodeSelection) || selection.node.type.name !== this.name) return false;
      const dom = this.editor.view.nodeDOM(selection.from);
      if (!(dom instanceof HTMLElement)) return false;
      dom.dispatchEvent(new CustomEvent('rich-table-enter', { detail: from }));
      return true;
    };
    return { Enter: enter('top'), ArrowDown: enter('top'), ArrowUp: enter('bottom') };
  },

  addStorage() {
    return {
      markdown: {
        serialize(state: any, node: any) {
          // As it came, blank lines and all, and unescaped: it is already
          // Markdown. `text` writes it line by line, so the block keeps its
          // place however the serializer is indenting around it.
          state.text(String(node.attrs.source), false);
          state.closeBlock(node);
        },
        parse: {
          setup(markdownit: any) {
            richTablePlugin(markdownit);
          },
        },
      },
    };
  },

  addProseMirrorPlugins() {
    const name = this.name;
    return [
      // Undo and redo, when the table changed from outside since: the step is
      // rebased onto what is there now instead of setting a whole old source
      // back over it (`shared/rich-table/merge.ts`).
      new Plugin<Edit[]>({
        key: editsKey,
        state: {
          init: () => [],
          apply(tr, edits, oldState, newState) {
            if (!tr.docChanged || tr.getMeta('addToHistory') === false || tr.getMeta('history$')) return edits;
            const was = sources(oldState.doc, name);
            const now = sources(newState.doc, name);
            if (was.length !== now.length) return edits;
            const added: Edit[] = [];
            was.forEach((s, i) => {
              if (s !== now[i]) added.push({ before: s, after: now[i], computed: !!tr.getMeta(computeKey) });
            });
            return added.length ? [...edits, ...added].slice(-400) : edits;
          },
        },
        appendTransaction(transactions, oldState, newState) {
          const history = transactions.find((t) => t.getMeta('history$'));
          if (!history) return null;
          const edits = editsKey.getState(oldState) ?? [];
          const redo = !!history.getMeta('history$')?.redo;
          const was = sources(oldState.doc, name);
          const now = sources(newState.doc, name);
          if (was.length !== now.length) return null;
          let tr: Transaction | null = null;
          now.forEach((target, i) => {
            const current = was[i];
            if (target === current) return;
            // What the step left, or started from: the base the change is worked out against.
            const base = redo ? before(edits, target) : after(edits, target);
            if (base === null || base === current) return;
            // Compared in one spelling: a hand-padded table and the same table
            // as the editor writes it differ on every line, and a patch read
            // across that difference lands in the wrong place.
            const canon = (s: string) => {
              const p = parseRichTable(s);
              return p.readOnly ? s : serializeRichTable(p.table);
            };
            const merged = rebase(canon(base), canon(target), canon(current));
            if (merged === target) return;
            let pos = -1;
            let k = 0;
            newState.doc.forEach((n, at) => { if (n.type.name === name && k++ === i) pos = at; });
            if (pos === -1) return;
            tr ??= newState.tr;
            tr.setNodeMarkup(pos, undefined, { ...newState.doc.nodeAt(pos)!.attrs, source: merged });
          });
          return tr;
        },
      }),
      new Plugin({
        key: new PluginKey('richTableTopLevel'),
        // A Rich Table lives at the top of the note. Inside a list or a quote
        // its lines would be written with `> ` or an indent in front, which
        // nothing reads back as a Rich Table — the comment, and with it every
        // column's type, would be lost on the next open. So it does not go
        // there: a change that would put it there is not made.
        //
        // Only a change that puts one there more is refused. A note that
        // arrived with a table already somewhere it should not be — written
        // by hand, by Syn — must still be editable everywhere else; refusing
        // every change to such a note froze the editor whole.
        filterTransaction(tr) {
          if (!tr.docChanged) return true;
          return misplaced(tr.doc, name) <= misplaced(tr.before, name);
        },
      }),
      new Plugin({
        key: computeKey,
        // Formula cells are written into the note (design §4.5), and written
        // in the same step as the edit that changed them: undo takes back
        // both, and the file never holds a value its formula disagrees with.
        appendTransaction(transactions, oldState, newState) {
          if (transactions.some((t) => t.getMeta(computeKey))) return null;
          const refresh = transactions.some((t) => t.getMeta(RICH_TABLE_REFRESH));
          if (!refresh) {
            if (!transactions.some((t) => t.docChanged)) return null;
            const before = new Set<PMNode>();
            oldState.doc.forEach((n) => { if (n.type.name === name) before.add(n); });
            let touched = false;
            newState.doc.forEach((n) => { if (n.type.name === name && !before.has(n)) touched = true; });
            if (!touched) return null;
          }
          return recompute(newState, name, refresh);
        },
      }),
    ];
  },
});

/**
 * A Rich Table's configuration that lost its table: its `<!-- rich-table`
 * comment, kept word for word and written back as it came, with a note on
 * screen saying what happened — rather than dropped on the next save.
 */
export const RichTableOrphan = Node.create({
  name: 'richTableOrphan',
  group: 'block',
  atom: true,
  selectable: true,

  addAttributes() {
    return {
      source: {
        default: '',
        parseHTML: (el) => el.getAttribute('data-source') ?? '',
        renderHTML: (attrs) => ({ 'data-source': attrs.source }),
      },
    };
  },

  parseHTML() {
    return [{ tag: 'div[data-type="rich-table-orphan"]' }];
  },

  renderHTML({ HTMLAttributes }) {
    return ['div', mergeAttributes(HTMLAttributes, { 'data-type': 'rich-table-orphan' })];
  },

  addNodeView() {
    return ({ node }) => {
      const dom = document.createElement('div');
      dom.className = 'rt-orphan';
      dom.contentEditable = 'false';
      const note = document.createElement('div');
      note.className = 'rt-orphan-note';
      note.textContent = i18n.global.t('rich_table.orphan');
      const pre = document.createElement('pre');
      pre.textContent = node.attrs.source;
      dom.append(note, pre);
      return { dom };
    };
  },

  addStorage() {
    return {
      markdown: {
        serialize(state: any, node: any) {
          state.text(String(node.attrs.source), false);
          state.closeBlock(node);
        },
        parse: {
          setup(markdownit: any) {
            richTablePlugin(markdownit);
          },
        },
      },
    };
  },
});
