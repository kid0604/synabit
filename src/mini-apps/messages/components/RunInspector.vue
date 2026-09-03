<script setup lang="ts">
/**
 * What Syn did, and what it was told before it did it.
 *
 * Two tabs, because they are the two halves of the same question. A run's
 * transcript answers "what happened"; the prompt preview answers "what did it
 * know going in". Debugging an assistant means holding both, and until this
 * panel neither was reachable: the first was `log::info!` on the user's own
 * machine, and the second was a string built in Rust that nobody had ever seen.
 *
 * A panel rather than a screen, and a wide one, because a transcript is read
 * beside the conversation it came from rather than instead of it.
 */
import { ref, computed, onMounted, onUnmounted } from 'vue';
import { useI18n } from 'vue-i18n';
import {
  X, RefreshCw, Loader2, Trash2, Square, Wrench, MessageSquare,
  Info, AlertTriangle, ChevronRight,
} from 'lucide-vue-next';
import { useSynRuns } from '../composables/useSynRuns';
import type { RunState, RunStep, Reversal } from '../types';

const props = defineProps<{ vaultPath: string }>();
const emit = defineEmits<{ close: [] }>();

const { t } = useI18n();

const {
  runs, selected, preview, isLoading, error,
  loadRuns, openRun, cancelRun, deleteRun, loadPreview,
} = useSynRuns(() => props.vaultPath);

type Tab = 'runs' | 'prompt';
const tab = ref<Tab>('runs');

/** The question the prompt preview is built for. Optional, and worth giving. */
const previewQuestion = ref('');

const showTab = async (next: Tab) => {
  tab.value = next;
  if (next === 'prompt' && !preview.value) await loadPreview(previewQuestion.value);
};

/**
 * A colour per state, and one that means "nothing is driving this".
 *
 * `interrupted` deliberately does not look like a failure: the app was closed
 * mid-run, which is a thing people do on purpose.
 */
const stateStyle = (state: RunState) => ({
  working: 'bg-blue-500 animate-pulse',
  done: 'bg-emerald-500',
  failed: 'bg-red-500',
  cancelled: 'bg-gray-400',
  budget_exhausted: 'bg-amber-500',
  interrupted: 'bg-gray-400',
}[state] ?? 'bg-gray-400');

const stateLabel = (state: RunState) => t(`syn.run_state_${state}`);

const when = (iso: string) => {
  const date = new Date(iso);
  return Number.isNaN(date.getTime()) ? iso : date.toLocaleString();
};

const duration = (ms: number) => (ms < 1000 ? `${ms}ms` : `${(ms / 1000).toFixed(1)}s`);

/** `12 / 50` — spent against the ceiling, or just spent when there is none. */
const against = (spent: number, cap: number | null) =>
  cap === null ? String(spent) : `${spent} / ${cap}`;

const stepIcon = (kind: RunStep['kind']) =>
  ({ tool_call: Wrench, assistant: MessageSquare, note: Info }[kind] ?? Info);

/** The one line somebody reads when they want to know if they can undo it. */
const reversalText = (reversal?: Reversal) => {
  if (!reversal) return null;
  if (reversal.kind === 'nothing') return t('syn.run_changed_nothing');
  if (reversal.kind === 'irreversible') return t('syn.run_irreversible');
  return reversal.how;
};

const prettyArgs = (args?: Record<string, unknown>) => {
  if (!args || Object.keys(args).length === 0) return '';
  try {
    return JSON.stringify(args, null, 2);
  } catch {
    return String(args);
  }
};

/** How full the prompt is against its budget, for the bar. */
const budgetUsed = computed(() => {
  if (!preview.value || preview.value.budget_chars === 0) return 0;
  return Math.min(100, Math.round((preview.value.chars / preview.value.budget_chars) * 100));
});

const onKeydown = (e: KeyboardEvent) => {
  if (e.key === 'Escape') emit('close');
};

onMounted(() => {
  loadRuns();
  window.addEventListener('keydown', onKeydown);
});
onUnmounted(() => window.removeEventListener('keydown', onKeydown));
</script>

<template>
  <Teleport to="body">
    <div class="fixed inset-0 z-[998] bg-black/40 backdrop-blur-sm" @click="emit('close')" />

    <div
      class="fixed right-0 top-0 bottom-0 z-[999] w-[860px] max-w-full flex flex-col
             bg-white dark:bg-[#13141a] border-l border-gray-200 dark:border-gray-800/60
             shadow-2xl shadow-black/20"
    >
      <!-- Header -->
      <div class="flex items-center justify-between px-6 py-4 border-b border-gray-100 dark:border-gray-800/60">
        <div class="flex items-center gap-4">
          <h2 class="text-lg font-semibold text-text dark:text-text-dark">{{ t('syn.inspector') }}</h2>
          <div class="flex gap-1 p-0.5 rounded-lg bg-gray-100 dark:bg-gray-800/60">
            <button
              v-for="option in (['runs', 'prompt'] as Tab[])"
              :key="option"
              class="px-3 py-1 text-xs font-medium rounded-md transition-colors"
              :class="tab === option
                ? 'bg-white dark:bg-gray-700 text-text dark:text-text-dark shadow-sm'
                : 'text-gray-500 hover:text-text dark:hover:text-text-dark'"
              @click="showTab(option)"
            >
              {{ t(`syn.inspector_tab_${option}`) }}
            </button>
          </div>
        </div>
        <div class="flex items-center gap-1">
          <button
            class="p-2 rounded-lg text-gray-500 hover:bg-gray-100 dark:hover:bg-gray-800"
            :title="t('syn.refresh')"
            @click="tab === 'runs' ? loadRuns() : loadPreview(previewQuestion)"
          >
            <Loader2 v-if="isLoading" class="w-4 h-4 animate-spin" />
            <RefreshCw v-else class="w-4 h-4" />
          </button>
          <button
            class="p-2 rounded-lg text-gray-500 hover:bg-gray-100 dark:hover:bg-gray-800"
            @click="emit('close')"
          >
            <X class="w-4 h-4" />
          </button>
        </div>
      </div>

      <div v-if="error" class="mx-6 mt-4 px-3 py-2 rounded-lg bg-red-50 dark:bg-red-950/40 text-sm text-red-700 dark:text-red-300">
        {{ error }}
      </div>

      <!-- ── Runs ─────────────────────────────────────────── -->
      <div v-if="tab === 'runs'" class="flex-1 flex min-h-0">
        <!-- The list -->
        <div class="w-[280px] shrink-0 border-r border-gray-100 dark:border-gray-800/60 overflow-y-auto">
          <p v-if="!runs.length && !isLoading" class="p-6 text-sm text-gray-500">
            {{ t('syn.runs_empty') }}
          </p>
          <button
            v-for="run in runs"
            :key="run.id"
            class="w-full text-left px-4 py-3 border-b border-gray-50 dark:border-gray-800/40
                   hover:bg-gray-50 dark:hover:bg-gray-800/40 transition-colors"
            :class="selected?.id === run.id ? 'bg-violet-50 dark:bg-violet-950/30' : ''"
            @click="openRun(run.id)"
          >
            <div class="flex items-center gap-2 mb-1">
              <span class="w-2 h-2 rounded-full shrink-0" :class="stateStyle(run.state)" />
              <span class="text-[11px] uppercase tracking-wide text-gray-500">{{ stateLabel(run.state) }}</span>
              <ChevronRight class="w-3 h-3 ml-auto text-gray-400" />
            </div>
            <p class="text-sm text-text dark:text-text-dark line-clamp-2">{{ run.goal }}</p>
            <p class="mt-1 text-[11px] text-gray-400">
              {{ when(run.created_at) }} · {{ t('syn.runs_tool_calls', { n: run.tool_calls }) }}
            </p>
          </button>
        </div>

        <!-- The transcript -->
        <div class="flex-1 overflow-y-auto">
          <p v-if="!selected" class="p-6 text-sm text-gray-500">{{ t('syn.runs_pick_one') }}</p>

          <div v-else class="p-6">
            <p class="text-base font-medium text-text dark:text-text-dark">{{ selected.goal }}</p>

            <div class="mt-3 flex flex-wrap gap-x-6 gap-y-1 text-xs text-gray-500">
              <span class="flex items-center gap-1.5">
                <span class="w-2 h-2 rounded-full" :class="stateStyle(selected.state)" />
                {{ stateLabel(selected.state) }}
              </span>
              <span v-if="selected.model">{{ selected.model }}</span>
              <span>{{ t('syn.run_rounds') }}: {{ against(selected.spent.iterations, selected.budget.iterations) }}</span>
              <span>{{ t('syn.run_tools') }}: {{ against(selected.spent.tool_calls, selected.budget.tool_calls) }}</span>
              <span>{{ duration(selected.spent.wall_ms) }}</span>
            </div>

            <p v-if="selected.error" class="mt-3 px-3 py-2 rounded-lg bg-red-50 dark:bg-red-950/40 text-sm text-red-700 dark:text-red-300">
              {{ selected.error }}
            </p>

            <div class="mt-4 flex gap-2">
              <button
                v-if="selected.state === 'working'"
                class="inline-flex items-center gap-1.5 px-3 py-1.5 text-xs rounded-lg
                       bg-gray-100 dark:bg-gray-800 hover:bg-gray-200 dark:hover:bg-gray-700"
                @click="cancelRun(selected.id)"
              >
                <Square class="w-3 h-3" /> {{ t('syn.stop') }}
              </button>
              <button
                class="inline-flex items-center gap-1.5 px-3 py-1.5 text-xs rounded-lg text-red-600
                       hover:bg-red-50 dark:hover:bg-red-950/40"
                @click="deleteRun(selected.id)"
              >
                <Trash2 class="w-3 h-3" /> {{ t('syn.delete') }}
              </button>
            </div>

            <!-- Steps -->
            <ol class="mt-6 space-y-3">
              <li
                v-for="step in selected.steps"
                :key="step.index"
                class="rounded-xl border border-gray-100 dark:border-gray-800/60 p-3"
                :class="step.ok === false ? 'border-red-200 dark:border-red-900/60' : ''"
              >
                <div class="flex items-center gap-2 text-xs">
                  <component
                    :is="step.ok === false ? AlertTriangle : stepIcon(step.kind)"
                    class="w-3.5 h-3.5 shrink-0"
                    :class="step.ok === false ? 'text-red-500' : 'text-gray-400'"
                  />
                  <span class="font-medium text-text dark:text-text-dark">
                    {{ step.tool ?? t(`syn.run_step_${step.kind}`) }}
                  </span>
                  <span class="text-gray-400">{{ t('syn.run_round_n', { n: step.iteration + 1 }) }}</span>
                  <span class="ml-auto text-gray-400">{{ duration(step.ms) }}</span>
                </div>

                <details v-if="prettyArgs(step.args)" class="mt-2">
                  <summary class="text-[11px] text-gray-500 cursor-pointer select-none">
                    {{ t('syn.run_arguments') }}
                  </summary>
                  <pre class="mt-1 p-2 rounded-lg bg-gray-50 dark:bg-gray-900/60 text-[11px]
                              text-gray-600 dark:text-gray-300 overflow-x-auto">{{ prettyArgs(step.args) }}</pre>
                </details>

                <pre
                  v-if="step.preview"
                  class="mt-2 p-2 rounded-lg bg-gray-50 dark:bg-gray-900/60 text-[11px]
                         text-gray-600 dark:text-gray-300 max-h-56 overflow-auto whitespace-pre-wrap"
                >{{ step.preview }}</pre>

                <p v-if="reversalText(step.reversal)" class="mt-2 text-[11px] text-gray-400">
                  {{ t('syn.run_undo') }}: {{ reversalText(step.reversal) }}
                </p>
              </li>
            </ol>
          </div>
        </div>
      </div>

      <!-- ── Prompt ───────────────────────────────────────── -->
      <div v-else class="flex-1 overflow-y-auto p-6">
        <p class="text-sm text-gray-500">{{ t('syn.prompt_explainer') }}</p>

        <div class="mt-3 flex gap-2">
          <input
            v-model="previewQuestion"
            class="flex-1 px-3 py-2 text-sm rounded-lg border border-gray-200 dark:border-gray-700
                   bg-white dark:bg-gray-900 text-text dark:text-text-dark"
            :placeholder="t('syn.prompt_question_placeholder')"
            @keydown.enter="loadPreview(previewQuestion)"
          />
          <button
            class="px-3 py-2 text-sm rounded-lg bg-violet-600 text-white hover:bg-violet-700"
            @click="loadPreview(previewQuestion)"
          >
            {{ t('syn.refresh') }}
          </button>
        </div>

        <template v-if="preview">
          <div class="mt-6">
            <div class="flex items-baseline justify-between text-sm">
              <span class="text-text dark:text-text-dark font-medium">
                {{ t('syn.prompt_total', { chars: preview.chars, tokens: preview.est_tokens }) }}
              </span>
              <span class="text-xs text-gray-400">
                {{ t('syn.prompt_budget', { chars: preview.budget_chars }) }}
              </span>
            </div>
            <div class="mt-2 h-1.5 rounded-full bg-gray-100 dark:bg-gray-800 overflow-hidden">
              <div class="h-full bg-violet-500 rounded-full" :style="{ width: `${budgetUsed}%` }" />
            </div>
            <p class="mt-1 text-[11px] text-gray-400">{{ t('syn.prompt_tokens_estimated') }}</p>
          </div>

          <table class="mt-5 w-full text-sm">
            <thead>
              <tr class="text-left text-xs text-gray-500 border-b border-gray-100 dark:border-gray-800/60">
                <th class="py-2 font-medium">{{ t('syn.prompt_section') }}</th>
                <th class="py-2 font-medium text-right">{{ t('syn.prompt_chars') }}</th>
                <th class="py-2 font-medium text-right">{{ t('syn.prompt_est_tokens') }}</th>
              </tr>
            </thead>
            <tbody>
              <tr
                v-for="section in preview.sections"
                :key="section.kind"
                class="border-b border-gray-50 dark:border-gray-800/40"
                :class="section.dropped ? 'opacity-50' : ''"
              >
                <td class="py-2 text-text dark:text-text-dark">
                  {{ section.label }}
                  <span v-if="section.dropped" class="ml-2 text-[11px] text-amber-600">
                    {{ t('syn.prompt_dropped') }}
                  </span>
                </td>
                <td class="py-2 text-right text-gray-500 tabular-nums">{{ section.chars }}</td>
                <td class="py-2 text-right text-gray-500 tabular-nums">{{ section.est_tokens }}</td>
              </tr>
            </tbody>
          </table>

          <p class="mt-6 mb-2 text-xs font-medium text-gray-500 uppercase tracking-wide">
            {{ t('syn.prompt_verbatim') }}
          </p>
          <pre class="p-3 rounded-xl bg-gray-50 dark:bg-gray-900/60 text-[11px]
                      text-gray-700 dark:text-gray-300 whitespace-pre-wrap">{{ preview.text }}</pre>
        </template>
      </div>
    </div>
  </Teleport>
</template>
