<script setup lang="ts">
/**
 * A Rich Table in the editor. The node holds the table's Markdown; this reads
 * it, shows the grid, and writes an edit back as a new `source` — one undo
 * step per edit. A table that must not be written (its comment is broken, or
 * from a later version) is shown and left exactly as it is.
 */
import { computed, onBeforeUnmount, onMounted, ref } from 'vue';
import { useI18n } from 'vue-i18n';
import { NodeViewWrapper } from '@tiptap/vue-3';
import { Selection } from '@tiptap/pm/state';
import RichTableGrid from '../../../shared/rich-table/components/RichTableGrid.vue';
import { parseRichTable, serializeRichTable, shareRows, type ParseResult } from '../../../shared/rich-table/markdown';
import type { RichTable } from '../../../shared/rich-table/model';
import { renameForeignColumn, renamedColumn } from '../../../shared/rich-table/ops';
import { nameInSource, noteTables, type NoteTable } from '../../../shared/rich-table/formulas';

const props = defineProps<{
  node: any;
  editor: any;
  getPos: () => number | undefined;
  selected: boolean;
  deleteNode: () => void;
  extension: any;
}>();
const { t, locale } = useI18n();

/**
 * The table, read from the node's source, sharing every unchanged row with
 * the table before it — the one this view just wrote, or the last one read,
 * after an undo. Unchanged rows keep their arrays, which is what lets the
 * grid skip them.
 */
let written: { source: string; table: RichTable } | null = null;
let last: RichTable | undefined;
const parsed = computed<ParseResult>(() => {
  const source: string = props.node.attrs.source;
  const result = parseRichTable(source);
  if (!result.readOnly) {
    result.table = shareRows(written?.source === source ? written.table : last, result.table);
    last = result.table;
  }
  return result;
});
const editable = computed(() => props.editor?.isEditable !== false);
const readOnly = computed(() => !editable.value || !!parsed.value.readOnly);

/** The note's tables, as formulas see them — read when this table changes, which is when it is computed. */
function noteState() {
  const self = props.getPos();
  const entries: NoteTable[] = [];
  let at = -1;
  props.editor.state.doc.forEach((node: any, pos: number) => {
    if (node.type.name !== 'richTable') return;
    if (pos === self) at = entries.length;
    const result = parseRichTable(node.attrs.source);
    entries.push({
      table: result.readOnly ? null : result.table,
      name: result.readOnly ? result.table.name ?? nameInSource(node.attrs.source) : result.table.name,
    });
  });
  return { entries, at, ...noteTables(entries) };
}

const notice = computed(() => {
  const p = parsed.value;
  if (p.readOnly === 'meta') return t('rich_table.notice.meta');
  if (p.readOnly === 'version') return t('rich_table.notice.version');
  void props.node;
  const note = noteState();
  const self = note.entries[note.at];
  if (self?.name && note.ambiguous.has(self.name)) return t('rich_table.notice.same_name', { name: self.name });
  const held = note.held.get(note.at);
  if (held?.kind === 'ambiguous') return t('rich_table.notice.reads_ambiguous', { name: held.name });
  if (held?.kind === 'unreadable') return t('rich_table.notice.reads_unreadable', { name: held.name });
  if (held?.kind === 'cycle') return t('rich_table.notice.ring');
  const w = p.warnings[0];
  if (!w) return undefined;
  if (w.kind === 'extra-cells') return t('rich_table.notice.extra_cells', { n: w.row + 1 });
  if (w.kind === 'orphan-column') return t('rich_table.notice.orphan', { name: w.name });
  return t('rich_table.notice.duplicate', { name: w.name });
});

const grid = ref<InstanceType<typeof RichTableGrid> | null>(null);

function onChange(table: RichTable) {
  if (readOnly.value) return;
  const pos = props.getPos();
  if (typeof pos !== 'number') return;
  const source = serializeRichTable(table);
  const before = parsed.value.table;
  written = { source, table };
  // A column of a named table renamed: the other tables of the note that read
  // it as `name[Old]` follow, in the same step, so undo takes back all of it.
  const renamed = table.name ? renamedColumn(before, table) : null;
  const others: { pos: number; source: string }[] = [];
  if (renamed) {
    props.editor.state.doc.forEach((node: any, at: number) => {
      if (node.type.name !== 'richTable' || at === pos) return;
      const other = parseRichTable(node.attrs.source);
      if (other.readOnly) return;
      const next = renameForeignColumn(other.table, table.name!, renamed.from, renamed.to);
      if (next !== other.table) others.push({ pos: at, source: serializeRichTable(next) });
    });
  }
  if (others.length) props.editor.commands.setRichTableSources([{ pos, source }, ...others]);
  else props.editor.commands.setRichTableSource(pos, source);
}

/** Back to the text: above the table, below it, or the table itself as a block. */
function onExit(where: 'up' | 'down' | 'select') {
  const pos = props.getPos();
  if (typeof pos !== 'number') return;
  const { state, view } = props.editor;
  let selection: Selection;
  if (where === 'select') {
    props.editor.chain().focus().setNodeSelection(pos).run();
    return;
  }
  if (where === 'up') {
    selection = Selection.near(state.doc.resolve(pos), -1);
  } else {
    selection = Selection.near(state.doc.resolve(pos + props.node.nodeSize), 1);
  }
  view.dispatch(state.tr.setSelection(selection).scrollIntoView());
  view.focus();
}

/** The other named tables of the note, read when a formula editor asks for them. */
function others(): Record<string, RichTable> {
  const out: Record<string, RichTable> = {};
  const note = noteState();
  note.entries.forEach((e, i) => {
    // A name two tables share means neither of them.
    if (i !== note.at && e.table && e.name && !note.ambiguous.has(e.name)) out[e.name] = e.table;
  });
  return out;
}

function onNavigate(note: string) {
  window.dispatchEvent(new CustomEvent('synabit-navigate', { detail: { type: 'note', id: note } }));
}

/**
 * Into the grid from the keyboard. The extension's keymap sees the key while
 * the table is selected as a block, and sends it here as an event on the
 * node's element: Enter or ↓ arrive on the first row, ↑ on the last.
 */
const host = ref<HTMLElement | null>(null);
let wrapper: Element | null = null;
const onEnter = (e: Event) => grid.value?.enter((e as CustomEvent<'top' | 'bottom'>).detail);
onMounted(() => {
  wrapper = host.value?.closest('[data-node-view-wrapper]') ?? null;
  wrapper?.addEventListener('rich-table-enter', onEnter);
  // Shown, a table with formulas is brought up to date — `TODAY()` moves on,
  // and Syn or another device may have written values without computing them.
  // After the editor has finished drawing, never in the middle of it.
  if (editable.value && /type:\s*formula/.test(props.node.attrs.source)) {
    setTimeout(() => props.editor?.commands.refreshRichTables?.(), 0);
  }
});
onBeforeUnmount(() => wrapper?.removeEventListener('rich-table-enter', onEnter));
</script>

<template>
  <NodeViewWrapper class="rt-node" :class="{ 'is-selected': selected }">
    <span ref="host" hidden />
    <RichTableGrid
      ref="grid"
      :table="parsed.table"
      :locale="locale"
      :read-only="readOnly"
      :notice="notice"
      :others="others"
      :search-notes="extension?.options?.searchNotes"
      @change="onChange"
      @exit="onExit"
      @undo="editor.commands.undo()"
      @redo="editor.commands.redo()"
      @remove="deleteNode()"
      @navigate="onNavigate"
    />
  </NodeViewWrapper>
</template>

<style>
/* Selected as a block — and not being worked in, when the ring would only be noise. */
.rt-node.is-selected:not(:focus-within) > .rt > .rt-scroll {
  box-shadow: 0 0 0 2px color-mix(in srgb, var(--color-accent, #1a66cc) 45%, transparent);
}
</style>
