<script setup lang="ts">
/**
 * How often what Syn has actually fires.
 *
 * The screen the 5 September review asked for as G1: *no number in the app
 * answers "how many times did Syn use memory or a skill in thirty days"*. Every
 * mechanism found dead in this codebase so far — `recall` uncalled in fifteen
 * runs, the skill detector firing on none of seventeen — was found by somebody
 * counting by hand. This does the counting.
 *
 * Plain numbers and bars made of two `div`s, no chart library: the question is
 * "is this zero", and a table answers it better than a picture. Every row says
 * in one line what it means and why it matters, because a number nobody can
 * interpret is a number nobody acts on.
 *
 * Tables carry the numbers; the bars beside them are decoration and hidden
 * from screen readers, which read the figure in the cell instead.
 */
import { computed, onMounted, ref } from 'vue';
import { useI18n } from 'vue-i18n';
import { Loader2, ShieldCheck } from 'lucide-vue-next';
import { useSynStats, share, barWidth, ROUND_BUCKETS, ENDINGS, CEILINGS } from '../composables/useSynStats';
import type { StatsPeriod } from '../types';

const props = defineProps<{ vaultPath: string }>();

const { t, locale } = useI18n();
const { stats, isLoading, error, load } = useSynStats(() => props.vaultPath);

/** Which stretch of runs is shown. Tokens by day are always the recent window. */
const which = ref<'recent' | 'on_disk'>('recent');
const period = computed<StatsPeriod | null>(() => stats.value?.[which.value] ?? null);

const num = (n: number | null | undefined) =>
  n == null ? '—' : n.toLocaleString(locale.value, { maximumFractionDigits: 1 });
const pct = (part: number, whole: number) => {
  const p = share(part, whole);
  return p == null ? '—' : `${p}%`;
};

const roundRows = computed(() => {
  const p = period.value;
  if (!p) return [];
  const max = Math.max(...ROUND_BUCKETS.map(b => p.rounds[b]));
  return ROUND_BUCKETS.map(b => ({ key: b, n: p.rounds[b], width: barWidth(p.rounds[b], max) }));
});

const endingRows = computed(() => {
  const p = period.value;
  if (!p) return [];
  const max = Math.max(...ENDINGS.map(s => p.ended[s] ?? 0));
  return ENDINGS.map(s => ({ key: s, n: p.ended[s] ?? 0, width: barWidth(p.ended[s] ?? 0, max) }));
});

const stoppedByCeiling = computed(() => period.value?.ended.budget_exhausted ?? 0);
const ceilingRows = computed(() => {
  const p = period.value;
  if (!p) return [];
  return CEILINGS.map(c => ({ key: c, n: p.ceilings[c] ?? 0 }));
});

const toolRows = computed(() => {
  const p = period.value;
  if (!p) return [];
  const max = Math.max(...p.tools.map(x => x.calls));
  return p.tools.map(x => ({ ...x, width: barWidth(x.calls, max) }));
});

const droppedSections = computed(() =>
  Object.entries(period.value?.prompt.by_section ?? {}).filter(([, n]) => (n ?? 0) > 0),
);

const dayRows = computed(() => {
  const days = stats.value?.by_day ?? [];
  const max = Math.max(0, ...days.map(d => d.tokens));
  // Newest first: the question is usually "what did today cost".
  return [...days].reverse().map(d => ({ ...d, width: barWidth(d.tokens, max) }));
});

const oldestDay = computed(() => stats.value?.oldest?.slice(0, 10) ?? '—');

defineExpose({ load });
onMounted(load);
</script>

<template>
  <div class="flex-1 overflow-y-auto p-6">
    <p class="text-sm text-gray-500 dark:text-gray-400">{{ t('syn.stats_explainer') }}</p>
    <p class="mt-2 flex items-start gap-1.5 text-xs text-emerald-700 dark:text-emerald-400">
      <ShieldCheck class="w-3.5 h-3.5 mt-px shrink-0" aria-hidden="true" />
      <span>{{ t('syn.stats_private') }}</span>
    </p>

    <p v-if="error" class="mt-4 px-3 py-2 rounded-lg bg-red-50 dark:bg-red-950/40 text-sm text-red-700 dark:text-red-300">
      {{ error }}
    </p>

    <p v-if="isLoading && !stats" class="mt-6 flex items-center gap-2 text-sm text-gray-500 dark:text-gray-400" role="status">
      <Loader2 class="w-4 h-4 animate-spin" aria-hidden="true" /> {{ t('syn.stats_loading') }}
    </p>

    <template v-else-if="stats && period">
      <!-- Which runs. Two buttons rather than a select: there are two
           choices, and both should be visible without opening anything. -->
      <div class="mt-5 flex flex-wrap items-center gap-3">
        <div
          class="inline-flex gap-1 p-0.5 rounded-lg bg-gray-100 dark:bg-gray-800/60"
          role="group"
          :aria-label="t('syn.stats_period_label')"
        >
          <button
            v-for="option in (['recent', 'on_disk'] as const)"
            :key="option"
            type="button"
            class="px-3 py-1 text-xs font-medium rounded-md transition-colors"
            :class="which === option
              ? 'bg-white dark:bg-gray-700 text-text dark:text-text-dark shadow-sm'
              : 'text-gray-500 dark:text-gray-400 hover:text-text dark:hover:text-text-dark'"
            :aria-pressed="which === option"
            @click="which = option"
          >
            {{ option === 'recent'
              ? t('syn.stats_period_recent', { n: stats.recent_days })
              : t('syn.stats_period_on_disk') }}
          </button>
        </div>
        <span v-if="which === 'on_disk'" class="text-xs text-gray-500 dark:text-gray-400">
          {{ t('syn.stats_on_disk_note', { kept: stats.kept, since: oldestDay }) }}
        </span>
      </div>

      <p v-if="!period.runs" class="mt-6 text-sm text-gray-500 dark:text-gray-400">{{ t('syn.stats_empty') }}</p>

      <template v-else>
        <!-- ── Runs ──────────────────────────────────────── -->
        <section class="mt-6" aria-labelledby="stats-runs">
          <h3 id="stats-runs" class="text-sm font-medium text-text dark:text-text-dark">{{ t('syn.stats_runs_title') }}</h3>
          <dl class="mt-2 grid grid-cols-1 sm:grid-cols-3 gap-3">
            <div class="rounded-xl border border-gray-100 dark:border-gray-800/60 p-3">
              <dt class="text-xs text-gray-500 dark:text-gray-400">{{ t('syn.stats_runs_total') }}</dt>
              <dd class="mt-0.5 text-lg font-semibold tabular-nums text-text dark:text-text-dark">{{ num(period.runs) }}</dd>
              <dd class="mt-1 text-xs text-gray-500 dark:text-gray-400">{{ t('syn.stats_runs_total_why') }}</dd>
            </div>
            <div
              v-for="surface in (['app', 'telegram', 'routine'] as const)"
              :key="surface"
              class="rounded-xl border border-gray-100 dark:border-gray-800/60 p-3"
            >
              <dt class="text-xs text-gray-500 dark:text-gray-400">{{ t(`syn.stats_surface_${surface}`) }}</dt>
              <dd class="mt-0.5 text-lg font-semibold tabular-nums text-text dark:text-text-dark">
                {{ num(period.by_surface[surface] ?? 0) }}
                <span class="text-xs font-normal text-gray-500 dark:text-gray-400">{{ pct(period.by_surface[surface] ?? 0, period.runs) }}</span>
              </dd>
              <dd class="mt-1 text-xs text-gray-500 dark:text-gray-400">{{ t('syn.stats_surface_why') }}</dd>
            </div>
          </dl>
        </section>

        <!-- ── Rounds ────────────────────────────────────── -->
        <section class="mt-8" aria-labelledby="stats-rounds">
          <h3 id="stats-rounds" class="text-sm font-medium text-text dark:text-text-dark">{{ t('syn.stats_rounds_title') }}</h3>
          <p class="mt-0.5 text-xs text-gray-500 dark:text-gray-400">{{ t('syn.stats_rounds_why') }}</p>
          <table class="mt-2 w-full text-sm">
            <caption class="sr-only">{{ t('syn.stats_rounds_title') }}</caption>
            <thead>
              <tr class="text-left text-xs text-gray-500 dark:text-gray-400 border-b border-gray-100 dark:border-gray-800/60">
                <th scope="col" class="py-1.5 font-medium w-40">{{ t('syn.stats_col_rounds') }}</th>
                <th scope="col" class="py-1.5 font-medium text-right w-16">{{ t('syn.stats_col_runs') }}</th>
                <th scope="col" class="py-1.5 font-medium text-right w-16">{{ t('syn.stats_col_share') }}</th>
                <th scope="col" class="py-1.5"><span class="sr-only">{{ t('syn.stats_col_bar') }}</span></th>
              </tr>
            </thead>
            <tbody>
              <tr v-for="row in roundRows" :key="row.key" class="border-b border-gray-50 dark:border-gray-800/40">
                <th scope="row" class="py-1.5 text-left font-normal text-text dark:text-text-dark">{{ t(`syn.stats_rounds_${row.key}`) }}</th>
                <td class="py-1.5 text-right tabular-nums text-gray-600 dark:text-gray-300">{{ num(row.n) }}</td>
                <td class="py-1.5 text-right tabular-nums text-gray-500 dark:text-gray-400">{{ pct(row.n, period.runs) }}</td>
                <td class="py-1.5 pl-3">
                  <div class="h-1.5 rounded-full bg-gray-100 dark:bg-gray-800 overflow-hidden" aria-hidden="true">
                    <div class="h-full rounded-full bg-accent/70 dark:bg-accent-dark/70" :style="{ width: `${row.width}%` }" />
                  </div>
                </td>
              </tr>
            </tbody>
          </table>
        </section>

        <!-- ── Endings, and which ceiling ─────────────────── -->
        <section class="mt-8" aria-labelledby="stats-ended">
          <h3 id="stats-ended" class="text-sm font-medium text-text dark:text-text-dark">{{ t('syn.stats_ended_title') }}</h3>
          <p class="mt-0.5 text-xs text-gray-500 dark:text-gray-400">{{ t('syn.stats_ended_why') }}</p>
          <table class="mt-2 w-full text-sm">
            <caption class="sr-only">{{ t('syn.stats_ended_title') }}</caption>
            <thead>
              <tr class="text-left text-xs text-gray-500 dark:text-gray-400 border-b border-gray-100 dark:border-gray-800/60">
                <th scope="col" class="py-1.5 font-medium w-40">{{ t('syn.stats_col_ending') }}</th>
                <th scope="col" class="py-1.5 font-medium text-right w-16">{{ t('syn.stats_col_runs') }}</th>
                <th scope="col" class="py-1.5 font-medium text-right w-16">{{ t('syn.stats_col_share') }}</th>
                <th scope="col" class="py-1.5"><span class="sr-only">{{ t('syn.stats_col_bar') }}</span></th>
              </tr>
            </thead>
            <tbody>
              <tr v-for="row in endingRows" :key="row.key" class="border-b border-gray-50 dark:border-gray-800/40">
                <th scope="row" class="py-1.5 text-left font-normal text-text dark:text-text-dark">{{ t(`syn.run_state_${row.key}`) }}</th>
                <td class="py-1.5 text-right tabular-nums text-gray-600 dark:text-gray-300">{{ num(row.n) }}</td>
                <td class="py-1.5 text-right tabular-nums text-gray-500 dark:text-gray-400">{{ pct(row.n, period.runs) }}</td>
                <td class="py-1.5 pl-3">
                  <div class="h-1.5 rounded-full bg-gray-100 dark:bg-gray-800 overflow-hidden" aria-hidden="true">
                    <div
                      class="h-full rounded-full"
                      :class="row.key === 'done' ? 'bg-emerald-400' : row.key === 'failed' ? 'bg-red-400' : 'bg-amber-400'"
                      :style="{ width: `${row.width}%` }"
                    />
                  </div>
                </td>
              </tr>
            </tbody>
          </table>

          <p v-if="period.asked" class="mt-2 text-xs text-gray-500 dark:text-gray-400">
            {{ t('syn.stats_asked', { asked: num(period.asked), per100: num(Math.round((period.asked / Math.max(1, period.runs)) * 100)), carried: num(period.carried_on ?? 0) }) }}
          </p>

          <h4 class="mt-5 text-xs font-medium text-text dark:text-text-dark">{{ t('syn.stats_ceiling_title') }}</h4>
          <p class="mt-0.5 text-xs text-gray-500 dark:text-gray-400">{{ t('syn.stats_ceiling_why') }}</p>
          <p v-if="!stoppedByCeiling" class="mt-2 text-xs text-gray-500 dark:text-gray-400">{{ t('syn.stats_ceiling_none') }}</p>
          <table v-else class="mt-2 w-full max-w-sm text-sm">
            <caption class="sr-only">{{ t('syn.stats_ceiling_title') }}</caption>
            <thead>
              <tr class="text-left text-xs text-gray-500 dark:text-gray-400 border-b border-gray-100 dark:border-gray-800/60">
                <th scope="col" class="py-1.5 font-medium">{{ t('syn.stats_col_ceiling') }}</th>
                <th scope="col" class="py-1.5 font-medium text-right">{{ t('syn.stats_col_runs') }}</th>
                <th scope="col" class="py-1.5 font-medium text-right">{{ t('syn.stats_col_share_of_all') }}</th>
              </tr>
            </thead>
            <tbody>
              <tr v-for="row in ceilingRows" :key="row.key" class="border-b border-gray-50 dark:border-gray-800/40">
                <th scope="row" class="py-1.5 text-left font-normal text-text dark:text-text-dark">{{ t(`syn.stats_ceiling_${row.key}`) }}</th>
                <td class="py-1.5 text-right tabular-nums text-gray-600 dark:text-gray-300">{{ num(row.n) }}</td>
                <td class="py-1.5 text-right tabular-nums text-gray-500 dark:text-gray-400">{{ pct(row.n, period.runs) }}</td>
              </tr>
            </tbody>
          </table>
        </section>

        <!-- ── Syn's own tools ───────────────────────────── -->
        <section class="mt-8" aria-labelledby="stats-tools">
          <h3 id="stats-tools" class="text-sm font-medium text-text dark:text-text-dark">{{ t('syn.stats_tools_title') }}</h3>
          <p class="mt-0.5 text-xs text-gray-500 dark:text-gray-400">{{ t('syn.stats_tools_why') }}</p>
          <table class="mt-2 w-full text-sm">
            <caption class="sr-only">{{ t('syn.stats_tools_title') }}</caption>
            <thead>
              <tr class="text-left text-xs text-gray-500 dark:text-gray-400 border-b border-gray-100 dark:border-gray-800/60">
                <th scope="col" class="py-1.5 font-medium">{{ t('syn.stats_col_tool') }}</th>
                <th scope="col" class="py-1.5 font-medium text-right w-16">{{ t('syn.stats_col_calls') }}</th>
                <th scope="col" class="py-1.5 font-medium text-right w-24">{{ t('syn.stats_col_runs_used') }}</th>
                <th scope="col" class="py-1.5 w-32"><span class="sr-only">{{ t('syn.stats_col_bar') }}</span></th>
              </tr>
            </thead>
            <tbody>
              <tr v-for="row in toolRows" :key="row.tool" class="border-b border-gray-50 dark:border-gray-800/40 align-top">
                <th scope="row" class="py-2 pr-3 text-left font-normal">
                  <span class="font-mono text-xs text-text dark:text-text-dark">{{ row.tool }}</span>
                  <span class="block mt-0.5 text-xs text-gray-500 dark:text-gray-400">{{ t(`syn.stats_tool_${row.tool}`) }}</span>
                </th>
                <td
                  class="py-2 text-right tabular-nums"
                  :class="row.calls === 0 ? 'text-amber-600 dark:text-amber-500' : 'text-gray-600 dark:text-gray-300'"
                >{{ num(row.calls) }}</td>
                <td class="py-2 text-right tabular-nums text-gray-500 dark:text-gray-400">{{ num(row.runs) }}</td>
                <td class="py-2 pl-3 pt-3.5">
                  <div class="h-1.5 rounded-full bg-gray-100 dark:bg-gray-800 overflow-hidden" aria-hidden="true">
                    <div class="h-full rounded-full bg-accent/70 dark:bg-accent-dark/70" :style="{ width: `${row.width}%` }" />
                  </div>
                </td>
              </tr>
            </tbody>
          </table>
        </section>

        <!-- ── What the prompt carried ───────────────────── -->
        <section class="mt-8" aria-labelledby="stats-prompt">
          <h3 id="stats-prompt" class="text-sm font-medium text-text dark:text-text-dark">{{ t('syn.stats_prompt_title') }}</h3>
          <p class="mt-0.5 text-xs text-gray-500 dark:text-gray-400">
            {{ period.memory.measured
              ? t('syn.stats_measured', { n: period.memory.measured, total: period.runs })
              : t('syn.stats_not_measured') }}
          </p>
          <dl v-if="period.memory.measured" class="mt-3 space-y-3">
            <div>
              <dt class="text-xs text-gray-500 dark:text-gray-400">{{ t('syn.stats_memory_share') }}</dt>
              <dd class="text-sm tabular-nums text-text dark:text-text-dark">
                {{ pct(period.memory.with_memory, period.memory.measured) }}
                <span class="text-xs text-gray-500 dark:text-gray-400">({{ t('syn.stats_of', { n: period.memory.with_memory, total: period.memory.measured }) }})</span>
              </dd>
              <dd class="text-xs text-gray-500 dark:text-gray-400">{{ t('syn.stats_memory_share_why') }}</dd>
            </div>
            <div>
              <dt class="text-xs text-gray-500 dark:text-gray-400">{{ t('syn.stats_memory_avg') }}</dt>
              <dd class="text-sm tabular-nums text-text dark:text-text-dark">{{ num(period.memory.avg_lines) }}</dd>
            </div>
            <div>
              <dt class="text-xs text-gray-500 dark:text-gray-400">{{ t('syn.stats_memory_dropped') }}</dt>
              <dd class="text-sm tabular-nums text-text dark:text-text-dark">
                {{ pct(period.memory.lines_dropped, period.memory.lines_sent + period.memory.lines_dropped) }}
                <span class="text-xs text-gray-500 dark:text-gray-400">({{ t('syn.stats_of', { n: period.memory.lines_dropped, total: period.memory.lines_sent + period.memory.lines_dropped }) }})</span>
              </dd>
              <dd class="text-xs text-gray-500 dark:text-gray-400">{{ t('syn.stats_memory_dropped_why') }}</dd>
            </div>
            <div>
              <dt class="text-xs text-gray-500 dark:text-gray-400">{{ t('syn.stats_prompt_dropped') }}</dt>
              <dd class="text-sm tabular-nums text-text dark:text-text-dark">
                {{ pct(period.prompt.any_dropped, period.prompt.measured) }}
                <span class="text-xs text-gray-500 dark:text-gray-400">({{ t('syn.stats_of', { n: period.prompt.any_dropped, total: period.prompt.measured }) }})</span>
              </dd>
              <dd v-if="droppedSections.length" class="text-xs text-gray-500 dark:text-gray-400">
                <span v-for="([kind, n], i) in droppedSections" :key="kind">{{ i ? ' · ' : '' }}{{ t(`syn.stats_section_${kind}`) }}: {{ num(n) }}</span>
              </dd>
              <dd class="text-xs text-gray-500 dark:text-gray-400">{{ t('syn.stats_prompt_dropped_why') }}</dd>
            </div>
            <div>
              <dt class="text-xs text-gray-500 dark:text-gray-400">{{ t('syn.stats_skills_offered') }}</dt>
              <dd class="text-sm tabular-nums text-text dark:text-text-dark">
                {{ pct(period.skills.offered, period.skills.measured) }}
                <span class="text-xs text-gray-500 dark:text-gray-400">({{ t('syn.stats_of', { n: period.skills.offered, total: period.skills.measured }) }})</span>
              </dd>
              <dd class="text-xs text-gray-500 dark:text-gray-400">{{ t('syn.stats_skills_offered_why') }}</dd>
            </div>
          </dl>
          <dl class="mt-3 space-y-3">
            <div>
              <dt class="text-xs text-gray-500 dark:text-gray-400">{{ t('syn.stats_skills_loaded') }}</dt>
              <dd class="text-sm tabular-nums text-text dark:text-text-dark">
                {{ num(period.skills.loaded) }}
                <span class="text-xs text-gray-500 dark:text-gray-400">{{ pct(period.skills.loaded, period.runs) }}</span>
              </dd>
              <dd v-if="period.skills.usage.length" class="text-xs text-gray-500 dark:text-gray-400">
                <span v-for="(skill, i) in period.skills.usage" :key="skill.name">{{ i ? ' · ' : '' }}{{ skill.name }} ({{ num(skill.runs) }})</span>
              </dd>
              <dd class="text-xs text-gray-500 dark:text-gray-400">{{ t('syn.stats_skills_loaded_why') }}</dd>
            </div>
            <div>
              <dt class="text-xs text-gray-500 dark:text-gray-400">{{ t('syn.stats_skills_injected') }}</dt>
              <dd class="text-sm tabular-nums text-text dark:text-text-dark">
                {{ num(period.skills.injected) }}
                <span class="text-xs text-gray-500 dark:text-gray-400">{{ pct(period.skills.injected, period.runs) }}</span>
              </dd>
              <dd class="text-xs text-gray-500 dark:text-gray-400">{{ t('syn.stats_skills_injected_why') }}</dd>
            </div>
            <div>
              <dt class="text-xs text-gray-500 dark:text-gray-400">{{ t('syn.stats_retrieval') }}</dt>
              <dd class="text-sm tabular-nums text-text dark:text-text-dark">
                <template v-if="period.retrieval.measured">
                  {{ t('syn.stats_retrieval_value', { avg: num(period.retrieval.avg_ms), max: num(period.retrieval.max_ms) }) }}
                </template>
                <template v-else>{{ t('syn.stats_not_measured') }}</template>
              </dd>
              <dd class="text-xs text-gray-500 dark:text-gray-400">{{ t('syn.stats_retrieval_why') }}</dd>
            </div>
          </dl>
        </section>

        <!-- ── What answers stood on ─────────────────────── -->
        <section class="mt-8" aria-labelledby="stats-footing">
          <h3 id="stats-footing" class="text-sm font-medium text-text dark:text-text-dark">{{ t('syn.stats_footing_title') }}</h3>
          <p class="mt-0.5 text-xs text-gray-500 dark:text-gray-400">{{ t('syn.stats_footing_why') }}</p>
          <dl class="mt-2 grid grid-cols-2 sm:grid-cols-4 gap-3">
            <div v-for="kind in (['grounded', 'inferred', 'guessing'] as const)" :key="kind" class="rounded-xl border border-gray-100 dark:border-gray-800/60 p-3">
              <dt class="text-xs text-gray-500 dark:text-gray-400">{{ t(`syn.footing_${kind}`) }}</dt>
              <dd class="mt-0.5 text-lg font-semibold tabular-nums text-text dark:text-text-dark">
                {{ num(period.footing[kind]) }}
                <span class="text-xs font-normal text-gray-500 dark:text-gray-400">{{ pct(period.footing[kind], period.footing.measured) }}</span>
              </dd>
            </div>
            <div class="rounded-xl border border-gray-100 dark:border-gray-800/60 p-3">
              <dt class="text-xs text-gray-500 dark:text-gray-400">{{ t('syn.stats_footing_unmeasured') }}</dt>
              <dd class="mt-0.5 text-lg font-semibold tabular-nums text-gray-500 dark:text-gray-400">{{ num(period.footing.unmeasured) }}</dd>
            </div>
          </dl>
        </section>
      </template>

      <!-- ── Tokens by day ───────────────────────────────── -->
      <section class="mt-8" aria-labelledby="stats-tokens">
        <h3 id="stats-tokens" class="text-sm font-medium text-text dark:text-text-dark">{{ t('syn.stats_tokens_title', { n: stats.recent_days }) }}</h3>
        <p class="mt-0.5 text-xs text-gray-500 dark:text-gray-400">{{ t('syn.stats_tokens_why') }}</p>
        <p class="mt-2 text-sm text-text dark:text-text-dark">
          {{ t('syn.stats_tokens_total', { tokens: num(period.tokens), cached: num(period.tokens_cached) }) }}
        </p>
        <table class="mt-2 w-full text-sm">
          <caption class="sr-only">{{ t('syn.stats_tokens_title', { n: stats.recent_days }) }}</caption>
          <thead>
            <tr class="text-left text-xs text-gray-500 dark:text-gray-400 border-b border-gray-100 dark:border-gray-800/60">
              <th scope="col" class="py-1.5 font-medium w-28">{{ t('syn.stats_col_day') }}</th>
              <th scope="col" class="py-1.5 font-medium text-right w-12">{{ t('syn.stats_col_runs') }}</th>
              <th scope="col" class="py-1.5 font-medium text-right w-24">{{ t('syn.stats_col_tokens') }}</th>
              <th scope="col" class="py-1.5 font-medium text-right w-24">{{ t('syn.stats_col_cached') }}</th>
              <th scope="col" class="py-1.5"><span class="sr-only">{{ t('syn.stats_col_bar') }}</span></th>
            </tr>
          </thead>
          <tbody>
            <tr
              v-for="day in dayRows"
              :key="day.day"
              class="border-b border-gray-50 dark:border-gray-800/40"
              :class="day.runs ? '' : 'text-gray-500 dark:text-gray-400'"
            >
              <th scope="row" class="py-1 text-left font-normal font-mono text-xs">{{ day.day }}</th>
              <td class="py-1 text-right tabular-nums">{{ num(day.runs) }}</td>
              <td class="py-1 text-right tabular-nums">{{ num(day.tokens) }}</td>
              <td class="py-1 text-right tabular-nums">{{ num(day.tokens_cached) }}</td>
              <td class="py-1 pl-3">
                <div class="h-1.5 rounded-full bg-gray-100 dark:bg-gray-800 overflow-hidden" aria-hidden="true">
                  <div class="h-full rounded-full bg-sky-400" :style="{ width: `${day.width}%` }" />
                </div>
              </td>
            </tr>
          </tbody>
        </table>
      </section>
    </template>
  </div>
</template>
