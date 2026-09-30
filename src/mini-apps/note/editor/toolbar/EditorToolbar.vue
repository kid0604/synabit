<script setup lang="ts">
/**
 * The always-visible formatting row above a note.
 *
 * Formatting used to be reachable three ways, all of them invisible until you
 * knew them: typing `/`, selecting text for the bubble menu, or writing
 * Markdown. This row is the fourth, and the only one that is there before
 * anybody has learnt anything. It is deliberately quiet — muted icons, the
 * accent only on what is on at the caret — so it reads as furniture rather
 * than as a second thing to look at.
 *
 * One tab stop for the whole row, arrow keys between buttons (the ARIA
 * toolbar pattern), so Tab from the title still reaches the text in two
 * presses rather than eleven.
 *
 * On a phone it is pinned to the bottom of the note, above the keyboard when
 * the keyboard is up; see `placement` and `keyboardInset`.
 */
import { computed, nextTick, onBeforeUnmount, onMounted, ref } from 'vue';
import { useI18n } from 'vue-i18n';
import type { Editor } from '@tiptap/vue-3';
import {
  Heading as HeadingIcon,
  ChevronDown,
  Bold as BoldIcon,
  Italic as ItalicIcon,
  List as ListIcon,
  ListOrdered,
  ListChecks,
  Quote as QuoteIcon,
  Link as LinkIcon,
  Plus,
} from 'lucide-vue-next';
import { usePlatform } from '../../../../composables/usePlatform';
import ToolbarMenu, { type ToolbarMenuItem } from './ToolbarMenu.vue';
import {
  TOOLBAR_ACTIONS,
  BLOCK_LEVELS,
  currentBlockLevel,
  setBlockLevel,
  labelWithShortcut,
  nextToolbarIndex,
  type ToolbarAction,
  type BlockLevel,
} from './toolbarCommands';
import type { SlashCommandItem } from '../config/slashCommandItems';

const props = defineProps<{
  editor: Editor;
  /** What "+ Insert" lists — the slash menu's items, already filtered. */
  insertItems: SlashCommandItem[];
  /** Above the text on a desktop, pinned to the bottom on a phone. */
  placement: 'top' | 'bottom';
}>();

const emit = defineEmits<{
  /** Open the editor's own link dialog. */
  (e: 'link'): void;
}>();

const { t } = useI18n();
const { isMac } = usePlatform();

const ICONS: Record<string, any> = {
  bold: BoldIcon,
  italic: ItalicIcon,
  bulletList: ListIcon,
  orderedList: ListOrdered,
  taskList: ListChecks,
  blockquote: QuoteIcon,
  link: LinkIcon,
};

/** Buttons are grouped: marks, then blocks, then the link. */
const GROUP_BREAK_BEFORE = new Set(['bulletList', 'link']);

const actionLabel = (a: ToolbarAction) => labelWithShortcut(t(a.labelKey), a.shortcut, isMac.value);

const runAction = (a: ToolbarAction) => {
  if (a.id === 'link') emit('link');
  else a.run?.(props.editor);
};

// ── Text style menu ────────────────────────────────────────
const level = computed(() => currentBlockLevel(props.editor));

const levelItems = computed<ToolbarMenuItem[]>(() =>
  BLOCK_LEVELS.map((b) => ({
    key: String(b.level),
    label: labelWithShortcut(t(b.labelKey), b.shortcut, isMac.value),
    checked: level.value === b.level,
  })),
);

const styleLabel = computed(() => {
  const current = BLOCK_LEVELS.find((b) => b.level === level.value);
  return current
    ? t('note.toolbar.text_style_current', { style: t(current.labelKey) })
    : t('note.toolbar.text_style');
});

// ── Insert menu ────────────────────────────────────────────
const insertMenuItems = computed<ToolbarMenuItem[]>(() =>
  props.insertItems.map((item) => ({
    key: item.title,
    label: t(item.titleKey),
    description: t(item.descriptionKey),
    icon: item.icon,
  })),
);

/**
 * Run a slash item at the caret. The slash menu hands its items the range of
 * the `/query` it typed so they can delete it; from here there is nothing to
 * delete, so the range is empty and sits where the caret is.
 */
const insert = (key: string) => {
  const item = props.insertItems.find((i) => i.title === key);
  closeMenu(false);
  if (!item) return;
  const at = props.editor.state.selection.from;
  item.command({ editor: props.editor, range: { from: at, to: at } });
};

// ── Menus ──────────────────────────────────────────────────
const openMenu = ref<null | 'style' | 'insert'>(null);
const menuLeft = ref(0);
const root = ref<HTMLElement | null>(null);
const styleButton = ref<HTMLButtonElement | null>(null);
const insertButton = ref<HTMLButtonElement | null>(null);

const toggleMenu = (which: 'style' | 'insert') => {
  if (openMenu.value === which) { closeMenu(true); return; }
  const button = which === 'style' ? styleButton.value : insertButton.value;
  // The row scrolls sideways on a phone, so the menu is placed against the
  // button's position on screen rather than its offset inside the row — and
  // it lives outside the row, where the row's overflow cannot clip it.
  if (button && root.value) {
    const b = button.getBoundingClientRect();
    const r = root.value.getBoundingClientRect();
    menuLeft.value = Math.max(0, Math.min(b.left - r.left, r.width - 260));
  }
  openMenu.value = which;
};

const closeMenu = (refocus: boolean) => {
  const which = openMenu.value;
  openMenu.value = null;
  if (!refocus) return;
  void nextTick(() => (which === 'style' ? styleButton.value : insertButton.value)?.focus());
};

const pickLevel = (key: string) => {
  closeMenu(false);
  setBlockLevel(props.editor, Number(key) as BlockLevel);
};

// ── Roving focus ───────────────────────────────────────────
const row = ref<HTMLElement | null>(null);
const focusIndex = ref(0);

const buttons = () => Array.from(row.value?.querySelectorAll<HTMLElement>('[data-toolbar-item]') ?? []);

const onRowFocusIn = (e: FocusEvent) => {
  const i = buttons().indexOf(e.target as HTMLElement);
  if (i >= 0) focusIndex.value = i;
};

const onRowKeydown = (e: KeyboardEvent) => {
  const list = buttons();
  const current = list.indexOf(document.activeElement as HTMLElement);
  if (current < 0) return;

  // The menu-button pattern: arrow down on a button with a popup opens it.
  if (e.key === 'ArrowDown' || (e.key === 'ArrowUp' && props.placement === 'bottom')) {
    const target = list[current];
    if (target === styleButton.value || target === insertButton.value) {
      e.preventDefault();
      toggleMenu(target === styleButton.value ? 'style' : 'insert');
    }
    return;
  }

  const next = nextToolbarIndex(current, e.key, list.length);
  if (next === null) return;
  e.preventDefault();
  focusIndex.value = next;
  list[next]?.focus();
};

/** Tab index for the button at `i` — only one of them is a tab stop. */
const tabIndexFor = (i: number) => (i === focusIndex.value ? 0 : -1);
/** Index of each action button in the row (the style button is 0). */
const actionIndex = (i: number) => i + 1;
const insertIndex = computed(() => TOOLBAR_ACTIONS.length + 1);

// ── Clicks outside ─────────────────────────────────────────
const onDocumentPointerDown = (e: PointerEvent) => {
  if (openMenu.value && root.value && !root.value.contains(e.target as Node)) closeMenu(false);
};

// ── Keyboard on a phone ────────────────────────────────────
/**
 * How far the on-screen keyboard reaches above the bottom of the note's
 * scroll area.
 *
 * Android shrinks the WebView when the keyboard opens, so the bottom of the
 * note is already above it and this stays 0. iOS does not: the keyboard is
 * drawn over the page, and only `visualViewport` knows how much of it is
 * hidden. The part below the scroll area (the tab bar) is subtracted, because
 * the toolbar is pinned to the scroll area, not to the window.
 */
const keyboardInset = ref(0);

const measureKeyboard = () => {
  const vv = window.visualViewport;
  if (!vv || !root.value) { keyboardInset.value = 0; return; }
  const covered = window.innerHeight - (vv.height + vv.offsetTop);
  const scroller = root.value.closest('.overflow-y-auto');
  const below = scroller ? window.innerHeight - scroller.getBoundingClientRect().bottom : 0;
  keyboardInset.value = Math.max(0, Math.round(covered - below));
};

onMounted(() => {
  document.addEventListener('pointerdown', onDocumentPointerDown, true);
  if (props.placement === 'bottom' && window.visualViewport) {
    window.visualViewport.addEventListener('resize', measureKeyboard);
    window.visualViewport.addEventListener('scroll', measureKeyboard);
    measureKeyboard();
  }
});

onBeforeUnmount(() => {
  document.removeEventListener('pointerdown', onDocumentPointerDown, true);
  window.visualViewport?.removeEventListener('resize', measureKeyboard);
  window.visualViewport?.removeEventListener('scroll', measureKeyboard);
});

const rootStyle = computed(() =>
  props.placement === 'bottom' ? { bottom: `${keyboardInset.value}px` } : undefined,
);
</script>

<template>
  <div
    ref="root"
    class="note-toolbar bg-base dark:bg-base-dark border-border dark:border-border-dark"
    :class="placement === 'bottom' ? 'note-toolbar--bottom border-t' : 'note-toolbar--top border-b'"
    :style="rootStyle"
  >
    <!--
      `mousedown.prevent` keeps the caret in the text: a click on a button
      would otherwise move focus to it, and the command would then run against
      a selection the editor no longer shows.
    -->
    <div
      ref="row"
      role="toolbar"
      aria-orientation="horizontal"
      :aria-label="t('note.toolbar.label')"
      class="note-toolbar-row"
      @keydown="onRowKeydown"
      @focusin="onRowFocusIn"
      @mousedown.prevent
    >
      <button
        ref="styleButton"
        type="button"
        data-toolbar-item
        class="btn-icon note-toolbar-btn !w-auto px-2 gap-0.5"
        :tabindex="tabIndexFor(0)"
        aria-haspopup="menu"
        :aria-expanded="openMenu === 'style'"
        :aria-label="styleLabel"
        :title="styleLabel"
        @click="toggleMenu('style')"
      >
        <HeadingIcon class="w-4 h-4" />
        <ChevronDown class="w-3 h-3" />
      </button>

      <template v-for="(action, i) in TOOLBAR_ACTIONS" :key="action.id">
        <span v-if="GROUP_BREAK_BEFORE.has(action.id)" class="note-toolbar-sep" aria-hidden="true" />
        <button
          type="button"
          data-toolbar-item
          class="btn-icon note-toolbar-btn"
          :class="{ 'is-active text-accent dark:text-accent-dark bg-accent/10': editor.isActive(action.activeWhen) }"
          :tabindex="tabIndexFor(actionIndex(i))"
          :aria-pressed="editor.isActive(action.activeWhen)"
          :aria-label="actionLabel(action)"
          :title="actionLabel(action)"
          @click="runAction(action)"
        >
          <component :is="ICONS[action.id]" class="w-4 h-4" />
        </button>
      </template>

      <span class="note-toolbar-sep" aria-hidden="true" />
      <button
        ref="insertButton"
        type="button"
        data-toolbar-item
        class="btn-icon note-toolbar-btn !w-auto px-2 gap-1"
        :tabindex="tabIndexFor(insertIndex)"
        aria-haspopup="menu"
        :aria-expanded="openMenu === 'insert'"
        :aria-label="t('note.toolbar.insert_hint')"
        :title="t('note.toolbar.insert_hint')"
        @click="toggleMenu('insert')"
      >
        <Plus class="w-4 h-4" />
        <span class="text-[13px]">{{ t('note.editor.insert') }}</span>
      </button>
    </div>

    <ToolbarMenu
      v-if="openMenu === 'style'"
      class="note-toolbar-popup"
      :style="{ left: menuLeft + 'px' }"
      :items="levelItems"
      :label="t('note.toolbar.text_style')"
      radio
      @select="pickLevel"
      @close="closeMenu"
    />
    <ToolbarMenu
      v-else-if="openMenu === 'insert'"
      class="note-toolbar-popup"
      :style="{ left: menuLeft + 'px' }"
      :items="insertMenuItems"
      :label="t('note.editor.insert')"
      @select="insert"
      @close="closeMenu"
    />
  </div>
</template>
