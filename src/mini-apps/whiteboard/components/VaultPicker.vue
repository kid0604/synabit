<script setup lang="ts">
import { computed, nextTick, onMounted, ref, watch } from 'vue';
import { useModalFocus } from '../composables/useModalFocus';
import { useI18n } from 'vue-i18n';
import { X } from 'lucide-vue-next';
import { useNodeService } from '../../../composables/useNodeService';
import { logger } from '../../../utils/logger';
import { PICKABLE_KINDS } from '../vaultCards';

/**
 * Find something in the vault and put it on the board as a card.
 *
 * Searches by title across the kinds a card can show. Typing filters; arrows
 * and Enter choose; several can be chosen one after another while it is open.
 */
const emit = defineEmits<{ (e: 'pick', item: { id: string; kind: string; title: string }): void; (e: 'close'): void }>();
const boxRef = ref<HTMLElement | null>(null);
const keepFocus = useModalFocus(boxRef, () => emit('close'));
const { t } = useI18n();
const nodes = useNodeService();

interface Found { id: string; kind: string; title: string; detail: string }
const all = ref<Found[]>([]);
const loading = ref(true);
const query = ref('');
const kind = ref<string>('all');
const active = ref(0);
const inputRef = ref<HTMLInputElement | null>(null);

/** Lower case, without Vietnamese marks, so "viec" finds "việc". */
const fold = (s: string) => s.normalize('NFD').replace(/[̀-ͯ]/g, '').replace(/đ/g, 'd').toLowerCase();

onMounted(async () => {
  nextTick(() => inputRef.value?.focus());
  try {
    const lists = await Promise.all(PICKABLE_KINDS.map((k) => nodes.getNodeSummaries(k).catch(() => [])));
    all.value = lists.flat().map((n: any) => ({
      id: n.id,
      kind: n.node_type,
      title: n.title || n.id,
      detail: n.node_type === 'task' ? (n.properties?.status ?? '') : n.node_type === 'event' ? String(n.properties?.start_at ?? '').slice(0, 10) : '',
    }));
  } catch (err) {
    logger.error('Could not list the vault for the board', err as string);
  } finally {
    loading.value = false;
  }
});

const shown = computed(() => {
  const q = fold(query.value.trim());
  return all.value
    .filter((n) => kind.value === 'all' || n.kind === kind.value)
    .filter((n) => !q || fold(n.title).includes(q))
    .slice(0, 60);
});

// The row moved to with the arrows is the row in view.
watch(active, (i) => nextTick(() => document.getElementById(`wb-vp-opt-${i}`)?.scrollIntoView({ block: 'nearest' })));

function pick(item: Found) {
  emit('pick', { id: item.id, kind: item.kind, title: item.title });
}

function onKey(e: KeyboardEvent) {
  if (e.key === 'ArrowDown') active.value = Math.min(active.value + 1, shown.value.length - 1);
  else if (e.key === 'ArrowUp') active.value = Math.max(active.value - 1, 0);
  else if (e.key === 'Home') active.value = 0;
  else if (e.key === 'End') active.value = Math.max(0, shown.value.length - 1);
  else if (e.key === 'Enter' && shown.value[active.value]) pick(shown.value[active.value]);
  else if (e.key === 'Escape') emit('close');
  else return;
  e.preventDefault();
  e.stopPropagation();
}
</script>

<template>
  <div class="wb-vp-backdrop" @click.self="emit('close')" @keydown.escape.stop="emit('close')" @keydown="keepFocus">
    <div ref="boxRef" class="wb-vp" role="dialog" aria-modal="true" :aria-label="t('whiteboard.card.add_title')">
      <div class="wb-vp-head">
        <input
          ref="inputRef"
          v-model="query"
          class="wb-vp-input"
          :placeholder="t('whiteboard.card.search')"
          :aria-label="t('whiteboard.card.search')"
          role="combobox"
          aria-expanded="true"
          aria-controls="wb-vp-list"
          :aria-activedescendant="shown[active] ? `wb-vp-opt-${active}` : undefined"
          @input="active = 0"
          @keydown="onKey"
        />
        <button class="wb-vp-close" :aria-label="t('whiteboard.close')" @click="emit('close')"><X :size="16" /></button>
      </div>
      <div class="wb-vp-kinds" role="group">
        <button
          v-for="k in ['all', ...PICKABLE_KINDS]"
          :key="k"
          class="wb-vp-kind"
          :aria-pressed="kind === k"
          @click="kind = k; active = 0"
        >{{ t(`whiteboard.card.kind.${k}`) }}</button>
      </div>
      <ul id="wb-vp-list" class="wb-vp-list" role="listbox">
        <li v-if="loading" class="wb-vp-empty">…</li>
        <li v-else-if="!shown.length" class="wb-vp-empty">{{ t('whiteboard.card.nothing') }}</li>
        <li
          v-for="(n, i) in shown"
          :id="`wb-vp-opt-${i}`"
          :key="n.id"
          role="option"
          :aria-selected="i === active"
          class="wb-vp-item"
          :class="{ active: i === active }"
          @mouseenter="active = i"
          @click="pick(n)"
        >
          <span class="wb-vp-item-kind">{{ t(`whiteboard.card.kind.${n.kind}`) }}</span>
          <span class="wb-vp-item-title">{{ n.title }}</span>
          <span v-if="n.detail" class="wb-vp-item-detail">{{ n.detail }}</span>
        </li>
      </ul>
    </div>
  </div>
</template>

<style scoped>
.wb-vp-backdrop {
  position: fixed;
  inset: 0;
  z-index: 300;
  display: flex;
  align-items: flex-start;
  justify-content: center;
  padding: 12vh 16px 16px;
  background: rgba(0, 0, 0, 0.3);
}
.wb-vp {
  width: min(560px, 100%);
  max-height: 70vh;
  display: flex;
  flex-direction: column;
  border-radius: 14px;
  background: var(--color-surface, #fff);
  border: 1px solid var(--color-border, #e6e6e6);
  box-shadow: 0 20px 60px rgba(0, 0, 0, 0.25);
  overflow: hidden;
}
.dark .wb-vp {
  background: var(--color-surface-dark, #1e1e1e);
  border-color: var(--color-border-dark, #333);
}
.wb-vp-head {
  display: flex;
  align-items: center;
  gap: 8px;
  padding: 10px 12px;
  border-bottom: 1px solid var(--color-border, #e6e6e6);
}
.dark .wb-vp-head {
  border-color: var(--color-border-dark, #333);
}
.wb-vp-input {
  flex: 1;
  font-size: 15px;
  background: transparent;
  border: none;
  outline: none;
  color: inherit;
}
.wb-vp-close {
  padding: 4px;
  border-radius: 6px;
}
.wb-vp-kinds {
  display: flex;
  gap: 4px;
  padding: 8px 12px;
  flex-wrap: wrap;
}
.wb-vp-kind {
  padding: 3px 10px;
  border-radius: 999px;
  font-size: 12px;
  border: 1px solid var(--color-border, #e6e6e6);
}
.dark .wb-vp-kind {
  border-color: var(--color-border-dark, #333);
}
.wb-vp-kind[aria-pressed='true'] {
  border-color: var(--color-accent);
  color: var(--color-accent);
}
.dark .wb-vp-kind[aria-pressed='true'] {
  border-color: var(--color-accent-dark);
  color: var(--color-accent-dark);
}
.wb-vp-list {
  overflow-y: auto;
  padding: 4px;
}
.wb-vp-empty {
  padding: 16px;
  font-size: 13px;
  text-align: center;
  color: var(--color-text-secondary, #52525b);
}
.wb-vp-item {
  display: flex;
  align-items: baseline;
  gap: 10px;
  padding: 7px 10px;
  border-radius: 8px;
  cursor: pointer;
  font-size: 14px;
}
.wb-vp-item.active {
  background: var(--color-surface-hover, #f4f4f5);
}
.dark .wb-vp-item.active {
  background: var(--color-surface-hover-dark, #2a2a2a);
}
.wb-vp-item-kind {
  flex-shrink: 0;
  min-width: 64px;
  font-size: 12px;
  color: var(--color-text-secondary, #52525b);
}
.wb-vp-item-title {
  flex: 1;
  min-width: 0;
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}
.wb-vp-item-detail {
  flex-shrink: 0;
  font-size: 12px;
  color: var(--color-text-secondary, #52525b);
}
/* The secondary grey is for light paper; on dark, its own. */
.dark .wb-vp-empty {
  color: var(--color-text-secondary-dark, #a1a1aa);
}
.dark .wb-vp-item-kind {
  color: var(--color-text-secondary-dark, #a1a1aa);
}
.dark .wb-vp-item-detail {
  color: var(--color-text-secondary-dark, #a1a1aa);
}
</style>
