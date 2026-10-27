<script setup lang="ts">
/**
 * Bước 0 of Syn's visuals, run by hand: frame the attack page, show what got
 * out, copy the table for appendix A of `docs/syn-visuals-plan-2026-10-26.md`.
 * Shown only in builds that carry the spike commands.
 */
import { ref } from 'vue';
import { useI18n } from 'vue-i18n';
import { runVisualSpike, asMarkdown, type SpikeReport } from '../../../shared/syn/visualSpike';

const { t } = useI18n();
const stage = ref<HTMLElement | null>(null);
const running = ref(false);
const report = ref<SpikeReport | null>(null);
const failure = ref('');

async function run() {
  if (!stage.value || running.value) return;
  running.value = true;
  failure.value = '';
  report.value = null;
  try {
    report.value = await runVisualSpike(stage.value);
  } catch (error) {
    failure.value = String(error);
  } finally {
    running.value = false;
  }
}

async function copy() {
  if (report.value) await navigator.clipboard.writeText(asMarkdown(report.value));
}
</script>

<template>
  <div class="p-3 rounded-xl border border-gray-200 dark:border-gray-700/50 space-y-2" data-visual-spike>
    <div class="flex items-start justify-between gap-3">
      <span class="min-w-0">
        <span class="block text-sm font-medium text-text dark:text-text-dark">{{ t('syn.visual_spike_title') }}</span>
        <span class="block text-xs text-gray-500 dark:text-gray-400 mt-0.5 leading-relaxed">{{ t('syn.visual_spike_hint') }}</span>
      </span>
      <button
        type="button"
        class="shrink-0 px-3 py-1.5 rounded-lg text-xs border border-gray-200 dark:border-gray-700/50 hover:bg-gray-50 dark:hover:bg-white/5 disabled:opacity-50"
        :disabled="running"
        @click="run"
      >
        {{ running ? t('syn.visual_spike_running') : t('syn.visual_spike_run') }}
      </button>
    </div>
    <div ref="stage" />
    <p v-if="failure" class="text-xs text-danger">{{ failure }}</p>
    <template v-if="report">
      <p class="text-xs" :class="report.leaked ? 'text-danger' : 'text-gray-500 dark:text-gray-400'">
        {{ t('syn.visual_spike_summary', { leaked: report.leaked, unclear: report.unclear, total: report.rows.length }) }}
      </p>
      <table class="w-full text-xs">
        <tr v-for="row in report.rows" :key="row.group + row.name">
          <td class="pr-2 text-gray-500 dark:text-gray-400">{{ row.group }}</td>
          <td class="pr-2 text-text dark:text-text-dark">{{ row.name }}</td>
          <td class="pr-2 font-medium" :class="row.outcome === 'LEAKED' || row.outcome === 'error' ? 'text-danger' : 'text-text dark:text-text-dark'">{{ row.outcome }}</td>
          <td class="text-gray-500 dark:text-gray-400 break-all">{{ row.detail }}</td>
        </tr>
      </table>
      <button type="button" class="px-3 py-1.5 rounded-lg text-xs border border-gray-200 dark:border-gray-700/50 hover:bg-gray-50 dark:hover:bg-white/5" @click="copy">
        {{ t('syn.visual_spike_copy') }}
      </button>
    </template>
  </div>
</template>
