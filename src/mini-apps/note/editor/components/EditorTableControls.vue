<script setup lang="ts">
import { ref, onMounted, onBeforeUnmount } from 'vue';
import type { Editor } from '@tiptap/vue-3';
import { Plus, GripVertical, Palette } from 'lucide-vue-next';

const props = defineProps<{
  editor: Editor;
}>();

// --- Table Controls (Confluence-style) ---
const isInTable = ref(false);
const activeTableEl = ref<HTMLElement | null>(null);
const tableRect = ref({ top: 0, left: 0, width: 0, height: 0, bottom: 0, right: 0 });
const colPositions = ref<{ left: number; width: number }[]>([]);
const rowPositions = ref<{ top: number; height: number }[]>([]);
const activeRowIdx = ref(-1);
const activeColIdx = ref(-1);

// Context menu
const showCtxMenu = ref(false);
const ctxMenuPos = ref({ top: 0, left: 0 });
const canMerge = ref(false);
const canSplit = ref(false);

// Saved CellSelection for merge/split (tracked on selectionUpdate, restored on action)
let lastCellSelection: any = null;
let lastCanMerge = false;
let lastCanSplit = false;

// Track CellSelection on every selection change
// Key insight: right-click fires mousedown → creates TextSelection → onSelectionUpdate fires again
// We must NOT clear savedCellSelection in that case (user is still in table, just lost CellSelection)
const trackCellSelection = () => {
  const sel = props.editor.state.selection;
  if ((sel as any).$anchorCell) {
    // Active CellSelection — save it
    lastCellSelection = sel;
    lastCanMerge = props.editor.can().mergeCells();
    lastCanSplit = props.editor.can().splitCell();
  } else if (!props.editor.isActive('table')) {
    // User left the table entirely — clear saved state
    lastCellSelection = null;
    lastCanMerge = false;
    lastCanSplit = false;
  }
  // If user is in table but no CellSelection (e.g. after right-click),
  // we intentionally KEEP lastCellSelection so merge can restore it
};

const updateTableControls = () => {
  const inTable = props.editor.isActive('table');
  isInTable.value = inTable;
  if (!inTable) { activeTableEl.value = null; return; }
  
  canMerge.value = props.editor.can().mergeCells() || lastCanMerge;
  canSplit.value = props.editor.can().splitCell() || lastCanSplit;

  // Find the actual table DOM element
  const { from } = props.editor.state.selection;
  const domAtPos = props.editor.view.domAtPos(from);
  let el = domAtPos.node as HTMLElement;
  while (el && el.tagName !== 'TABLE') {
    el = el.parentElement as HTMLElement;
  }
  if (!el) return;
  activeTableEl.value = el;

  const wrapper = el.closest('.tiptap-wrapper');
  const wrapperRect = wrapper ? wrapper.getBoundingClientRect() : { top: 0, left: 0 };

  const rect = el.getBoundingClientRect();
  tableRect.value = { 
    top: rect.top - wrapperRect.top, 
    left: rect.left - wrapperRect.left, 
    width: rect.width, 
    height: rect.height, 
    bottom: rect.bottom - wrapperRect.top, 
    right: rect.right - wrapperRect.left 
  };

  // Read column positions from first row
  const firstRow = el.querySelector('tr');
  if (firstRow) {
    const cells = firstRow.querySelectorAll('td, th');
    colPositions.value = Array.from(cells).map(c => {
      const cr = c.getBoundingClientRect();
      return { left: cr.left - wrapperRect.left, width: cr.width };
    });
  }

  // Read row positions
  const rows = el.querySelectorAll('tr');
  rowPositions.value = Array.from(rows).map(r => {
    const rr = r.getBoundingClientRect();
    return { top: rr.top - wrapperRect.top, height: rr.height };
  });

  // Determine active row and col for showing specific handles
  let cell = domAtPos.node as HTMLElement;
  if (cell && cell.nodeType === Node.TEXT_NODE) {
    cell = cell.parentElement as HTMLElement;
  }
  while (cell && cell.tagName !== 'TD' && cell.tagName !== 'TH' && cell !== activeTableEl.value) {
    cell = cell.parentElement as HTMLElement;
  }
  if (cell && (cell.tagName === 'TD' || cell.tagName === 'TH')) {
    const row = cell.parentElement as HTMLTableRowElement;
    if (row && activeTableEl.value) {
      const allRows = Array.from(activeTableEl.value.querySelectorAll('tr'));
      activeRowIdx.value = allRows.indexOf(row);
      const allCells = Array.from(row.querySelectorAll('td, th'));
      activeColIdx.value = allCells.indexOf(cell);
    }
  } else {
    activeRowIdx.value = -1;
    activeColIdx.value = -1;
  }
};

const openContextMenu = (e: MouseEvent) => {
  if (!props.editor.isActive('table')) return;
  e.preventDefault();
  const wrapper = activeTableEl.value?.closest('.tiptap-wrapper');
  const wrapperRect = wrapper ? wrapper.getBoundingClientRect() : { top: 0, left: 0 };
  
  ctxMenuPos.value = { top: e.clientY - wrapperRect.top, left: e.clientX - wrapperRect.left };
  showCtxMenu.value = true;
};

const closeCtxMenu = () => { showCtxMenu.value = false; };

const ctxAction = (action: string) => {
  // For merge: restore saved CellSelection first
  if (action === 'mergeCells' && lastCellSelection) {
    try {
      const tr = props.editor.state.tr.setSelection(lastCellSelection);
      props.editor.view.dispatch(tr);
    } catch (e) { /* positions may be stale */ }
    props.editor.commands.mergeCells();
    lastCellSelection = null;
    lastCanMerge = false;
    closeCtxMenu();
    setTimeout(updateTableControls, 50);
    return;
  }
  
  const chain = props.editor.chain().focus();
  switch (action) {
    case 'addRowAbove': chain.addRowBefore().run(); break;
    case 'addRowBelow': chain.addRowAfter().run(); break;
    case 'deleteRow': chain.deleteRow().run(); break;
    case 'addColLeft': chain.addColumnBefore().run(); break;
    case 'addColRight': chain.addColumnAfter().run(); break;
    case 'deleteCol': chain.deleteColumn().run(); break;
    case 'splitCell': chain.splitCell().run(); break;
    case 'toggleHeaderRow': chain.toggleHeaderRow().run(); break;
    case 'toggleHeaderCol': chain.toggleHeaderColumn().run(); break;
    case 'deleteTable': chain.deleteTable().run(); break;
  }
  closeCtxMenu();
  setTimeout(updateTableControls, 50);
};

const setCellColor = (color: string | null, close = true) => {
  if (lastCellSelection) {
    try {
      const tr = props.editor.state.tr.setSelection(lastCellSelection);
      props.editor.view.dispatch(tr);
    } catch (e) { /* positions may be stale */ }
    lastCellSelection = null;
    lastCanMerge = false;
  }
  
  if (color) {
    props.editor.chain().focus().setCellAttribute('backgroundColor', color).run();
  } else {
    props.editor.chain().focus().setCellAttribute('backgroundColor', null).run();
  }
  if (close) {
    closeCtxMenu();
  }
  setTimeout(updateTableControls, 50);
};

// Focus a specific cell to position cursor there before operations

const addRowAtBottom = () => {
  if (!activeTableEl.value) return;
  // Focus last row, then add after
  const rows = activeTableEl.value.querySelectorAll('tr');
  if (rows.length > 0) {
    const lastRow = rows[rows.length - 1];
    const cell = lastRow.querySelector('td, th');
    if (cell) {
      const pos = props.editor.view.posAtDOM(cell, 0);
      props.editor.chain().setTextSelection(pos).addRowAfter().run();
    }
  }
};

const addColAtRight = () => {
  if (!activeTableEl.value) return;
  const firstRow = activeTableEl.value.querySelector('tr');
  if (firstRow) {
    const cells = firstRow.querySelectorAll('td, th');
    const lastCell = cells[cells.length - 1];
    if (lastCell) {
      const pos = props.editor.view.posAtDOM(lastCell, 0);
      props.editor.chain().setTextSelection(pos).addColumnAfter().run();
    }
  }
};

const getCellPos = (domNode: Element) => {
  const pos = props.editor.view.posAtDOM(domNode, 0);
  const $pos = props.editor.state.doc.resolve(pos);
  for (let d = $pos.depth; d > 0; d--) {
    const name = $pos.node(d).type.name;
    if (name === 'tableCell' || name === 'tableHeader') {
      return $pos.before(d);
    }
  }
  return pos - 1;
};

const selectWholeTable = () => {
  if (!activeTableEl.value) return;
  const rows = activeTableEl.value.querySelectorAll('tr');
  if (rows.length > 0) {
    const firstCell = rows[0].querySelector('td, th');
    const lastRow = rows[rows.length - 1];
    const lastCell = lastRow.children[lastRow.children.length - 1];
    if (firstCell && lastCell) {
      const anchorPos = getCellPos(firstCell);
      const headPos = getCellPos(lastCell);
      props.editor.chain().setCellSelection({ anchorCell: anchorPos, headCell: headPos }).run();
    }
  }
};

const selectColumn = (colIdx: number, e?: MouseEvent) => {
  if (!activeTableEl.value) return;
  const rows = activeTableEl.value.querySelectorAll('tr');
  if (rows.length > 0) {
    const firstCell = rows[0].querySelectorAll('td, th')[colIdx];
    const lastCell = rows[rows.length - 1].querySelectorAll('td, th')[colIdx];
    if (firstCell && lastCell) {
      const anchorPos = getCellPos(firstCell);
      const headPos = getCellPos(lastCell);
      if (e?.shiftKey && lastCellSelection) {
        props.editor.chain().setCellSelection({ anchorCell: lastCellSelection.$anchorCell.pos, headCell: headPos }).run();
      } else {
        props.editor.chain().setCellSelection({ anchorCell: anchorPos, headCell: headPos }).run();
      }
    }
  }
};

const selectRow = (rowIdx: number, e?: MouseEvent) => {
  if (!activeTableEl.value) return;
  const row = activeTableEl.value.querySelectorAll('tr')[rowIdx];
  if (row) {
    const firstCell = row.children[0];
    const lastCell = row.children[row.children.length - 1];
    if (firstCell && lastCell) {
      const anchorPos = getCellPos(firstCell);
      const headPos = getCellPos(lastCell);
      if (e?.shiftKey && lastCellSelection) {
        props.editor.chain().setCellSelection({ anchorCell: lastCellSelection.$anchorCell.pos, headCell: headPos }).run();
      } else {
        props.editor.chain().setCellSelection({ anchorCell: anchorPos, headCell: headPos }).run();
      }
    }
  }
};

// Close context menu on click outside
const onDocMouseDown = (e: MouseEvent) => {
  let target = e.target as HTMLElement | Node | null;
  if (target && target.nodeType === Node.TEXT_NODE) {
    target = target.parentElement;
  }
  const el = target as HTMLElement;
  if (el && el.closest && !el.closest('.tc-ctx-menu, .tc-corner-handle, .tc-col-handle, .tc-row-handle')) {
    closeCtxMenu();
  }
};

// Listen to editor's selectionUpdate event
const onSelectionUpdate = () => {
  trackCellSelection();
  setTimeout(updateTableControls, 10);
};

onMounted(() => {
  props.editor.on('selectionUpdate', onSelectionUpdate);
  document.addEventListener('mousedown', onDocMouseDown, true);
});

onBeforeUnmount(() => {
  props.editor.off('selectionUpdate', onSelectionUpdate);
  document.removeEventListener('mousedown', onDocMouseDown, true);
});

// Expose openContextMenu so parent can call it from handleDOMEvents
defineExpose({ openContextMenu, updateTableControls, trackCellSelection });
</script>

<template>
  <!-- Table Controls: + buttons, row/col handles -->
  <template v-if="isInTable && activeTableEl">
    <!-- Column handles (top of each column) -->
    <button
      v-for="(col, i) in colPositions" :key="'ch-'+i"
      v-show="i === activeColIdx"
      class="tc-col-handle"
      :style="{ position: 'absolute', top: (tableRect.top - 20) + 'px', left: (col.left + col.width / 2 - 10) + 'px' }"
      @mousedown.prevent.stop="(e: MouseEvent) => { selectColumn(i, e); openContextMenu(e); }"
      @click.stop
    >
      <GripVertical class="w-3 h-3 rotate-90" />
    </button>

    <!-- Row handles (left of each row) -->
    <button
      v-for="(row, i) in rowPositions" :key="'rh-'+i"
      v-show="i === activeRowIdx"
      class="tc-row-handle"
      :style="{ position: 'absolute', top: (row.top + row.height / 2 - 10) + 'px', left: (tableRect.left - 22) + 'px' }"
      @mousedown.prevent.stop="(e: MouseEvent) => { selectRow(i, e); openContextMenu(e); }"
      @click.stop
    >
      <GripVertical class="w-3 h-3" />
    </button>

    <!-- Corner handle (select whole table) -->
    <button
      class="tc-corner-handle"
      :style="{ position: 'absolute', top: (tableRect.top - 22) + 'px', left: (tableRect.left - 24) + 'px' }"
      @mousedown.prevent.stop="(e: MouseEvent) => { selectWholeTable(); openContextMenu(e); }"
      @click.stop
    >
      <svg width="10" height="10" viewBox="0 0 10 10"><rect x="0" y="0" width="4" height="4" fill="currentColor" rx="0.5"/><rect x="6" y="0" width="4" height="4" fill="currentColor" rx="0.5"/><rect x="0" y="6" width="4" height="4" fill="currentColor" rx="0.5"/><rect x="6" y="6" width="4" height="4" fill="currentColor" rx="0.5"/></svg>
    </button>

    <!-- Add row button (bottom) -->
    <button
      class="tc-add-btn tc-add-row"
      :style="{ position: 'absolute', top: (tableRect.bottom + 2) + 'px', left: (tableRect.left + tableRect.width / 2 - 14) + 'px' }"
      @mousedown.prevent="addRowAtBottom"
      title="Add row"
    >
      <Plus class="w-3.5 h-3.5" />
    </button>

    <!-- Add column button (right) -->
    <button
      class="tc-add-btn tc-add-col"
      :style="{ position: 'absolute', top: (tableRect.top + tableRect.height / 2 - 14) + 'px', left: (tableRect.right + 2) + 'px' }"
      @mousedown.prevent="addColAtRight"
      title="Add column"
    >
      <Plus class="w-3.5 h-3.5" />
    </button>
  </template>

  <!-- Table Context Menu -->
  <Transition name="bubble">
    <div
      v-if="showCtxMenu && editor"
      class="tc-ctx-menu"
      :style="{ position: 'absolute', top: ctxMenuPos.top + 'px', left: ctxMenuPos.left + 'px' }"
      @mousedown.prevent.stop
    >
      <button @click="ctxAction('addRowAbove')">Add row above</button>
      <button @click="ctxAction('addRowBelow')">Add row below</button>
      <button @click="ctxAction('deleteRow')" class="ctx-danger">Delete row</button>
      <div class="ctx-sep" />
      <button @click="ctxAction('addColLeft')">Add column left</button>
      <button @click="ctxAction('addColRight')">Add column right</button>
      <button @click="ctxAction('deleteCol')" class="ctx-danger">Delete column</button>
      <div class="ctx-sep" />
      <button @click="ctxAction('mergeCells')">Merge cells</button>
      <button @click="ctxAction('splitCell')">Split cell</button>
      <button @click="ctxAction('toggleHeaderRow')">Toggle header row</button>
      <button @click="ctxAction('toggleHeaderCol')">Toggle header column</button>
      <div class="ctx-sep" />
      
      <div class="flex items-center gap-2 px-3 py-1.5 border-b border-gray-100 dark:border-[#333]">
        <span class="text-xs text-gray-500 font-medium w-10 shrink-0">Color:</span>
        <div class="flex items-center gap-2 flex-1">
          <div class="w-5 h-5 shrink-0 rounded-full border border-gray-200 dark:border-gray-700 bg-transparent flex items-center justify-center text-[10px] text-gray-400 hover:text-gray-700 dark:hover:text-gray-200 hover:bg-gray-100 dark:hover:bg-[#444] transition-colors cursor-pointer" @click="setCellColor(null)" title="Clear color">✕</div>
          <div class="w-5 h-5 shrink-0 rounded-full border border-gray-200 dark:border-gray-700 hover:scale-110 transition-transform cursor-pointer" style="background-color: #fee2e2;" @click="setCellColor('rgba(239, 68, 68, 0.15)')"></div>
          <div class="w-5 h-5 shrink-0 rounded-full border border-gray-200 dark:border-gray-700 hover:scale-110 transition-transform cursor-pointer" style="background-color: #dbeafe;" @click="setCellColor('rgba(59, 130, 246, 0.15)')"></div>
          <div class="w-5 h-5 shrink-0 rounded-full border border-gray-200 dark:border-gray-700 hover:scale-110 transition-transform cursor-pointer" style="background-color: #d1fae5;" @click="setCellColor('rgba(16, 185, 129, 0.15)')"></div>
          <div class="w-5 h-5 shrink-0 rounded-full border border-gray-200 dark:border-gray-700 hover:scale-110 transition-transform cursor-pointer" style="background-color: #fef3c7;" @click="setCellColor('rgba(245, 158, 11, 0.15)')"></div>
          <div class="w-5 h-5 shrink-0 rounded-full border border-gray-200 dark:border-gray-700 hover:scale-110 transition-transform cursor-pointer" style="background-color: #f3e8ff;" @click="setCellColor('rgba(168, 85, 247, 0.15)')"></div>
          <label class="w-5 h-5 shrink-0 rounded-full border border-gray-200 dark:border-gray-700 flex items-center justify-center cursor-pointer hover:bg-gray-100 dark:hover:bg-[#444] relative hover:scale-110 transition-transform" title="Custom color">
            <Palette class="w-3 h-3 text-gray-500 dark:text-gray-400" />
            <input 
              type="color" 
              @input="(e) => setCellColor((e.target as HTMLInputElement).value, false)" 
              class="absolute opacity-0 inset-0 w-full h-full cursor-pointer"
            />
          </label>
        </div>
      </div>

      <button @click="ctxAction('deleteTable')" class="ctx-danger">Delete table</button>
    </div>
  </Transition>
</template>
