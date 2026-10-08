<script setup lang="ts">
import { computed, nextTick, onMounted, ref } from 'vue';
import { useModalFocus } from '../composables/useModalFocus';
import { useI18n } from 'vue-i18n';
import { today, weekStart } from '../vaultCards';

/**
 * Make a live frame: a name and a question to the vault, in the same words
 * the search bar and the Tasks filter take. A few common questions are one
 * click away, for whoever has not learned the words yet.
 */
const emit = defineEmits<{
  (e: 'create', value: { query: string; title: string; layout: 'grid' | 'kanban' | 'timeline'; field?: string }): void;
  (e: 'close'): void;
}>();
const boxRef = ref<HTMLElement | null>(null);
const keepFocus = useModalFocus(boxRef, () => emit('close'));
const { t } = useI18n();

const title = ref('');
const query = ref('');
/** How the answers are laid out, and the field a kanban or a timeline goes by. */
const layout = ref<'grid' | 'kanban' | 'timeline'>('grid');
const field = ref('');
const fieldPlaceholder = computed(() => (layout.value === 'timeline' ? 'due_date' : 'status'));
const queryRef = ref<HTMLInputElement | null>(null);

const examples = computed(() => [
  { key: 'due_week', query: `is:task status:todo due_date:<=${endOfWeek()} sort:due_date` },
  { key: 'done_week', query: `is:task status:done completed_at:>=${weekStart()} sort:-completed_at` },
  { key: 'in_progress', query: 'is:task status:in_progress' },
  { key: 'projects', query: 'is:project status:active' },
  { key: 'overdue', query: `is:task status:todo due_date:<${today()} sort:due_date` },
  // This week's work as a board: what is open, and what was finished since
  // Monday — so a card dragged to Done stays on the board until the week ends.
  { key: 'task_board', query: `is:task (-status:done OR completed_at:>=${weekStart()})`, layout: 'kanban' as const },
  { key: 'due_line', query: 'is:task -status:done sort:due_date', layout: 'timeline' as const },
]);

function endOfWeek(): string {
  const [y, m, d] = weekStart().split('-').map(Number);
  const end = new Date(y, m - 1, d + 6);
  const pad = (n: number) => String(n).padStart(2, '0');
  return `${end.getFullYear()}-${pad(end.getMonth() + 1)}-${pad(end.getDate())}`;
}

function useExample(e: { key: string; query: string; layout?: 'grid' | 'kanban' | 'timeline' }) {
  query.value = e.query;
  layout.value = e.layout ?? 'grid';
  if (!title.value) title.value = t(`whiteboard.live.example.${e.key}`);
}

function create() {
  if (!query.value.trim()) return;
  emit('create', {
    query: query.value.trim(),
    title: title.value.trim(),
    layout: layout.value,
    ...(layout.value !== 'grid' ? { field: field.value.trim() || fieldPlaceholder.value } : {}),
  });
}

onMounted(() => nextTick(() => queryRef.value?.focus()));
</script>

<template>
  <div class="wb-lf-backdrop" @click.self="emit('close')" @keydown.escape.stop="emit('close')" @keydown="keepFocus">
    <form ref="boxRef" class="wb-lf" role="dialog" aria-modal="true" :aria-label="t('whiteboard.live.title')" @submit.prevent="create">
      <h2 class="wb-lf-title">{{ t('whiteboard.live.title') }}</h2>
      <p class="wb-lf-hint">{{ t('whiteboard.live.hint') }}</p>
      <label class="wb-lf-label">
        {{ t('whiteboard.live.query') }}
        <input ref="queryRef" v-model="query" class="wb-lf-input" placeholder="is:task status:todo tag:work" spellcheck="false" />
      </label>
      <div class="wb-lf-examples">
        <button v-for="e in examples" :key="e.key" type="button" class="wb-lf-example" @click="useExample(e)">
          {{ t(`whiteboard.live.example.${e.key}`) }}
        </button>
      </div>
      <div class="wb-lf-label" role="radiogroup" :aria-label="t('whiteboard.live.layout')">
        {{ t('whiteboard.live.layout') }}
        <div class="wb-lf-layouts">
          <button
            v-for="l in (['grid', 'kanban', 'timeline'] as const)"
            :key="l"
            type="button"
            role="radio"
            class="wb-lf-example"
            :class="{ 'wb-lf-example--on': layout === l }"
            :aria-checked="layout === l"
            @click="layout = l"
          >{{ t(`whiteboard.live.layouts.${l}`) }}</button>
        </div>
      </div>
      <label v-if="layout !== 'grid'" class="wb-lf-label">
        {{ t(layout === 'kanban' ? 'whiteboard.live.group_by' : 'whiteboard.live.date_field') }}
        <input v-model="field" class="wb-lf-input" :placeholder="fieldPlaceholder" spellcheck="false" />
      </label>
      <label class="wb-lf-label">
        {{ t('whiteboard.live.name') }}
        <input v-model="title" class="wb-lf-input" />
      </label>
      <div class="wb-lf-actions">
        <button type="button" class="wb-lf-btn" @click="emit('close')">{{ t('whiteboard.close') }}</button>
        <button type="submit" class="wb-lf-btn wb-lf-btn--primary" :disabled="!query.trim()">{{ t('whiteboard.live.create') }}</button>
      </div>
    </form>
  </div>
</template>

<style scoped>
.wb-lf-backdrop {
  position: fixed;
  inset: 0;
  z-index: 300;
  display: flex;
  align-items: flex-start;
  justify-content: center;
  padding: 12vh 16px 16px;
  background: rgba(0, 0, 0, 0.3);
}
.wb-lf-layouts {
  display: flex;
  gap: 6px;
  margin-top: 4px;
}
.wb-lf-example--on {
  color: #fff;
  background: var(--color-accent);
  border-color: var(--color-accent);
}
.wb-lf {
  width: min(520px, 100%);
  display: flex;
  flex-direction: column;
  gap: 10px;
  padding: 18px;
  border-radius: 14px;
  background: var(--color-surface, #fff);
  border: 1px solid var(--color-border, #e6e6e6);
  box-shadow: 0 20px 60px rgba(0, 0, 0, 0.25);
}
.dark .wb-lf {
  background: var(--color-surface-dark, #1e1e1e);
  border-color: var(--color-border-dark, #333);
}
.wb-lf-title { font-size: 16px; font-weight: 700; }
.wb-lf-hint { font-size: 13px; color: var(--color-text-secondary, #52525b); }
.dark .wb-lf-hint { color: var(--color-text-secondary-dark, #a1a1aa); }
.wb-lf-label { display: flex; flex-direction: column; gap: 4px; font-size: 12px; font-weight: 600; }
.wb-lf-input {
  padding: 7px 10px;
  border-radius: 8px;
  font-size: 14px;
  font-weight: 400;
  border: 1px solid var(--color-border, #e6e6e6);
  background: transparent;
  color: inherit;
}
.dark .wb-lf-input { border-color: var(--color-border-dark, #333); }
.wb-lf-input:focus { outline: 2px solid var(--color-accent); outline-offset: -1px; }
.wb-lf-examples { display: flex; flex-wrap: wrap; gap: 6px; }
.wb-lf-example {
  padding: 3px 10px;
  border-radius: 999px;
  font-size: 12px;
  border: 1px solid var(--color-border, #e6e6e6);
}
.dark .wb-lf-example { border-color: var(--color-border-dark, #333); }
.wb-lf-example:hover, .wb-lf-example:focus-visible { border-color: var(--color-accent); color: var(--color-accent); }
/* A ring you can see from across the room, not just a recoloured border. */
.wb-lf-example:focus-visible { outline: 2px solid var(--color-accent); outline-offset: 2px; }
/* The light accent is about 3:1 on the dark surface; the dark one clears 4.5:1. */
.dark .wb-lf-example:hover, .dark .wb-lf-example:focus-visible { border-color: var(--color-accent-dark); color: var(--color-accent-dark); }
.dark .wb-lf-example:focus-visible { outline-color: var(--color-accent-dark); }
.dark .wb-lf-input:focus { outline-color: var(--color-accent-dark); }
.wb-lf-actions { display: flex; justify-content: flex-end; gap: 8px; margin-top: 4px; }
.wb-lf-btn { padding: 6px 14px; border-radius: 8px; font-size: 13px; }
.wb-lf-btn--primary { background: var(--color-accent); color: #fff; }
.wb-lf-btn--primary:disabled { opacity: 0.5; }
</style>
