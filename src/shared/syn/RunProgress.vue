<script setup lang="ts">
/**
 * What a run is doing, while it does it.
 *
 * Before this the screen had one thing to show during a run: the raw name of
 * the last tool. A run that takes eight rounds was a spinner with `query_nodes`
 * under it — nothing a person could follow, judge, or decide to stop for a
 * reason. Now it shows the plan the model wrote (`update_plan`), what it is
 * doing in words, and how much of the run's allowance it has used.
 *
 * # What is announced
 *
 * One polite live region, and only for a step changing state — "Now: …",
 * "Done: …". Not every tool call and not the counters: a screen reader user
 * asked to listen to "round three, twelve tool calls" every second would turn
 * the whole thing off, and the steps are the part worth hearing.
 */
import { computed, ref, watch } from 'vue';
import { useI18n } from 'vue-i18n';
import type { PlanStep, RunProgress } from '../../mini-apps/messages/types';
import PlanList from './PlanList.vue';
import { shortCount, toolLabel } from './toolLabel';

const props = defineProps<{
  plan: PlanStep[];
  progress: RunProgress | null;
}>();

const { t } = useI18n();

const doing = computed(() => (props.progress?.tool ? toolLabel(t, props.progress.tool) : null));

const counters = computed(() => {
  const p = props.progress;
  if (!p || p.round === 0) return '';
  const parts = [
    p.rounds_max ? t('syn.progress_round', { n: p.round, max: p.rounds_max }) : t('syn.progress_round_open', { n: p.round }),
  ];
  if (p.tool_calls > 0) parts.push(t('syn.tool_calls_count', { n: p.tool_calls }, p.tool_calls));
  if (p.tokens > 0) parts.push(t('syn.progress_tokens', { n: shortCount(p.tokens) }));
  return parts.join(' · ');
});

/** How much of the round allowance is gone, for the thin bar. */
const used = computed(() => {
  const p = props.progress;
  if (!p?.rounds_max) return null;
  return Math.min(100, Math.round((p.round / p.rounds_max) * 100));
});

const announced = ref('');
watch(
  () => props.plan,
  (now, before) => {
    const was = new Map((before ?? []).map(s => [s.text, s.status]));
    const started = now.find(s => s.status === 'doing' && was.get(s.text) !== 'doing');
    const finished = now.find(s => s.status === 'done' && was.has(s.text) && was.get(s.text) !== 'done');
    if (started) announced.value = t('syn.plan_now_doing', { step: started.text });
    else if (finished) announced.value = t('syn.plan_step_done', { step: finished.text });
  },
);
</script>

<template>
  <div>
  <section
    v-if="plan.length || counters"
    class="rounded-xl border border-violet-200/60 dark:border-violet-500/20 bg-violet-50/40 dark:bg-violet-500/5 px-4 py-3 space-y-2"
    :aria-label="$t('syn.progress_label')"
  >
    <div v-if="plan.length">
      <h3 class="text-xs font-semibold uppercase tracking-wide text-violet-600 dark:text-violet-400 mb-1.5">
        {{ $t('syn.plan_title') }}
      </h3>
      <PlanList :steps="plan" />
    </div>
    <div v-if="counters" class="flex items-center gap-3 text-xs text-gray-500 dark:text-gray-400">
      <span v-if="doing" class="font-medium text-violet-600 dark:text-violet-400">{{ doing }}</span>
      <span>{{ counters }}</span>
    </div>
    <div v-if="used !== null" class="h-1 rounded-full bg-violet-100 dark:bg-violet-500/10 overflow-hidden" aria-hidden="true">
      <div class="h-full bg-violet-400 dark:bg-violet-500 transition-[width] duration-300" :style="{ width: `${used}%` }" />
    </div>
  </section>
  <!-- Outside the section, so the region exists before anything is said into
       it: one that appears with its first message is often not read. -->
  <p class="sr-only" aria-live="polite">{{ announced }}</p>
  </div>
</template>
