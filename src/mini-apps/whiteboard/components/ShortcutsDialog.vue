<script setup lang="ts">
import { nextTick, onMounted, ref } from 'vue';
import { useI18n } from 'vue-i18n';
import { useModalFocus } from '../composables/useModalFocus';

/**
 * Every key the board answers to, in one place. Most of them were only in
 * tooltips and menus, and a few — Tab on a mind-map item, F6, Shift+F10 —
 * were nowhere a person could find them.
 */
const emit = defineEmits<{ (e: 'close'): void }>();
const { t } = useI18n();

const isMac = /Mac|iPhone|iPad/.test(navigator.platform);
const k = (mac: string, other: string) => (isMac ? mac : other);

const groups: { title: string; rows: [string, string][] }[] = [
  {
    title: 'tools',
    rows: [
      ['V', 'tool_select'], ['H', 'tool_pan'], ['D', 'tool_draw'], ['E', 'tool_eraser'], ['S', 'tool_shape'],
      ['T', 'tool_text'], ['N', 'tool_sticky'], ['M', 'tool_mindmap'], ['F', 'tool_frame'],
      ['Enter', 'place'],
    ],
  },
  {
    title: 'edit',
    rows: [
      [k('⌘Z', 'Ctrl+Z'), 'undo'], [k('⇧⌘Z', 'Ctrl+Y'), 'redo'],
      [k('⌘C / ⌘X / ⌘V', 'Ctrl+C / Ctrl+X / Ctrl+V'), 'clipboard'],
      [k('⌘D', 'Ctrl+D'), 'duplicate'], [k('⌘A', 'Ctrl+A'), 'select_all'],
      [k('⌫', 'Delete'), 'delete'], ['F2', 'edit_words'],
      ['← ↑ → ↓', 'nudge'], [k('⌘G / ⇧⌘G', 'Ctrl+G / Ctrl+Shift+G'), 'group'],
      [k('⇧⌘] / ⇧⌘[', 'Ctrl+Shift+] / Ctrl+Shift+['), 'order'], [k('⇧⌘L', 'Ctrl+Shift+L'), 'lock'],
    ],
  },
  {
    title: 'mindmap',
    rows: [['Tab', 'mind_child'], ['Enter', 'mind_sibling']],
  },
  {
    title: 'move',
    rows: [
      [k('⇧F10', 'Shift+F10'), 'menu'], [k('⌘F', 'Ctrl+F'), 'find'], ['F6', 'panel'], ['?', 'this_list'], ['Esc', 'escape'],
    ],
  },
  {
    title: 'present',
    rows: [['→ / Space', 'present_next'], ['←', 'present_prev'], ['L', 'present_laser'], ['Esc', 'present_exit']],
  },
];

const box = ref<HTMLElement | null>(null);
const closeBtn = ref<HTMLButtonElement | null>(null);
const keepFocus = useModalFocus(box, () => emit('close'));
onMounted(() => nextTick(() => closeBtn.value?.focus()));
</script>

<template>
  <div class="wb-keys-backdrop" @click.self="emit('close')" @keydown.escape.stop="emit('close')" @keydown="keepFocus">
    <div ref="box" class="wb-keys" role="dialog" aria-modal="true" aria-labelledby="wb-keys-title">
      <div class="wb-keys-head">
        <h2 id="wb-keys-title" class="wb-keys-title">{{ t('whiteboard.keys.title') }}</h2>
        <button ref="closeBtn" type="button" class="wb-keys-btn" @click="emit('close')">{{ t('whiteboard.close') }}</button>
      </div>
      <div class="wb-keys-body">
        <section v-for="g in groups" :key="g.title" class="wb-keys-group">
          <h3 class="wb-keys-group-title">{{ t(`whiteboard.keys.group.${g.title}`) }}</h3>
          <dl class="wb-keys-list">
            <template v-for="[key, what] in g.rows" :key="what">
              <dt><kbd>{{ key }}</kbd></dt>
              <dd>{{ t(`whiteboard.keys.do.${what}`) }}</dd>
            </template>
          </dl>
        </section>
      </div>
      <p class="wb-keys-note">{{ t('whiteboard.keys.note') }}</p>
    </div>
  </div>
</template>

<style scoped>
.wb-keys-backdrop { position: fixed; inset: 0; z-index: 200; display: grid; place-items: center; background: rgb(0 0 0 / 0.3); padding: 16px; }
.wb-keys { width: min(720px, 100%); max-height: min(640px, calc(100vh - 32px)); display: flex; flex-direction: column; gap: 10px; padding: 16px; border-radius: 14px; background: var(--color-surface, #fff); color: var(--color-text, #18181b); box-shadow: 0 16px 48px rgb(0 0 0 / 0.2); }
:global(.dark .wb-keys) { background: var(--color-surface-dark, #1e1e1e); color: var(--color-text-dark, #fafafa); }
.wb-keys-head { display: flex; align-items: center; justify-content: space-between; gap: 8px; }
.wb-keys-title { font-size: 15px; font-weight: 700; }
.wb-keys-body { overflow: auto; display: grid; grid-template-columns: repeat(auto-fill, minmax(280px, 1fr)); gap: 16px; }
.wb-keys-group-title { font-size: 12px; font-weight: 700; text-transform: uppercase; letter-spacing: 0.04em; margin-bottom: 6px; color: var(--color-text-secondary, #52525b); }
:global(.dark .wb-keys-group-title) { color: var(--color-text-secondary-dark, #a1a1aa); }
.wb-keys-list { display: grid; grid-template-columns: auto 1fr; gap: 6px 12px; align-items: baseline; font-size: 13px; }
.wb-keys-list dt { white-space: nowrap; }
kbd { display: inline-block; padding: 1px 6px; border-radius: 5px; font: 12px/1.6 ui-monospace, SFMono-Regular, Menlo, monospace; border: 1px solid var(--color-border, #e6e6e6); background: rgb(0 0 0 / 0.03); }
:global(.dark .wb-keys kbd) { border-color: var(--color-border-dark, #333); background: rgb(255 255 255 / 0.05); }
.wb-keys-note { font-size: 12px; color: var(--color-text-secondary, #52525b); }
:global(.dark .wb-keys-note) { color: var(--color-text-secondary-dark, #a1a1aa); }
.wb-keys-btn { padding: 6px 14px; border-radius: 8px; font-size: 13px; font-weight: 600; border: 1px solid var(--color-border, #e6e6e6); }
:global(.dark .wb-keys-btn) { border-color: var(--color-border-dark, #333); }
</style>
