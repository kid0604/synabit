<script setup lang="ts">
/**
 * The room on this Mac, as far as a person needs to see it: whether it holds,
 * what is running in it, and the button that ends all of it.
 *
 * Whether Syn may use the room at all is a permission like the others, asked
 * on a card the first time and listed with them; it is not repeated here.
 * Stopping is here, and in the tray, because it must not depend on anything
 * the model could be talked out of: it goes straight to `syn_machine_stop_all`.
 *
 * The folders open to the rooms are here too: each can be closed, allowed away
 * from the computer, or opened from here in the system's dialog. A folder is
 * the app's alone until the switch says otherwise (design §8, A23).
 */
import { onMounted, onBeforeUnmount, ref, computed } from 'vue';
import { invoke } from '@tauri-apps/api/core';
import { useI18n } from 'vue-i18n';
import { FolderOpen, Loader2, ShieldCheck, ShieldAlert, Square, Undo2, X as XIcon } from 'lucide-vue-next';
import { logger } from '../../../utils/logger';
import { dialogTitle, refusedText, warnAboutHidden, type Door, type Picked } from '../composables/doorPick';

const props = defineProps<{ vaultPath: string }>();

interface Status {
  sealed: boolean | null;
  broken: string | null;
  running: number;
}

interface Report {
  sealed: boolean;
  broken: string | null;
  lines: { verdict: 'held' | 'leaked' | 'no_control' | 'skipped' }[];
}

const { t, locale } = useI18n();
const day = (iso: string) => new Date(iso).toLocaleDateString(locale.value);

const status = ref<Status | null>(null);
const checking = ref(false);
const stopping = ref(false);
const said = ref('');

const load = async () => {
  try {
    status.value = await invoke<Status>('syn_machine_status');
  } catch (e) {
    logger.warn('[Syn] Could not read the room status', e);
  }
};

const check = async () => {
  checking.value = true;
  said.value = '';
  try {
    const report = await invoke<Report>('syn_machine_check');
    const count = (v: string) => report.lines.filter(l => l.verdict === v).length;
    said.value = t('syn.machine_check_result', {
      held: count('held'),
      leaked: count('leaked'),
      other: count('no_control'),
    });
  } catch (e) {
    said.value = String(e);
  } finally {
    checking.value = false;
    await load();
  }
};

const stopAll = async () => {
  stopping.value = true;
  try {
    const n = await invoke<number>('syn_machine_stop_all');
    said.value = t('syn.machine_stopped', { n }, n);
  } catch (e) {
    said.value = String(e);
  } finally {
    stopping.value = false;
    await load();
  }
};

type Listed = Door & { expires: string };
const doors = ref<Listed[]>([]);
const opening = ref(false);
const doorSaid = ref('');

const loadDoors = async () => {
  if (!props.vaultPath) return;
  try {
    doors.value = await invoke<Listed[]>('syn_doors', { vaultPath: props.vaultPath });
  } catch (e) {
    logger.warn('[Syn] Could not read the open folders', e);
  }
};

const addDoor = async () => {
  opening.value = true;
  doorSaid.value = '';
  try {
    const picked = await invoke<Picked>('syn_add_door', { vaultPath: props.vaultPath, title: dialogTitle() });
    if (picked.outcome === 'refused') doorSaid.value = refusedText(picked.why);
    if (picked.outcome === 'opened') {
      doorSaid.value = t('syn.door_opened', { name: picked.door.name });
      await warnAboutHidden(picked);
    }
  } catch (e) {
    doorSaid.value = String(e);
  } finally {
    opening.value = false;
    await loadDoors();
  }
};

const closeDoor = async (door: Listed) => {
  try {
    await invoke('syn_remove_door', { vaultPath: props.vaultPath, id: door.id });
  } catch (e) {
    doorSaid.value = String(e);
  }
  await loadDoors();
};

const setAway = async (door: Listed, away: boolean) => {
  try {
    await invoke('syn_door_away', { vaultPath: props.vaultPath, id: door.id, away });
  } catch (e) {
    doorSaid.value = String(e);
  }
  await loadDoors();
};

interface Batch {
  id: string;
  door_name: string;
  root: string;
  at: string;
  standing: 'finished' | 'interrupted' | 'undone';
  moved: number;
  trashed: number;
  made: number;
  can_undo: boolean;
}
const batches = ref<Batch[]>([]);
const undoing = ref<string | null>(null);
const batchSaid = ref('');

const loadBatches = async () => {
  try {
    batches.value = await invoke<Batch[]>('syn_rehearsal_batches');
  } catch (e) {
    logger.warn('[Syn] Could not read the applied changes', e);
  }
};

const undoBatch = async (batch: Batch) => {
  undoing.value = batch.id;
  batchSaid.value = '';
  try {
    const undone = await invoke<{ put_back: number; left: string[] }>('syn_undo_rehearsal', { id: batch.id });
    batchSaid.value = undone.left.length
      ? t('syn.machine_batch_undo_left', { n: undone.put_back, what: undone.left.join('; ') })
      : t('syn.machine_batch_undo_result', { n: undone.put_back });
  } catch (e) {
    batchSaid.value = String(e);
  } finally {
    undoing.value = null;
    await loadBatches();
  }
};

const state = computed(() => {
  const s = status.value;
  if (!s || s.sealed === null) return { tone: 'quiet', text: t('syn.machine_state_unknown') };
  if (s.sealed) return { tone: 'good', text: t('syn.machine_state_sealed') };
  return { tone: 'bad', text: t('syn.machine_state_broken', { why: s.broken ?? '' }) };
});

// What is running changes while the section is open; a few seconds is soon enough.
let timer: ReturnType<typeof setInterval> | undefined;
onMounted(() => {
  void load();
  void loadDoors();
  void loadBatches();
  timer = setInterval(load, 4000);
});
onBeforeUnmount(() => clearInterval(timer));
</script>

<template>
  <div class="space-y-3">
    <p class="text-xs text-gray-500 dark:text-gray-400 leading-relaxed">{{ t('syn.machine_intro') }}</p>

    <div
      class="flex items-start gap-2 p-3 rounded-xl border text-xs leading-relaxed"
      :class="state.tone === 'bad'
        ? 'border-amber-300 dark:border-amber-500/40 bg-amber-50/50 dark:bg-amber-500/5 text-amber-800 dark:text-amber-200'
        : 'border-gray-200 dark:border-gray-700/50 text-gray-600 dark:text-gray-300'"
    >
      <ShieldAlert v-if="state.tone === 'bad'" class="w-4 h-4 mt-px shrink-0" aria-hidden="true" />
      <ShieldCheck v-else class="w-4 h-4 mt-px shrink-0" :class="state.tone === 'good' ? 'text-accent dark:text-accent-dark' : 'opacity-60'" aria-hidden="true" />
      <span class="min-w-0 break-words">{{ state.text }}</span>
    </div>

    <p v-if="status" class="text-xs text-gray-600 dark:text-gray-300">
      {{ t('syn.machine_running', { n: status.running }, status.running) }}
    </p>

    <div class="flex flex-wrap items-center gap-2">
      <button
        type="button"
        :disabled="checking"
        @click="check"
        class="flex items-center gap-1.5 px-3 py-2 rounded-lg text-xs font-medium
               bg-gray-100 dark:bg-white/5 text-gray-700 dark:text-gray-200
               hover:bg-gray-200 dark:hover:bg-white/10 transition-colors cursor-pointer
               disabled:opacity-50 disabled:cursor-not-allowed"
      >
        <Loader2 v-if="checking" class="w-3.5 h-3.5 animate-spin" aria-hidden="true" />
        <ShieldCheck v-else class="w-3.5 h-3.5" aria-hidden="true" />
        <span>{{ checking ? t('syn.machine_checking') : t('syn.machine_check') }}</span>
      </button>
      <button
        type="button"
        :disabled="stopping"
        @click="stopAll"
        class="flex items-center gap-1.5 px-3 py-2 rounded-lg text-xs font-medium
               bg-red-50 dark:bg-red-500/10 text-red-700 dark:text-red-300
               hover:bg-red-100 dark:hover:bg-red-500/20 transition-colors cursor-pointer
               disabled:opacity-50 disabled:cursor-not-allowed"
      >
        <Loader2 v-if="stopping" class="w-3.5 h-3.5 animate-spin" aria-hidden="true" />
        <Square v-else class="w-3.5 h-3.5" aria-hidden="true" />
        <span>{{ t('syn.machine_stop_all') }}</span>
      </button>
    </div>

    <p v-if="said" class="text-xs text-gray-500 dark:text-gray-400" role="status">{{ said }}</p>

    <div class="pt-3 space-y-2 border-t border-gray-100 dark:border-gray-800">
      <h4 class="text-xs font-medium text-text dark:text-text-dark">{{ t('syn.machine_doors_title') }}</h4>
      <p class="text-xs text-gray-500 dark:text-gray-400 leading-relaxed">{{ t('syn.machine_doors_intro') }}</p>
      <p v-if="!doors.length" class="text-xs text-gray-500 dark:text-gray-400">{{ t('syn.machine_doors_none') }}</p>
      <ul v-else class="space-y-2" data-test="doors">
        <li
          v-for="door in doors"
          :key="door.id"
          class="p-3 rounded-xl border border-gray-200 dark:border-gray-700/50 text-xs space-y-1.5"
        >
          <div class="flex items-start gap-2">
            <FolderOpen class="w-4 h-4 mt-px shrink-0 opacity-60" aria-hidden="true" />
            <div class="min-w-0 flex-1">
              <p class="font-medium text-text dark:text-text-dark break-words">{{ door.name }}</p>
              <p class="text-gray-500 dark:text-gray-400 break-all">{{ door.path }}</p>
              <p class="text-gray-500 dark:text-gray-400">
                {{ t('syn.machine_door_expires', { date: day(door.expires) }) }}
                <template v-if="door.hidden"> · {{ t('syn.machine_door_hidden', { n: door.hidden }, door.hidden) }}</template>
              </p>
            </div>
            <button
              type="button"
              class="flex items-center gap-1 px-2 py-1 rounded-lg text-gray-600 dark:text-gray-300 hover:bg-gray-100 dark:hover:bg-white/5 cursor-pointer"
              @click="closeDoor(door)"
            >
              <XIcon class="w-3.5 h-3.5" aria-hidden="true" />
              <span>{{ t('syn.machine_door_close') }}</span>
            </button>
          </div>
          <label class="flex items-start gap-2 pl-6 cursor-pointer">
            <input
              type="checkbox"
              class="mt-0.5"
              :checked="door.away"
              @change="setAway(door, ($event.target as HTMLInputElement).checked)"
            />
            <span>
              <span class="text-text dark:text-text-dark">{{ t('syn.machine_door_away') }}</span>
              <span class="block text-gray-500 dark:text-gray-400">{{ t('syn.machine_door_away_hint') }}</span>
            </span>
          </label>
        </li>
      </ul>
      <button
        type="button"
        :disabled="opening || !vaultPath"
        @click="addDoor"
        class="flex items-center gap-1.5 px-3 py-2 rounded-lg text-xs font-medium
               bg-gray-100 dark:bg-white/5 text-gray-700 dark:text-gray-200
               hover:bg-gray-200 dark:hover:bg-white/10 transition-colors cursor-pointer
               disabled:opacity-50 disabled:cursor-not-allowed"
      >
        <Loader2 v-if="opening" class="w-3.5 h-3.5 animate-spin" aria-hidden="true" />
        <FolderOpen v-else class="w-3.5 h-3.5" aria-hidden="true" />
        <span>{{ t('syn.machine_door_add') }}</span>
      </button>
      <p v-if="doorSaid" class="text-xs text-gray-500 dark:text-gray-400" role="status">{{ doorSaid }}</p>
    </div>

    <div class="pt-3 space-y-2 border-t border-gray-100 dark:border-gray-800" data-test="batches">
      <h4 class="text-xs font-medium text-text dark:text-text-dark">{{ t('syn.machine_batches_title') }}</h4>
      <p v-if="!batches.length" class="text-xs text-gray-500 dark:text-gray-400">{{ t('syn.machine_batches_none') }}</p>
      <ul v-else class="space-y-2">
        <li
          v-for="batch in batches"
          :key="batch.id"
          class="p-3 rounded-xl border text-xs space-y-1"
          :class="batch.standing === 'interrupted'
            ? 'border-amber-300 dark:border-amber-500/40 bg-amber-50/50 dark:bg-amber-500/5'
            : 'border-gray-200 dark:border-gray-700/50'"
        >
          <div class="flex items-start gap-2">
            <div class="min-w-0 flex-1">
              <p class="text-text dark:text-text-dark break-words">
                {{ t('syn.machine_batch_line', { name: batch.door_name, date: day(batch.at), moved: batch.moved, trashed: batch.trashed, made: batch.made }) }}
              </p>
              <p class="text-gray-500 dark:text-gray-400 break-all">{{ batch.root }}</p>
              <p v-if="batch.standing === 'interrupted'" class="text-amber-800 dark:text-amber-200">{{ t('syn.machine_batch_interrupted') }}</p>
              <p v-if="batch.standing === 'undone'" class="text-gray-500 dark:text-gray-400">{{ t('syn.machine_batch_undone') }}</p>
            </div>
            <button
              v-if="batch.can_undo"
              type="button"
              :disabled="undoing !== null"
              class="flex items-center gap-1 px-2 py-1 rounded-lg text-gray-700 dark:text-gray-200 bg-gray-100 dark:bg-white/5 hover:bg-gray-200 dark:hover:bg-white/10 cursor-pointer disabled:opacity-50"
              @click="undoBatch(batch)"
            >
              <Loader2 v-if="undoing === batch.id" class="w-3.5 h-3.5 animate-spin" aria-hidden="true" />
              <Undo2 v-else class="w-3.5 h-3.5" aria-hidden="true" />
              <span>{{ t('syn.machine_batch_undo') }}</span>
            </button>
          </div>
        </li>
      </ul>
      <p v-if="batchSaid" class="text-xs text-gray-500 dark:text-gray-400 break-words" role="status">{{ batchSaid }}</p>
    </div>
  </div>
</template>
