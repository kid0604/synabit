<script setup lang="ts">
import { computed, onMounted, ref } from 'vue';
import { useI18n } from 'vue-i18n';
import { invoke } from '@tauri-apps/api/core';
import { X } from 'lucide-vue-next';
import { boardPreview } from '../../../shared/boardPreview';
import { readBoardFile, type WhiteboardData } from '../boardFile';
import { useModalFocus } from '../composables/useModalFocus';

/**
 * The versions of a board kept on this device (see `keep_version` in
 * whiteboards.rs), each shown as a picture, and any one of them put back.
 * Putting one back is an ordinary change: undoable, saved, synced.
 */
const props = defineProps<{ vaultPath: string; path: string }>();
const emit = defineEmits<{ (e: 'restore', data: WhiteboardData): void; (e: 'close'): void }>();
const { t, locale } = useI18n();

interface Version { at: number; size: number }
const versions = ref<Version[]>([]);
const loading = ref(true);
const chosen = ref<number | null>(null);
const board = ref<WhiteboardData | null>(null);
const failed = ref(false);
/** The chosen version could not be read: said, not left loading. */
const readFailed = ref(false);
const box = ref<HTMLElement | null>(null);
const keepFocus = useModalFocus(box, () => emit('close'));

onMounted(async () => {
  try {
    versions.value = await invoke<Version[]>('list_board_versions', { vaultPath: props.vaultPath, path: props.path });
    if (versions.value.length) await choose(versions.value[0].at);
  } catch {
    failed.value = true;
  } finally {
    loading.value = false;
  }
});

async function choose(at: number) {
  chosen.value = at;
  board.value = null;
  readFailed.value = false;
  try {
    const raw = await invoke<string>('read_board_version', { vaultPath: props.vaultPath, path: props.path, at });
    // Another version chosen while this one was read: that one is shown.
    if (chosen.value !== at) return;
    const read = readBoardFile(raw);
    board.value = read.ok ? read.data : null;
    readFailed.value = !read.ok;
  } catch {
    if (chosen.value !== at) return;
    board.value = null;
    readFailed.value = true;
  }
}

const when = (at: number) =>
  new Intl.DateTimeFormat(String(locale.value), { dateStyle: 'medium', timeStyle: 'short' }).format(new Date(at));
const picture = computed(() => (board.value ? boardPreview(board.value) : ''));
</script>

<template>
  <div class="wb-hist-backdrop" @click.self="emit('close')" @keydown.escape.stop="emit('close')" @keydown="keepFocus">
    <div ref="box" class="wb-hist" role="dialog" aria-modal="true" :aria-label="t('whiteboard.history.title')">
      <div class="wb-hist-head">
        <h2 class="wb-hist-title">{{ t('whiteboard.history.title') }}</h2>
        <button class="wb-hist-icon" :aria-label="t('whiteboard.close')" @click="emit('close')"><X :size="16" /></button>
      </div>
      <p class="wb-hist-hint">{{ t('whiteboard.history.hint') }}</p>
      <p v-if="loading" class="wb-hist-empty">…</p>
      <p v-else-if="failed" class="wb-hist-empty" role="alert">{{ t('whiteboard.history.list_failed') }}</p>
      <p v-else-if="!versions.length" class="wb-hist-empty">{{ t('whiteboard.history.none') }}</p>
      <div v-else class="wb-hist-body">
        <ul class="wb-hist-list" role="listbox" :aria-label="t('whiteboard.history.title')">
          <li v-for="v in versions" :key="v.at" role="presentation">
            <button
              role="option"
              :aria-selected="chosen === v.at"
              class="wb-hist-item"
              :class="{ active: chosen === v.at }"
              @click="choose(v.at)"
            >{{ when(v.at) }}</button>
          </li>
        </ul>
        <div class="wb-hist-view">
          <!-- eslint-disable-next-line vue/no-v-html -->
          <div v-if="picture" class="wb-hist-picture" v-html="picture" />
          <p v-else-if="readFailed" class="wb-hist-empty" role="alert">{{ t('whiteboard.history.read_failed') }}</p>
          <p v-else class="wb-hist-empty">…</p>
          <div class="wb-hist-actions">
            <span v-if="board" class="wb-hist-meta">{{ t('whiteboard.history.items', { count: board.nodes.length }) }}</span>
            <button class="wb-hist-restore" :disabled="!board" @click="board && emit('restore', board)">{{ t('whiteboard.history.restore') }}</button>
          </div>
        </div>
      </div>
    </div>
  </div>
</template>

<style scoped>
.wb-hist-backdrop {
  position: fixed;
  inset: 0;
  z-index: 200;
  display: grid;
  place-items: center;
  background: rgb(0 0 0 / 0.35);
  padding: 16px;
}
.wb-hist {
  width: min(820px, 100%);
  max-height: calc(100% - 32px);
  display: flex;
  flex-direction: column;
  gap: 8px;
  padding: 16px;
  border-radius: 14px;
  background: var(--color-surface, #fff);
  color: var(--color-text, #18181b);
  box-shadow: 0 16px 48px rgb(0 0 0 / 0.2);
}
:global(.dark .wb-hist) {
  background: var(--color-surface-dark, #1e1e1e);
  color: var(--color-text-dark, #fafafa);
}
.wb-hist-head { display: flex; align-items: center; justify-content: space-between; }
.wb-hist-title { font-size: 15px; font-weight: 700; }
.wb-hist-hint, .wb-hist-meta, .wb-hist-empty { font-size: 13px; color: var(--color-text-secondary, #52525b); }
:global(.dark .wb-hist-hint), :global(.dark .wb-hist-meta), :global(.dark .wb-hist-empty) { color: var(--color-text-secondary-dark, #a1a1aa); }
.wb-hist-icon { display: grid; place-items: center; width: 30px; height: 30px; border-radius: 8px; }
.wb-hist-body { display: grid; grid-template-columns: 220px 1fr; gap: 12px; min-height: 0; overflow: hidden; }
@media (max-width: 640px) { .wb-hist-body { grid-template-columns: 1fr; } }
.wb-hist-list { overflow-y: auto; max-height: 420px; display: flex; flex-direction: column; gap: 2px; }
.wb-hist-item { width: 100%; text-align: left; padding: 8px 10px; border-radius: 8px; font-size: 13px; }
.wb-hist-item:hover, .wb-hist-item:focus-visible { background: var(--color-surface-hover, #f4f4f5); outline: none; }
:global(.dark .wb-hist-item:hover), :global(.dark .wb-hist-item:focus-visible) { background: var(--color-surface-hover-dark, #2a2a2a); }
.wb-hist-item.active { background: color-mix(in oklab, var(--color-accent) 15%, transparent); font-weight: 600; }
.wb-hist-view { display: flex; flex-direction: column; gap: 8px; min-width: 0; }
.wb-hist-picture { border: 1px solid var(--color-border, #e6e6e6); border-radius: 10px; padding: 8px; overflow: hidden; }
.wb-hist-picture :deep(svg) { width: 100%; height: auto; max-height: 360px; }
.wb-hist-actions { display: flex; align-items: center; justify-content: space-between; gap: 8px; }
.wb-hist-restore { padding: 6px 14px; border-radius: 8px; font-size: 13px; font-weight: 600; color: #fff; background: var(--color-accent); }
.wb-hist-restore:disabled { opacity: 0.4; }
</style>
