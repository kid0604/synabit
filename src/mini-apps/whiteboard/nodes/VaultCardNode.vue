<script setup lang="ts">
import { computed, nextTick, onMounted, ref, watch } from 'vue';
import { useI18n } from 'vue-i18n';
import { NodeResizer } from '@vue-flow/node-resizer';
import { Handle, Position } from '@vue-flow/core';
import { CheckSquare, Square, User, CalendarDays, FolderKanban, FileText, File, LayoutDashboard, ExternalLink, CircleAlert } from 'lucide-vue-next';
import { useNodeService } from '../../../composables/useNodeService';
import { useEventBus } from '../../../composables/useEventBus';
import { logger } from '../../../utils/logger';
import { showAppNotice } from '../../../composables/useAppNotice';
import { today } from '../vaultCards';

/**
 * A thing from the vault, live on the board. See `vaultCards.ts`.
 *
 * Loaded when shown and again whenever the vault says it changed. What can
 * be changed from here goes to the vault, and the card follows from there.
 */
const props = defineProps<{
  id: string;
  selected?: boolean;
  data: { ref: string; kind: string; title?: string; width?: number; height?: number; locked?: boolean };
}>();

const emit = defineEmits<{
  (e: 'update:data', data: any): void;
  (e: 'open', ref: string, kind: string): void;
}>();

const { t, locale } = useI18n();
const nodes = useNodeService();
const bus = useEventBus();

const node = ref<any | null>(null);
const missing = ref(false);

async function load() {
  try {
    const found = await nodes.getNode(props.data.ref);
    node.value = found;
    missing.value = !found;
    // The board keeps the last-seen name, for previews, search and Syn.
    if (found?.title && found.title !== props.data.title) emit('update:data', { title: found.title, kind: found.node_type });
  } catch (err) {
    logger.error('Could not load a card', err as string);
    missing.value = true;
  }
}

onMounted(load);
watch(() => props.data.ref, load);
// The vault said something changed: if it is this card's thing, look again.
bus.on('node:updated', (payload: any) => {
  if (!payload || payload.id === props.data.ref || payload.relPath === props.data.ref) void load();
});
bus.on('vault:file-modified', (payload: any) => {
  const paths: string[] = Array.isArray(payload) ? payload : payload?.paths ?? [];
  if (paths.some((p) => p.endsWith(props.data.ref))) void load();
});

const kind = computed(() => node.value?.node_type ?? props.data.kind);
const title = computed(() => node.value?.title ?? props.data.title ?? props.data.ref);
const p = computed<Record<string, any>>(() => node.value?.properties ?? {});

const icon = computed(() => ({
  task: p.value.status === 'done' ? CheckSquare : Square,
  person: User,
  event: CalendarDays,
  project: FolderKanban,
  note: FileText,
  whiteboard: LayoutDashboard,
}[kind.value as string] ?? File));

const done = computed(() => kind.value === 'task' && p.value.status === 'done');
const overdue = computed(() => kind.value === 'task' && !done.value && p.value.due_date && p.value.due_date < today());

/** The one or two facts worth showing under the title, by kind. */
const facts = computed<string[]>(() => {
  const v = p.value;
  const date = (s?: string) => (s ? new Date(s).toLocaleDateString(locale.value, { day: 'numeric', month: 'short' }) : '');
  const time = (s?: string) => (s ? new Date(s).toLocaleTimeString(locale.value, { hour: '2-digit', minute: '2-digit' }) : '');
  switch (kind.value) {
    case 'task':
      return [v.due_date ? t('whiteboard.card.due', { date: date(v.due_date) }) : '', v.priority ? String(v.priority) : ''].filter(Boolean);
    case 'event':
      return [v.is_all_day ? date(v.start_at) : `${date(v.start_at)} ${time(v.start_at)}`, v.location ?? ''].filter(Boolean);
    case 'person':
      return [v.company ?? '', v.email ?? v.phone ?? ''].filter(Boolean);
    case 'project':
      return [v.status ?? '', v.due_date ? t('whiteboard.card.due', { date: date(v.due_date) }) : ''].filter(Boolean);
    default:
      return [String(node.value?.preview ?? node.value?.content ?? '').replace(/[#*_`>]/g, '').trim().slice(0, 120)].filter(Boolean);
  }
});

const busy = ref(false);

/** Tick a task off, or back on — in the vault, where the Tasks app sees it too. */
async function toggleDone() {
  if (kind.value !== 'task' || busy.value || props.data.locked) return;
  busy.value = true;
  try {
    const finishing = !done.value;
    await nodes.writeNode({
      relPath: props.data.ref,
      nodeType: 'task',
      title: title.value,
      properties: finishing ? { status: 'done', completed_at: today() } : { status: 'todo', completed_at: null },
      eventType: 'updated',
    });
    // What the Tasks app says when it ticks one off, so whatever listens —
    // streaks, the project's progress — hears it from here too.
    bus.emit('task:status-changed', { id: props.data.ref, oldStatus: finishing ? 'todo' : 'done', newStatus: finishing ? 'done' : 'todo', title: title.value });
    if (finishing) bus.emit('task:completed', { id: props.data.ref, title: title.value, projectId: p.value.project_id });
    await load();
  } catch (err) {
    logger.error('Could not change the task', err as string);
    showAppNotice(t('whiteboard.fail.card_change'), 'error');
  } finally {
    busy.value = false;
  }
}

// ─── Renaming ──────────────────────────────────────────────
const editing = ref(false);
const draft = ref('');
const inputRef = ref<HTMLInputElement | null>(null);
const renamable = computed(() => !missing.value && ['task', 'project', 'note', 'event', 'person'].includes(kind.value));

function startEdit() {
  if (!renamable.value || props.data.locked) return;
  editing.value = true;
  draft.value = title.value;
  nextTick(() => {
    inputRef.value?.focus();
    inputRef.value?.select();
  });
}

/** A new title, written to the thing itself. */
async function finishEdit() {
  if (!editing.value) return;
  editing.value = false;
  const next = draft.value.trim();
  if (!next || next === title.value || !node.value) return;
  try {
    await nodes.writeNode({ relPath: props.data.ref, nodeType: node.value.node_type, title: next, properties: {}, eventType: 'updated' });
    await load();
  } catch (err) {
    logger.error('Could not rename', err as string);
    showAppNotice(t('whiteboard.fail.card_change'), 'error');
  }
}

function onResizeEnd(event: any) {
  emit('update:data', { width: Math.round(event.params.width), height: Math.round(event.params.height) });
}

defineExpose({ startEdit });
</script>

<template>
  <div class="wb-card" :class="[`wb-card--${kind}`, { 'is-done': done, 'is-missing': missing }]" @dblclick.stop="startEdit">
    <NodeResizer :is-visible="!!selected && !data.locked" :min-width="180" :min-height="72" color="var(--wb-selection, var(--color-accent))" @resize-end="onResizeEnd" />

    <div class="wb-card__head">
      <button
        v-if="kind === 'task' && !missing"
        class="wb-card__check nodrag"
        :aria-pressed="done"
        :aria-label="t(done ? 'whiteboard.card.reopen' : 'whiteboard.card.complete')"
        :title="t(done ? 'whiteboard.card.reopen' : 'whiteboard.card.complete')"
        :disabled="busy"
        @click.stop="toggleDone"
      >
        <component :is="icon" :size="16" />
      </button>
      <component :is="missing ? CircleAlert : icon" v-else :size="16" class="wb-card__icon" />

      <input
        v-if="editing"
        ref="inputRef"
        v-model="draft"
        class="wb-card__input nodrag nopan"
        :aria-label="t('whiteboard.card.title')"
        @blur="finishEdit"
        @keydown.enter="finishEdit"
        @keydown.escape.stop="editing = false"
      />
      <span v-else class="wb-card__title">{{ title }}</span>

      <button
        v-if="!missing"
        class="wb-card__open nodrag"
        :title="t('whiteboard.card.open')"
        :aria-label="t('whiteboard.card.open')"
        @click.stop="emit('open', data.ref, kind)"
      >
        <ExternalLink :size="14" />
      </button>
    </div>

    <p v-if="missing" class="wb-card__facts">{{ t('whiteboard.card.missing') }}</p>
    <p v-else-if="facts.length" class="wb-card__facts" :class="{ 'is-overdue': overdue }">{{ facts.join(' · ') }}</p>

    <Handle id="top" type="source" :position="Position.Top" class="wb-card__handle" />
    <Handle id="right" type="source" :position="Position.Right" class="wb-card__handle" />
    <Handle id="bottom" type="source" :position="Position.Bottom" class="wb-card__handle" />
    <Handle id="left" type="source" :position="Position.Left" class="wb-card__handle" />
  </div>
</template>

<style scoped>
.wb-card {
  position: relative;
  width: 100%;
  height: 100%;
  padding: 10px 12px;
  border-radius: 12px;
  background: var(--color-surface, #fff);
  border: 1px solid var(--color-border, #e6e6e6);
  border-left: 4px solid var(--card-accent, #94a3b8);
  box-shadow: 0 1px 3px rgba(0, 0, 0, 0.06);
  overflow: hidden;
  cursor: grab;
}
.dark .wb-card {
  background: var(--color-surface-dark, #1e1e1e);
  border-color: var(--color-border-dark, #333);
}
.wb-card--task { --card-accent: #3b82f6; }
.wb-card--event { --card-accent: #f59e0b; }
.wb-card--person { --card-accent: #ec4899; }
.wb-card--project { --card-accent: #7c3aed; }
.wb-card--note { --card-accent: #10b981; }
.wb-card.is-missing { opacity: 0.6; border-style: dashed; }
.wb-card__head {
  display: flex;
  align-items: center;
  gap: 8px;
}
.wb-card__icon,
.wb-card__check {
  flex-shrink: 0;
  color: var(--card-accent);
}
.wb-card__check {
  display: flex;
  padding: 2px;
  border-radius: 4px;
  cursor: pointer;
}
.wb-card__check:hover,
.wb-card__check:focus-visible {
  background: color-mix(in srgb, var(--card-accent) 15%, transparent);
  outline: none;
}
.wb-card__title {
  flex: 1;
  min-width: 0;
  font-size: 14px;
  font-weight: 600;
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}
.is-done .wb-card__title {
  text-decoration: line-through;
  opacity: 0.6;
}
.wb-card__input {
  flex: 1;
  min-width: 0;
  font: inherit;
  font-size: 14px;
  font-weight: 600;
  background: transparent;
  border: none;
  border-bottom: 1px solid var(--color-accent);
  outline: none;
  color: inherit;
}
.wb-card__open {
  flex-shrink: 0;
  display: flex;
  padding: 3px;
  border-radius: 6px;
  color: var(--color-text-secondary, #52525b);
  opacity: 0.6;
}
.wb-card__open:hover,
.wb-card__open:focus-visible {
  opacity: 1;
  background: var(--color-surface-hover, #f4f4f5);
  outline: none;
}
.dark .wb-card__open:hover {
  background: var(--color-surface-hover-dark, #2a2a2a);
}
.wb-card__facts {
  margin-top: 6px;
  font-size: 12px;
  color: var(--color-text-secondary, #52525b);
  display: -webkit-box;
  -webkit-line-clamp: 3;
  -webkit-box-orient: vertical;
  overflow: hidden;
}
.dark .wb-card__facts {
  color: var(--color-text-secondary-dark, #a1a1aa);
}
.wb-card__facts.is-overdue {
  color: var(--color-danger, #dc2626);
}
.wb-card__handle {
  width: 10px !important;
  height: 10px !important;
  background: var(--color-accent) !important;
  border: 2px solid white !important;
  opacity: 0;
}
.wb-card:hover .wb-card__handle {
  opacity: 1;
}
/* The secondary grey is for light paper; on dark, its own. */
.dark .wb-card__open {
  color: var(--color-text-secondary-dark, #a1a1aa);
}
</style>
