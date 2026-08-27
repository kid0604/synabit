<script setup lang="ts">
/**
 * Where a cap goes when it stops being fleeting.
 *
 * QuickCap is deliberately outside the graph: a cap is a thought caught
 * before it is lost, not knowledge. Value appears at the moment it becomes
 * something else — so promotion is the most important action in the app, and
 * until now it was two small icons at the bottom of a card.
 *
 * # Why a palette rather than more icons
 *
 * The card's action row already holds five buttons. Every destination added
 * as a sixth icon makes the previous five harder to hit, and there are more
 * coming: an event, a person, a transaction. A list that can be searched and
 * driven from the keyboard scales where a row of icons does not, and it suits
 * working through an inbox — which is a keyboard job, not a mouse one.
 *
 * # The destination that was missing
 *
 * "Append to an existing note" is the one this exists for. Processing a
 * fleeting note is mostly "this belongs in the note I already wrote about X",
 * not "make a new note" — so the most common move in the whole method had no
 * button at all.
 */
import { ref, computed, onMounted, onUnmounted, nextTick, watch } from 'vue';
import { invoke } from '@tauri-apps/api/core';
import { FileText, FilePlus, CheckSquare, Search, CornerDownLeft, Calendar, User, Wallet } from 'lucide-vue-next';

import { logger } from '../../utils/logger';

export type PromoteTarget =
  | { kind: 'new-note' }
  | { kind: 'new-task' }
  | { kind: 'new-event' }
  | { kind: 'new-transaction' }
  | { kind: 'append-note'; relPath: string; title: string }
  | { kind: 'append-person'; relPath: string; title: string };

const props = defineProps<{
  vaultPath: string;
  /** How many caps are being promoted at once. */
  capCount: number;
  /** Whether this vault has Finance accounts to book a transaction against. */
  financeReady: boolean;
}>();
const emit = defineEmits<{ (e: 'close'): void; (e: 'choose', target: PromoteTarget): void }>();

/**
 * Which screen is showing. The second one is the same list either way — a
 * search box over existing nodes — so notes and people share it rather than
 * growing a screen each.
 */
const step = ref<'destinations' | 'pick-note' | 'pick-person'>('destinations');
const query = ref('');
const highlighted = ref(0);
const inputRef = ref<HTMLInputElement | null>(null);

interface Hit {
  id: string;
  title: string;
}

const noteHits = ref<Hit[]>([]);

/** Loaded once when the person step opens: a vault holds dozens, not thousands. */
const people = ref<Hit[]>([]);

const personHits = computed(() => {
  const q = query.value.trim().toLowerCase();
  if (!q) return people.value.slice(0, 20);
  return people.value.filter((p) => p.title.toLowerCase().includes(q)).slice(0, 20);
});

const visibleHits = computed(() => (step.value === 'pick-person' ? personHits.value : noteHits.value));
const isSearching = ref(false);
let searchTimer: ReturnType<typeof setTimeout> | null = null;

const destinations = computed(() => {
  const all = [
    { id: 'append-note', icon: FileText, label: 'append_to_note', hint: 'append_to_note_hint' },
    { id: 'new-note', icon: FilePlus, label: 'new_note', hint: 'new_note_hint' },
    { id: 'new-task', icon: CheckSquare, label: 'new_task', hint: 'new_task_hint' },
    { id: 'new-event', icon: Calendar, label: 'new_event', hint: 'new_event_hint' },
    { id: 'append-person', icon: User, label: 'append_to_person', hint: 'append_to_person_hint' },
  ];

  // A transaction carries one amount, so several caps cannot become one; and
  // it needs an account, which a vault that has never opened Finance does not
  // have. Both cases hide the entry rather than offering something that would
  // only be refused.
  if (props.financeReady && props.capCount === 1) {
    all.push({ id: 'new-transaction', icon: Wallet, label: 'new_transaction', hint: 'new_transaction_hint' });
  }
  const q = query.value.trim().toLowerCase();
  if (!q) return all;
  return all.filter((d) => d.label.includes(q) || d.id.includes(q));
});

/** Whatever the arrow keys are currently moving through. */
const rowCount = computed(() =>
  step.value === 'destinations' ? destinations.value.length : visibleHits.value.length,
);

const focusInput = async () => {
  await nextTick();
  inputRef.value?.focus();
};

const searchNotes = async (q: string) => {
  if (!q.trim()) {
    noteHits.value = [];
    return;
  }
  isSearching.value = true;
  try {
    const response = await invoke<{ results: { id: string; title: string }[] }>('search_notes', {
      vaultPath: props.vaultPath,
      query: q,
    });
    // Only apply if the user has not typed past this query.
    if (query.value === q) {
      noteHits.value = response.results.slice(0, 20).map((r) => ({ id: r.id, title: r.title }));
      highlighted.value = 0;
    }
  } catch (e) {
    logger.error('Could not search notes', e);
  } finally {
    isSearching.value = false;
  }
};

watch(query, (q) => {
  if (step.value !== 'pick-note') {
    highlighted.value = 0;
    return;
  }
  if (searchTimer) clearTimeout(searchTimer);
  searchTimer = setTimeout(() => void searchNotes(q), 180);
});

const enterPicker = async (next: 'pick-note' | 'pick-person') => {
  step.value = next;
  query.value = '';
  noteHits.value = [];
  highlighted.value = 0;

  if (next === 'pick-person' && people.value.length === 0) {
    try {
      const rows = await invoke<{ id: string; title: string }[]>('get_node_summaries', {
        nodeType: 'person',
      });
      people.value = rows.map((r) => ({ id: r.id, title: r.title }));
    } catch (e) {
      logger.error('Could not load people', e);
    }
  }

  await focusInput();
};

const choose = (index: number) => {
  if (step.value === 'destinations') {
    const destination = destinations.value[index];
    if (!destination) return;
    if (destination.id === 'append-note') {
      void enterPicker('pick-note');
      return;
    }
    if (destination.id === 'append-person') {
      void enterPicker('pick-person');
      return;
    }
    emit('choose', { kind: destination.id as 'new-note' | 'new-task' | 'new-event' | 'new-transaction' });
    return;
  }

  const hit = visibleHits.value[index];
  if (!hit) return;
  emit('choose', {
    kind: step.value === 'pick-person' ? 'append-person' : 'append-note',
    relPath: hit.id,
    title: hit.title,
  });
};

const onKeydown = (event: KeyboardEvent) => {
  switch (event.key) {
    case 'Escape':
      event.preventDefault();
      // Back out one step rather than closing outright: the note picker is a
      // place you can arrive at by mistake.
      if (step.value !== 'destinations') {
        step.value = 'destinations';
        query.value = '';
        highlighted.value = 0;
        void focusInput();
      } else {
        emit('close');
      }
      return;
    case 'ArrowDown':
      event.preventDefault();
      if (rowCount.value > 0) highlighted.value = (highlighted.value + 1) % rowCount.value;
      return;
    case 'ArrowUp':
      event.preventDefault();
      if (rowCount.value > 0) {
        highlighted.value = (highlighted.value - 1 + rowCount.value) % rowCount.value;
      }
      return;
    case 'Enter':
      event.preventDefault();
      choose(highlighted.value);
      return;
  }
};

onMounted(() => {
  window.addEventListener('keydown', onKeydown);
  void focusInput();
});

onUnmounted(() => {
  window.removeEventListener('keydown', onKeydown);
  if (searchTimer) clearTimeout(searchTimer);
});
</script>

<template>
  <div
    class="fixed inset-0 z-[130] flex items-start justify-center p-4 pt-[12vh] bg-black/40 dark:bg-black/60 backdrop-blur-sm"
    @click="emit('close')"
  >
    <div
      class="w-full max-w-lg rounded-2xl bg-white dark:bg-[#1e1e1e] border border-[#e6e6e6] dark:border-[#2c2c2c] shadow-xl overflow-hidden flex flex-col"
      @click.stop
    >
      <div class="flex items-center gap-3 px-4 py-3 border-b border-[#e6e6e6] dark:border-[#2c2c2c]">
        <Search class="w-4 h-4 text-gray-400 shrink-0" />
        <input
          ref="inputRef"
          v-model="query"
          type="text"
          class="flex-1 bg-transparent outline-none text-[15px] text-[#1c1c1e] dark:text-[#f4f4f5] placeholder-gray-400"
          :placeholder="step === 'destinations' ? $t('quickcap.promote_placeholder') : step === 'pick-person' ? $t('quickcap.promote_find_person') : $t('quickcap.promote_find_note')"
        />
      </div>

      <div class="max-h-[46vh] overflow-y-auto py-1">
        <template v-if="step === 'destinations'">
          <button
            v-for="(destination, index) in destinations"
            :key="destination.id"
            @click="choose(index)"
            @mouseenter="highlighted = index"
            class="w-full flex items-center gap-3 px-4 py-2.5 text-left transition-colors cursor-pointer"
            :class="highlighted === index ? 'bg-gray-100 dark:bg-[#2a2a2a]' : ''"
          >
            <component :is="destination.icon" class="w-4 h-4 text-gray-500 shrink-0" />
            <span class="flex-1 min-w-0">
              <span class="block text-[14px] text-[#1c1c1e] dark:text-[#f4f4f5]">{{ $t(`quickcap.${destination.label}`) }}</span>
              <span class="block text-[11px] text-gray-400 dark:text-gray-500">{{ $t(`quickcap.${destination.hint}`) }}</span>
            </span>
            <CornerDownLeft v-if="highlighted === index" class="w-3.5 h-3.5 text-gray-400 shrink-0" />
          </button>
        </template>

        <template v-else>
          <button
            v-for="(hit, index) in visibleHits"
            :key="hit.id"
            @click="choose(index)"
            @mouseenter="highlighted = index"
            class="w-full flex items-center gap-3 px-4 py-2.5 text-left transition-colors cursor-pointer"
            :class="highlighted === index ? 'bg-gray-100 dark:bg-[#2a2a2a]' : ''"
          >
            <component :is="step === 'pick-person' ? User : FileText" class="w-4 h-4 text-gray-500 shrink-0" />
            <span class="flex-1 min-w-0 truncate text-[14px] text-[#1c1c1e] dark:text-[#f4f4f5]">{{ hit.title }}</span>
            <CornerDownLeft v-if="highlighted === index" class="w-3.5 h-3.5 text-gray-400 shrink-0" />
          </button>

          <p v-if="!isSearching && visibleHits.length === 0" class="px-4 py-6 text-center text-[13px] text-gray-400">
            {{ step === 'pick-person'
                ? $t('quickcap.promote_no_people')
                : query.trim() ? $t('quickcap.promote_no_notes') : $t('quickcap.promote_find_note') }}
          </p>
        </template>
      </div>

      <div class="px-4 py-2 border-t border-[#e6e6e6] dark:border-[#2c2c2c] text-[11px] text-gray-400 dark:text-gray-500 select-none">
        {{ $t('quickcap.promote_hint') }}
      </div>
    </div>
  </div>
</template>
