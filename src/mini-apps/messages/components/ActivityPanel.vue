<script setup lang="ts">
/**
 * What Syn is doing, what is waiting for you, and what finished today.
 *
 * The runs were always on disk and always listed — in the inspector, which is
 * a console: every state mixed together, newest first, one click from a raw
 * transcript. That answers "what happened" for somebody debugging. It does not
 * answer the question a person walking back to their desk has, which is *does
 * anything need me*. This screen is that question first.
 *
 * It is also where work Syn does on its own will report, once it can (the
 * roadmap's phase E): a run nobody is watching needs somewhere to be found.
 */
import { useI18n } from 'vue-i18n';
import { ArrowUpRight, FileSearch, Square, Send } from 'lucide-vue-next';
import type { RunSummary } from '../types';
import { useSynActivity } from '../composables/useSynActivity';

const props = defineProps<{ vaultPath: string }>();
const emit = defineEmits<{
  'open-conversation': [id: string];
  'inspect-run': [id: string];
}>();

const { t } = useI18n();
const { activity, loaded, stop } = useSynActivity(() => props.vaultPath);

const when = (run: RunSummary) => {
  const at = new Date(run.updated_at);
  return Number.isNaN(at.getTime()) ? '' : at.toLocaleTimeString([], { hour: '2-digit', minute: '2-digit' });
};

const sections = [
  { key: 'waiting', title: 'syn.activity_waiting' },
  { key: 'working', title: 'syn.activity_working' },
  { key: 'finished', title: 'syn.activity_finished' },
] as const;
</script>

<template>
  <div class="flex-1 overflow-y-auto px-6 py-6">
    <div class="max-w-3xl mx-auto space-y-6">
      <header>
        <h2 class="text-lg font-semibold text-text dark:text-text-dark">{{ t('syn.activity') }}</h2>
        <p class="mt-1 text-sm text-gray-500 dark:text-gray-400">{{ t('syn.activity_explainer') }}</p>
      </header>

      <p
        v-if="loaded && !activity.working.length && !activity.waiting.length && !activity.finished.length"
        class="text-sm text-gray-400"
      >
        {{ t('syn.activity_none') }}
      </p>

      <section v-for="section in sections" :key="section.key" v-show="activity[section.key].length">
        <h3 class="text-xs font-semibold uppercase tracking-wide text-gray-500 dark:text-gray-400 mb-2">
          {{ t(section.title) }} · {{ activity[section.key].length }}
        </h3>
        <ul class="space-y-2">
          <li
            v-for="run in activity[section.key]"
            :key="run.id"
            class="rounded-xl border px-4 py-3"
            :class="section.key === 'waiting'
              ? 'border-amber-300/60 dark:border-amber-500/30 bg-amber-50/50 dark:bg-amber-500/5'
              : 'border-gray-200 dark:border-gray-800/60'"
          >
            <div class="flex items-start gap-3">
              <div class="min-w-0 flex-1">
                <p class="text-sm text-text dark:text-text-dark line-clamp-2">{{ run.goal }}</p>
                <p class="mt-1 flex flex-wrap items-center gap-x-2 text-[11px] text-gray-500 dark:text-gray-400">
                  <span class="font-medium">{{ t(`syn.run_state_${run.state}`) }}</span>
                  <span>{{ when(run) }}</span>
                  <span v-if="run.tool_calls">{{ t('syn.tool_calls_count', { n: run.tool_calls }, run.tool_calls) }}</span>
                  <span v-if="run.trigger === 'schedule'" class="inline-flex items-center gap-0.5 text-violet-600 dark:text-violet-400">
                    {{ t('syn.activity_from_routine') }}
                  </span>
                  <span v-if="run.surface === 'telegram'" class="inline-flex items-center gap-0.5">
                    <Send class="w-3 h-3" aria-hidden="true" />{{ t('syn.activity_from_telegram') }}
                  </span>
                </p>
              </div>
              <div class="flex shrink-0 items-center gap-1">
                <button
                  v-if="run.state === 'working'"
                  type="button"
                  class="inline-flex items-center gap-1 px-2 py-1 rounded-lg text-xs text-red-600 dark:text-red-400 hover:bg-red-50 dark:hover:bg-red-500/10 focus-visible:outline-2 focus-visible:outline-red-500"
                  @click="stop(run.id)"
                >
                  <Square class="w-3 h-3" aria-hidden="true" />{{ t('syn.activity_stop') }}
                </button>
                <button
                  v-if="run.conversation_id"
                  type="button"
                  class="inline-flex items-center gap-1 px-2 py-1 rounded-lg text-xs text-violet-600 dark:text-violet-400 hover:bg-violet-50 dark:hover:bg-violet-500/10 focus-visible:outline-2 focus-visible:outline-violet-500"
                  @click="emit('open-conversation', run.conversation_id)"
                >
                  <ArrowUpRight class="w-3 h-3" aria-hidden="true" />{{ t('syn.activity_open') }}
                </button>
                <button
                  type="button"
                  class="inline-flex items-center gap-1 px-2 py-1 rounded-lg text-xs text-gray-500 hover:bg-gray-100 dark:hover:bg-white/5 focus-visible:outline-2 focus-visible:outline-violet-500"
                  @click="emit('inspect-run', run.id)"
                >
                  <FileSearch class="w-3 h-3" aria-hidden="true" />{{ t('syn.activity_details') }}
                </button>
              </div>
            </div>
          </li>
        </ul>
      </section>
    </div>
  </div>
</template>
