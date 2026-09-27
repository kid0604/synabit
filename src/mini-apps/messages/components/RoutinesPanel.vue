<script setup lang="ts">
/**
 * The routines the person has set up, and the one place to set them up.
 *
 * Only here. There is no tool that creates a routine and there will not be
 * one: a routine is the person deciding what Syn does while they are not
 * looking, and that decision is not the assistant's to make. See
 * `syn::routine`.
 *
 * The screen says exactly what a routine will do and when — the days in words,
 * the next time it runs — and what it may not do, once, at the top: read and
 * write new notes, never change or remove. A person should be able to decide
 * whether to trust one from this screen alone.
 */
import { onMounted, ref } from 'vue';
import { useI18n } from 'vue-i18n';
import { invoke } from '@tauri-apps/api/core';
import { CalendarClock, Play, Pencil, Trash2, ArrowUpRight, Plus, Smartphone, ShieldCheck } from 'lucide-vue-next';
import { logger } from '../../../utils/logger';
import { blankRoutine, daysInWords, toggleDay, whenInWords, type Routine, type RoutineView } from '../routines';

const props = defineProps<{ vaultPath: string }>();
const emit = defineEmits<{ 'open-conversation': [id: string] }>();

const { t, locale } = useI18n();

const routines = ref<RoutineView[]>([]);
const editing = ref<Routine | null>(null);
const error = ref<string | null>(null);
const started = ref<string | null>(null);

const message = (e: unknown) => (e as { message?: string })?.message ?? String(e);

const load = async () => {
  try {
    routines.value = await invoke<RoutineView[]>('syn_list_routines', { vaultPath: props.vaultPath });
  } catch (e) {
    logger.error('[Syn] Could not read routines', e);
    error.value = message(e);
  }
};
onMounted(load);

const startNew = (template?: 'morning' | 'week') => {
  const routine = blankRoutine();
  if (template === 'morning') {
    routine.name = t('syn.routine_tpl_morning');
    routine.ask = t('syn.routine_tpl_morning_ask');
    routine.schedule = { at: '07:30', weekdays: [1, 2, 3, 4, 5] };
  } else if (template === 'week') {
    routine.name = t('syn.routine_tpl_week');
    routine.ask = t('syn.routine_tpl_week_ask');
    routine.schedule = { at: '17:00', weekdays: [5] };
  }
  editing.value = routine;
  error.value = null;
};

const approve = async (routine: RoutineView) => {
  try {
    await invoke('syn_approve_routine', { vaultPath: props.vaultPath, routineId: routine.id });
    await load();
  } catch (e) {
    error.value = message(e);
  }
};

const edit = (routine: RoutineView) => {
  const { next_run: _next, last_slot: _last, approved_here: _approved, ...plain } = routine;
  editing.value = { ...plain, schedule: { ...plain.schedule, weekdays: [...plain.schedule.weekdays] } };
  error.value = null;
};

const save = async () => {
  if (!editing.value) return;
  try {
    await invoke('syn_save_routine', { vaultPath: props.vaultPath, routine: editing.value });
    editing.value = null;
    error.value = null;
    await load();
  } catch (e) {
    error.value = message(e);
  }
};

const setEnabled = async (routine: RoutineView, enabled: boolean) => {
  const { next_run: _next, last_slot: _last, approved_here: _approved, ...plain } = routine;
  try {
    await invoke('syn_save_routine', { vaultPath: props.vaultPath, routine: { ...plain, enabled } });
    await load();
  } catch (e) {
    error.value = message(e);
  }
};

const remove = async (routine: RoutineView) => {
  if (!window.confirm(`${t('syn.routine_delete')}: ${routine.name}?`)) return;
  try {
    await invoke('syn_delete_routine', { vaultPath: props.vaultPath, routineId: routine.id });
    await load();
  } catch (e) {
    error.value = message(e);
  }
};

const runNow = async (routine: RoutineView) => {
  try {
    await invoke('syn_run_routine_now', { vaultPath: props.vaultPath, routineId: routine.id });
    started.value = routine.id;
    // The conversation is made on the first run; read it back once it exists.
    setTimeout(load, 1500);
  } catch (e) {
    error.value = message(e);
  }
};
</script>

<template>
  <div class="flex-1 overflow-y-auto px-6 py-6">
    <div class="max-w-3xl mx-auto space-y-6">
      <header>
        <h2 class="text-lg font-semibold text-text dark:text-text-dark">{{ t('syn.routines') }}</h2>
        <p class="mt-1 text-sm text-gray-500 dark:text-gray-400">{{ t('syn.routines_explainer') }}</p>
      </header>

      <p v-if="error" role="alert" class="text-sm text-red-600 dark:text-red-400">{{ error }}</p>

      <!-- The form: one routine at a time, with the schedule said back. -->
      <form
        v-if="editing"
        class="rounded-xl border border-violet-200 dark:border-violet-500/30 p-4 space-y-3"
        @submit.prevent="save"
      >
        <label class="block text-sm">
          <span class="font-medium text-text dark:text-text-dark">{{ t('syn.routine_name') }}</span>
          <input
            v-model="editing.name"
            required
            class="mt-1 w-full px-3 py-2 rounded-lg bg-gray-50 dark:bg-white/5 border border-gray-200 dark:border-gray-700/50 text-sm"
          />
        </label>
        <label class="block text-sm">
          <span class="font-medium text-text dark:text-text-dark">{{ t('syn.routine_ask') }}</span>
          <textarea
            v-model="editing.ask"
            required
            rows="4"
            class="mt-1 w-full px-3 py-2 rounded-lg bg-gray-50 dark:bg-white/5 border border-gray-200 dark:border-gray-700/50 text-sm"
          />
        </label>
        <div class="flex flex-wrap items-end gap-4">
          <label class="text-sm">
            <span class="block font-medium text-text dark:text-text-dark">{{ t('syn.routine_at') }}</span>
            <input
              v-model="editing.schedule.at"
              type="time"
              required
              class="mt-1 px-3 py-2 rounded-lg bg-gray-50 dark:bg-white/5 border border-gray-200 dark:border-gray-700/50 text-sm"
            />
          </label>
          <fieldset class="text-sm">
            <legend class="font-medium text-text dark:text-text-dark">
              {{ t('syn.routine_days') }} — {{ daysInWords(editing.schedule.weekdays, t) }}
            </legend>
            <div class="mt-1 flex gap-1">
              <button
                v-for="day in [1, 2, 3, 4, 5, 6, 7]"
                :key="day"
                type="button"
                :aria-pressed="editing.schedule.weekdays.includes(day)"
                class="w-9 h-8 rounded-lg text-xs font-medium focus-visible:outline-2 focus-visible:outline-violet-500"
                :class="editing.schedule.weekdays.includes(day)
                  ? 'bg-violet-600 text-white'
                  : 'bg-gray-100 dark:bg-white/5 text-gray-600 dark:text-gray-300'"
                @click="editing.schedule.weekdays = toggleDay(editing.schedule.weekdays, day)"
              >
                {{ t(`syn.weekday_${day}`) }}
              </button>
            </div>
          </fieldset>
        </div>
        <label class="flex items-center gap-2 text-sm text-text dark:text-text-dark">
          <input v-model="editing.to_phone" type="checkbox" />
          {{ t('syn.routine_to_phone') }}
        </label>
        <div class="flex gap-2 pt-1">
          <button type="submit" class="px-3 py-1.5 rounded-lg text-sm font-medium bg-violet-600 hover:bg-violet-700 text-white">
            {{ t('syn.routine_save') }}
          </button>
          <button type="button" class="px-3 py-1.5 rounded-lg text-sm text-gray-600 dark:text-gray-300 hover:bg-gray-100 dark:hover:bg-white/5" @click="editing = null">
            {{ t('syn.routine_cancel') }}
          </button>
        </div>
      </form>

      <!-- Ways to start one. -->
      <div v-else class="flex flex-wrap items-center gap-2">
        <button type="button" class="inline-flex items-center gap-1.5 px-3 py-1.5 rounded-lg text-sm font-medium bg-violet-600 hover:bg-violet-700 text-white" @click="startNew()">
          <Plus class="w-4 h-4" aria-hidden="true" />{{ t('syn.routine_new') }}
        </button>
        <span class="text-xs text-gray-500">{{ t('syn.routine_templates') }}:</span>
        <button type="button" class="px-2.5 py-1 rounded-lg text-xs border border-gray-200 dark:border-gray-700 hover:bg-gray-50 dark:hover:bg-white/5" @click="startNew('morning')">
          {{ t('syn.routine_tpl_morning') }}
        </button>
        <button type="button" class="px-2.5 py-1 rounded-lg text-xs border border-gray-200 dark:border-gray-700 hover:bg-gray-50 dark:hover:bg-white/5" @click="startNew('week')">
          {{ t('syn.routine_tpl_week') }}
        </button>
      </div>

      <p v-if="!routines.length && !editing" class="text-sm text-gray-400">{{ t('syn.routines_none') }}</p>

      <ul class="space-y-2">
        <li
          v-for="routine in routines"
          :key="routine.id"
          class="rounded-xl border border-gray-200 dark:border-gray-800/60 px-4 py-3"
          :class="routine.enabled ? '' : 'opacity-60'"
        >
          <div class="flex items-start gap-3">
            <CalendarClock class="w-4 h-4 mt-0.5 shrink-0 text-violet-500" aria-hidden="true" />
            <div class="min-w-0 flex-1">
              <p class="text-sm font-medium text-text dark:text-text-dark">{{ routine.name }}</p>
              <p class="mt-0.5 text-xs text-gray-500 dark:text-gray-400">
                {{ daysInWords(routine.schedule.weekdays, t) }} · {{ routine.schedule.at }}
                <template v-if="routine.to_phone"> · <Smartphone class="inline w-3 h-3" aria-hidden="true" /> Telegram</template>
                ·
                <template v-if="routine.next_run">{{ t('syn.routine_next', { when: whenInWords(routine.next_run, locale) }) }}</template>
                <template v-else>{{ t('syn.routine_off') }}</template>
              </p>
              <p class="mt-1 text-xs text-gray-600 dark:text-gray-300 line-clamp-2">{{ routine.ask }}</p>
              <!-- Arrived by sync, or changed elsewhere: the whole question is
                   shown, not two lines of it, because this is what is being
                   agreed to. -->
              <div
                v-if="!routine.approved_here"
                class="mt-2 rounded-lg border border-amber-300 dark:border-amber-500/40 bg-amber-50 dark:bg-amber-500/10 px-3 py-2"
              >
                <p class="text-xs text-amber-800 dark:text-amber-200">{{ t('syn.routine_not_approved') }}</p>
                <p class="mt-1 text-xs whitespace-pre-wrap text-gray-700 dark:text-gray-200">{{ routine.ask }}</p>
                <button
                  type="button"
                  class="mt-2 inline-flex items-center gap-1 px-2 py-1 rounded-lg text-xs font-medium bg-amber-600 hover:bg-amber-700 text-white"
                  @click="approve(routine)"
                >
                  <ShieldCheck class="w-3 h-3" aria-hidden="true" />{{ t('syn.routine_approve') }}
                </button>
              </div>
              <p v-if="started === routine.id" class="mt-1 text-xs text-emerald-600 dark:text-emerald-400" role="status">
                {{ t('syn.routine_started') }}
              </p>
            </div>
            <label class="shrink-0 inline-flex items-center gap-1.5 text-xs text-gray-500">
              <input type="checkbox" :checked="routine.enabled" @change="setEnabled(routine, ($event.target as HTMLInputElement).checked)" />
              {{ t('syn.routine_enabled') }}
            </label>
          </div>
          <div class="mt-2 flex flex-wrap gap-1 pl-7">
            <button v-if="routine.approved_here" type="button" class="inline-flex items-center gap-1 px-2 py-1 rounded-lg text-xs text-violet-600 dark:text-violet-400 hover:bg-violet-50 dark:hover:bg-violet-500/10" @click="runNow(routine)">
              <Play class="w-3 h-3" aria-hidden="true" />{{ t('syn.routine_run_now') }}
            </button>
            <button
              v-if="routine.conversation_id"
              type="button"
              class="inline-flex items-center gap-1 px-2 py-1 rounded-lg text-xs text-gray-600 dark:text-gray-300 hover:bg-gray-100 dark:hover:bg-white/5"
              @click="emit('open-conversation', routine.conversation_id)"
            >
              <ArrowUpRight class="w-3 h-3" aria-hidden="true" />{{ t('syn.routine_open') }}
            </button>
            <button type="button" class="inline-flex items-center gap-1 px-2 py-1 rounded-lg text-xs text-gray-600 dark:text-gray-300 hover:bg-gray-100 dark:hover:bg-white/5" @click="edit(routine)">
              <Pencil class="w-3 h-3" aria-hidden="true" />{{ t('syn.routine_edit') }}
            </button>
            <button type="button" class="inline-flex items-center gap-1 px-2 py-1 rounded-lg text-xs text-red-600 dark:text-red-400 hover:bg-red-50 dark:hover:bg-red-500/10" @click="remove(routine)">
              <Trash2 class="w-3 h-3" aria-hidden="true" />{{ t('syn.routine_delete') }}
            </button>
          </div>
        </li>
      </ul>
    </div>
  </div>
</template>
