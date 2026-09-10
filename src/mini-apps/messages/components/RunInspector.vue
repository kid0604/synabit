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
import { ref, computed, nextTick, onMounted, onUnmounted } from 'vue';
import { useI18n } from 'vue-i18n';
import { useRoute } from 'vue-router';
import {
  X, RefreshCw, Loader2, Trash2, Square, Wrench, MessageSquare,
  Info, AlertTriangle, ChevronRight, Pin, PinOff, Check, Sparkles, X as XIcon,
} from 'lucide-vue-next';
import { invoke } from '@tauri-apps/api/core';
import { logger } from '../../../utils/logger';
import { useSynRuns } from '../composables/useSynRuns';
import { useSynMemory, isStale, orderMemories } from '../composables/useSynMemory';
import { useSynSkills, mayBeEnabled } from '../composables/useSynSkills';
import { useSynAudit, hasLapsed } from '../composables/useSynAudit';
import { captureFocus } from '../../../shared/syn/focus';
import type { RunState, RunStep, Reversal, Memory, Skill, ToolCard, Run, Capability } from '../types';
import { capabilityLabel } from '../composables/useSynConsent';

const props = defineProps<{
  vaultPath: string;
  /**
   * Which tab to open on, when something sent the reader here.
   *
   * `null` for the ordinary case — somebody pressed the inspector button and
   * wants the runs.
   */
  initialTab?: 'memory' | 'skills' | null;
  /**
   * The id to scroll to and ring, when one was named.
   *
   * A notice saying *"I am holding two contradictory things about you"* is only
   * half an act if it lands the reader in a list of forty and leaves them to
   * find the two. See `syn::notice`.
   */
  highlight?: string | null;
}>();
const emit = defineEmits<{ close: []; use: [name: string] }>();

const { t } = useI18n();
const route = useRoute();

const {
  runs, selected, preview, isLoading, error,
  loadRuns, openRun, cancelRun, deleteRun, loadPreview,
} = useSynRuns(() => props.vaultPath);

type Tab = 'runs' | 'prompt' | 'tools' | 'memory' | 'skills' | 'permissions';
const tab = ref<Tab>('runs');

const {
  memories, proposals, budget, error: memoryError,
  load: loadMemories, setPinned, confirm: confirmMemory, forget, accept, dismiss,
} = useSynMemory(() => props.vaultPath);

const {
  ordered: orderedSkills, error: skillError, trials, trialling, recipeProblems,
  load: loadSkills, setEnabled, usageOf, trial, create: createSkill, decideRevision,
  save: saveSkill,
} = useSynSkills(() => props.vaultPath);

const {
  entries: auditEntries, ordered: orderedGrants, error: auditError,
  load: loadAudit, revoke: revokeGrant,
} = useSynAudit(() => props.vaultPath);

/**
 * The skill open for editing, and the draft of it.
 *
 * One at a time. A screen that lets two be edited at once has to decide what
 * happens to the other when one is saved, and the honest answer is nothing
 * good.
 */
const editingSkill = ref<string | null>(null);
const draft = ref({ description: '', when_to_use: '', tier: 'prose' as Skill['tier'], body: '' });

const beginEdit = (skill: Skill) => {
  editingSkill.value = skill.id;
  draft.value = {
    description: skill.description,
    when_to_use: skill.when_to_use,
    tier: skill.tier,
    body: skill.body,
  };
};

const commitEdit = async (skill: Skill) => {
  if (await saveSkill(skill, draft.value)) editingSkill.value = null;
};

/** The name of a skill being started. Empty when the row is closed. */
const newSkillName = ref('');
const startSkill = async () => {
  const name = newSkillName.value.trim();
  if (!name) return;
  if (await createSkill(name)) newSkillName.value = '';
};

/** How full the pinned budget is, for the bar on the memory tab. */
const memoryUsed = computed(() => {
  if (!budget.value || budget.value.budget_chars === 0) return 0;
  return Math.min(100, Math.round((budget.value.chars / budget.value.budget_chars) * 100));
});

const confidenceLabel = (memory: Memory) => `${Math.round(memory.confidence * 100)}%`;

/** Both live with the data they order, so nothing here is a second opinion. */
const orderedMemories = computed(() => orderMemories(memories.value));

/**
 * Open the conversation a memory came out of.
 *
 * "Why do you believe this about me?" is the question this screen most needs to
 * answer, and every memory has carried `source_run` since it was written.
 */
const showTheRunBehind = async (memory: Memory) => {
  if (!memory.source_run) return;
  tab.value = 'runs';
  if (!runs.value.length) await loadRuns();
  await openRun(memory.source_run);
};

/**
 * Everything Syn can reach, and what each one costs to undo.
 *
 * Its own tab rather than a block at the top of Permissions. That tab is about
 * decisions — what has been granted, what was done with it — and it says so in
 * its own explainer: *what Syn is allowed to do outside the vault*. The
 * catalogue is not a decision and most of it never leaves the vault, so it
 * would have been the largest thing on a screen that explicitly excludes it.
 */
const tools = ref<ToolCard[]>([]);
const toolError = ref<string | null>(null);

/**
 * Always re-read, never cached.
 *
 * It used to return early when the list was already loaded, which was fine for
 * a catalogue that could not change. It can now: a switch writes to the consent
 * ledger, and the usage tally moves every time Syn calls anything. A screen
 * showing yesterday's answer to "is this on" is worse than no screen.
 */
const loadTools = async () => {
  try {
    tools.value = await invoke<ToolCard[]>('syn_list_tools', { vaultPath: props.vaultPath });
  } catch (e) {
    toolError.value = (e as { message?: string })?.message ?? String(e);
  }
};

/**
 * Turn a whole kind of power on or off.
 *
 * The unit is the group, not the tool. Twenty-nine switches is twenty-nine
 * decisions, and the groups are already the words the consent card uses — so
 * one switch here answers a question somebody can hold in their head.
 *
 * The capability goes back exactly as it arrived. Composing a scope string in
 * TypeScript would be a second copy of `Capability::scope_key`, and the first
 * thing a second copy does is drift.
 *
 * Re-read afterwards rather than assumed: the answer to "is it off now" is the
 * ledger's, and this asks it rather than guessing.
 */
const switching = ref<string | null>(null);
const setCapability = async (capability: Capability, allowed: boolean, key: string) => {
  switching.value = key;
  try {
    await invoke('syn_set_capability', {
      vaultPath: props.vaultPath,
      capability,
      allowed,
    });
    await loadTools();
    // The Permissions tab is the other view of this one record. Left stale, it
    // would show a switch that this screen says is off and that one does not.
    await loadAudit();
  } catch (e) {
    toolError.value = (e as { message?: string })?.message ?? String(e);
  } finally {
    switching.value = null;
  }
};

/**
 * Grouped by what they need, in order of how much they can change.
 *
 * The shape of the list is the answer to "what can it reach": fourteen tools
 * that only read, nine that change one note, four that change many files at
 * once. A flat alphabetical list of twenty-seven hides exactly that.
 */
const CAPABILITY_ORDER = ['VaultRead', 'VaultWrite', 'VaultStructural'];

const toolGroups = computed(() => {
  const by = new Map<string, {
    label: ReturnType<typeof capabilityLabel> | null;
    capability: Capability | null;
    tools: ToolCard[];
  }>();
  for (const tool of tools.value) {
    const cap = tool.capability ?? null;
    const key = cap === null ? '' : typeof cap === 'string' ? cap : Object.keys(cap)[0];
    if (!by.has(key)) {
      by.set(key, {
        label: cap === null ? null : capabilityLabel(cap),
        capability: cap,
        tools: [],
      });
    }
    by.get(key)!.tools.push(tool);
  }
  return [...by.entries()]
    .sort(([a], [b]) => {
      // Unclassified first: it is a bug and should not be buried.
      if (!a) return -1;
      if (!b) return 1;
      return CAPABILITY_ORDER.indexOf(a) - CAPABILITY_ORDER.indexOf(b);
    })
    .map(([key, group]) => ({
      key,
      ...group,
      // Off, not partly off. A capability is one row in the ledger, so every
      // tool under it moves together — and if that ever stopped being true the
      // header would be lying rather than merely wrong.
      on: group.tools.some(tool => tool.offered),
      // What this group costs the payload, in the units the Prompt tab uses.
      chars: group.tools.reduce((sum, tool) => sum + tool.chars, 0),
      never: group.tools.filter(tool => !tool.used).length,
    }));
});

/** What the whole list costs right now — the figure a switch moves. */
const toolChars = computed(() =>
  tools.value.filter(tool => tool.offered).reduce((sum, tool) => sum + tool.chars, 0),
);

/**
 * Which descriptions are open.
 *
 * Clamped by default: these are written for the model, and `query_nodes` alone
 * is a paragraph. Twenty-seven paragraphs is a page nobody reads, which would
 * defeat the point of having built the screen.
 */
const openTools = ref(new Set<string>());
const toggleTool = (name: string) => {
  const next = new Set(openTools.value);
  if (!next.delete(name)) next.add(name);
  openTools.value = next;
};

/** The question the prompt preview is built for. Optional, and worth giving. */
const previewQuestion = ref('');

/**
 * The preview, built against the screen as it is right now.
 *
 * Without the focus, this panel would be the one place in the app where the
 * on-screen section is invisible — and it is the panel whose entire job is to
 * say what Syn is told. A section nobody can see here is a section nobody can
 * debug when it misfires.
 *
 * The screen is this one, honestly: the panel is open over Messages, so that
 * is what it reports. Select some text in a message and refresh to watch the
 * section appear and the breakdown charge for it.
 */
const showPrompt = (question: string) =>
  loadPreview(question, captureFocus({ app: (route.name as string) ?? 'messages' }));

/**
 * Land on the tab that was asked for, and put the named thing in front of the
 * reader.
 *
 * The scroll waits for the list to have loaded *and* rendered — the tab's data
 * is fetched on show, so an element addressed before that is an element that
 * does not exist yet, and the whole point would be silently lost.
 */
const goTo = async (next: Tab, id: string | null) => {
  await showTab(next);
  if (!id) return;
  await nextTick();
  document
    .getElementById(`syn-item-${cssId(id)}`)
    ?.scrollIntoView({ block: 'center', behavior: 'smooth' });
};

/**
 * An id safe to put in `id=""`.
 *
 * These are vault paths — `SynMemory/1a2b.md` — with slashes and dots in them,
 * which `getElementById` handles fine but which would break the moment anybody
 * reached for a CSS selector instead.
 */
const cssId = (id: string) => id.replace(/[^A-Za-z0-9_-]/g, '-');

const showTab = async (next: Tab) => {
  tab.value = next;
  if (next === 'prompt' && !preview.value) await showPrompt(previewQuestion.value);
  if (next === 'tools') await loadTools();
  if (next === 'memory') await loadMemories();
  if (next === 'skills') await loadSkills();
  if (next === 'permissions') await loadAudit();
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
  awaiting_consent: 'bg-violet-500',
  // Waiting on a person, not on permission — see `syn::ambiguity`.
  awaiting_choice: 'bg-violet-400',
  interrupted: 'bg-gray-400',
}[state] ?? 'bg-gray-400');

const stateLabel = (state: RunState) => t(`syn.run_state_${state}`);

const when = (iso: string) => {
  const date = new Date(iso);
  return Number.isNaN(date.getTime()) ? iso : date.toLocaleString();
};

const duration = (ms: number) => (ms < 1000 ? `${ms}ms` : `${(ms / 1000).toFixed(1)}s`);

/**
 * How much of a result the run file keeps. Mirrors `run::MAX_STEP_PREVIEW`.
 *
 * Only used to decide whether there is more to fetch — a preview at exactly the
 * cap is one that was cut.
 */
const PREVIEW_CAP = 4000;

/** The whole results fetched so far, by step index. */
const whole = ref<Record<number, string>>({});

/**
 * Fetch what a step actually returned.
 *
 * One at a time and only when asked: the whole results live beside the run
 * rather than inside it, because `list_runs` parses every run file and a page
 * read in every step would make opening the list slow for everybody.
 */
const showWhole = async (step: number) => {
  if (!selected.value) return;
  try {
    const text = await invoke<string | null>('syn_run_result', {
      vaultPath: props.vaultPath,
      runId: selected.value.id,
      step,
    });
    if (text) whole.value[step] = text;
  } catch (e) {
    logger.error('[Syn] Could not read what that step returned', e);
  }
};

/**
 * Where a run's tokens went, for the tooltip on the total.
 *
 * Summed across steps rather than stored: the run keeps one total, because a
 * ceiling needs one number to compare against, and the breakdown belongs to the
 * steps that were charged. Silent on a run recorded before any of it was
 * counted — an empty tooltip is honest, a row of zeros is not.
 */
const tokenBreakdown = (run: Run): string => {
  const sum = (pick: (u: NonNullable<RunStep['usage']>) => number | undefined) =>
    run.steps.reduce((n, s) => n + (s.usage ? pick(s.usage) ?? 0 : 0), 0);

  const input = sum(u => u.input);
  const output = sum(u => u.output);
  const cached = sum(u => u.input_cached);
  const hidden = sum(u => u.output_hidden);
  if (input + output === 0) return '';

  const parts = [`${input} in`, `${output} out`];
  if (cached > 0) parts.splice(1, 0, `${cached} of it cached`);
  if (hidden > 0) parts.push(`${hidden} reasoning`);
  return parts.join(' · ');
};

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

/** The same, for the tool declarations. */
const toolsUsed = computed(() => {
  const tools = preview.value?.tools;
  if (!tools || tools.budget_chars === 0) return 0;
  return Math.min(100, Math.round((tools.chars / tools.budget_chars) * 100));
});

/**
 * Prompt plus tools — what one turn costs before anybody has said anything.
 *
 * The number this screen existed to give and did not: the tool declarations
 * are the `tools` field of the request rather than part of the prompt text, so
 * every figure above was exactly right while the page as a whole understated a
 * turn by more than the entire fixed prompt.
 */
const totalTokens = computed(() =>
  preview.value ? preview.value.est_tokens + preview.value.tools.est_tokens : 0
);

/**
 * Ollama's default `num_ctx`.
 *
 * Named here because it is the number that decides whether any of this matters:
 * against a hosted model with a large window the totals above are a cost, and
 * against this one they are a wall. The warning is about the default a local
 * install actually gets, not about the setting this vault happens to have — a
 * user who raised `num_ctx` has already thought about it.
 */
const SMALL_WINDOW = 8192;
const overWindow = computed(() => totalTokens.value > SMALL_WINDOW * 0.6);

const onKeydown = (e: KeyboardEvent) => {
  if (e.key === 'Escape') emit('close');
};

onMounted(() => {
  // Sent here by a notice, or opened by hand. The first case skips the runs
  // entirely — loading them would be work nobody asked for on the way to a
  // memory.
  if (props.initialTab) {
    void goTo(props.initialTab, props.highlight ?? null);
  } else {
    loadRuns();
  }
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
              v-for="option in (['runs', 'prompt', 'tools', 'memory', 'skills', 'permissions'] as Tab[])"
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
            @click="tab === 'runs' ? loadRuns() : tab === 'tools' ? loadTools() : tab === 'memory' ? loadMemories() : tab === 'skills' ? loadSkills() : tab === 'permissions' ? loadAudit() : showPrompt(previewQuestion)"
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

      <div v-if="error || memoryError || skillError || auditError || toolError" class="mx-6 mt-4 px-3 py-2 rounded-lg bg-red-50 dark:bg-red-950/40 text-sm text-red-700 dark:text-red-300">
        {{ error || memoryError || skillError || auditError || toolError }}
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
              <!--
                Tokens, which were never shown because they were never counted:
                the reply's tokens alone, and on a streamed OpenAI-compatible
                request not even those, so every run said zero. The number is
                the whole turn now — input is most of what a run costs, being
                the prompt, the tool declarations, the conversation and every
                page read into them, re-sent on every round.
              -->
              <span v-if="selected.spent.tokens > 0" :title="tokenBreakdown(selected)">
                {{ t('syn.run_tokens') }}: {{ against(selected.spent.tokens, selected.budget.tokens) }}
              </span>
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

                <!--
                  What the model was actually given.

                  The preview stops at four thousand characters and a page slice
                  is twenty-four thousand, so most of the largest thing in a turn
                  was not written down anywhere. Every extraction bug found on
                  9 and 10 September was invisible until somebody fetched the
                  page by hand and re-ran the extraction on it.
                -->
                <pre
                  v-if="step.preview"
                  class="mt-2 p-2 rounded-lg bg-gray-50 dark:bg-gray-900/60 text-[11px]
                         text-gray-600 dark:text-gray-300 max-h-56 overflow-auto whitespace-pre-wrap"
                >{{ whole[step.index] ?? step.preview }}</pre>
                <button
                  v-if="step.preview.length >= PREVIEW_CAP && whole[step.index] === undefined"
                  class="mt-1 text-[11px] text-violet-500 hover:underline cursor-pointer"
                  @click="showWhole(step.index)"
                >
                  {{ t('syn.run_show_whole') }}
                </button>

                <p v-if="reversalText(step.reversal)" class="mt-2 text-[11px] text-gray-400">
                  {{ t('syn.run_undo') }}: {{ reversalText(step.reversal) }}
                </p>
              </li>
            </ol>
          </div>
        </div>
      </div>

      <!-- ── Skills ───────────────────────────────────────── -->
      <!-- ── Tools ────────────────────────────────────────
           What Syn can reach at all. The catalogue existed from the first
           day and was read in one place in the whole codebase, to validate
           recipe step names; nothing ever showed it to anybody. -->
      <div v-else-if="tab === 'tools'" class="flex-1 overflow-y-auto p-6">
        <p class="text-sm text-gray-500">{{ t('syn.tools_explainer') }}</p>
        <!-- The count, and what it costs. The second number is the one a
             switch moves, and it is the same figure the Prompt tab shows
             against its budget — `ToolCard.chars` is measured by the same
             `serde_json` call, so the parts add up to that whole. -->
        <p class="mt-1 text-[11px] text-gray-400">
          {{ t('syn.tools_count', { n: tools.length }) }} ·
          {{ t('syn.tools_cost', { chars: toolChars, tokens: Math.round(toolChars / 4) }) }}
        </p>

        <div v-for="group in toolGroups" :key="group.key || 'unclassified'" class="mt-6">
          <div class="mb-2 flex items-center gap-2">
            <h3
              class="text-sm font-medium"
              :class="group.label ? 'text-text dark:text-text-dark' : 'text-amber-600 dark:text-amber-500'"
            >
              {{ group.label ? t(group.label.key, group.label.values) : t('syn.tools_unclassified') }}
              <span class="ml-1.5 text-[11px] font-normal text-gray-400">{{ group.tools.length }}</span>
            </h3>

            <!-- The switch, on the group and not on the tool.

                 Twenty-nine switches is twenty-nine decisions; four is one you
                 can hold in your head. And the group is what the consent ledger
                 can actually record — a `Never` is filed per capability, so a
                 per-tool switch would need a scope the ledger has no word for.

                 Off is a `Never` in that same ledger, which the Permissions tab
                 shows and can take back. One record, two views.

                 Only for a capability that is one whole thing. A scoped one —
                 `NetRead { domain }` — arrives here from a catalogue built with
                 no arguments, so its host is the empty string, and a switch on
                 it would file a refusal against nowhere. Those are decided per
                 host, on the card, at the moment the host is known. -->
            <button
              v-if="typeof group.capability === 'string'"
              type="button"
              role="switch"
              :aria-checked="group.on"
              :aria-label="t(group.on ? 'syn.tools_switch_off' : 'syn.tools_switch_on')"
              :disabled="switching === group.key"
              class="ml-auto relative w-9 h-5 shrink-0 rounded-full transition-colors disabled:opacity-50"
              :class="group.on ? 'bg-violet-500' : 'bg-gray-300 dark:bg-gray-700'"
              @click="setCapability(group.capability, !group.on, group.key)"
            >
              <span
                class="absolute top-0.5 w-4 h-4 rounded-full bg-white transition-all"
                :class="group.on ? 'left-[18px]' : 'left-0.5'"
              />
            </button>
          </div>

          <!-- What switching it off actually does, in the same breath as the
               switch. Reading the vault is the one worth spelling out: turning
               it off leaves Syn answering from the conversation alone, which is
               a thing some people want and nobody should discover by accident. -->
          <p class="mb-2 text-[11px] text-gray-400 leading-relaxed max-w-prose">
            <template v-if="group.on">
              {{ t('syn.tools_group_cost', { chars: group.chars, tokens: Math.round(group.chars / 4) }) }}
              <span v-if="group.never" class="text-gray-400">
                · {{ t('syn.tools_group_never', { n: group.never }) }}
              </span>
            </template>
            <span v-else class="text-amber-600 dark:text-amber-500">
              {{ t('syn.tools_group_off') }}
            </span>
          </p>

          <ul class="space-y-2">
            <li
              v-for="tool in group.tools"
              :key="tool.name"
              class="rounded-xl border border-gray-100 dark:border-gray-800/60 p-3 transition-opacity"
              :class="tool.offered ? '' : 'opacity-50'"
            >
              <button
                class="w-full flex items-start gap-2 text-left cursor-pointer"
                @click="toggleTool(tool.name)"
              >
                <ChevronRight
                  class="w-3.5 h-3.5 mt-1 shrink-0 text-gray-400 transition-transform"
                  :class="openTools.has(tool.name) ? 'rotate-90' : ''"
                />
                <span class="min-w-0 flex-1">
                  <span class="flex items-baseline gap-2">
                    <code class="text-[13px] font-mono text-violet-600 dark:text-violet-400">{{ tool.name }}</code>
                    <!-- How often Syn has actually reached for it.

                         This is what turns a list of twenty-nine claims into
                         one decision: most of them have never been called, and
                         until this line nothing said so. It informs and does
                         not decide — `restore_node` is used on the one day
                         somebody needs it, which is why the switch is on the
                         group above and this number is here. -->
                    <span class="ml-auto shrink-0 text-[11px] text-gray-400">
                      <template v-if="tool.used">
                        {{ t('syn.tools_used', { n: tool.used }) }}
                        <span v-if="tool.last_used"> · {{ tool.last_used.slice(0, 10) }}</span>
                      </template>
                      <span v-else class="text-gray-300 dark:text-gray-600">{{ t('syn.tools_used_never') }}</span>
                    </span>
                  </span>
                  <!-- Verbatim, and clamped until asked for: written for the
                       model, and `query_nodes` alone is a paragraph.

                       `block` is in the bound class and not the static one, and
                       that is the whole bug this line once had: `line-clamp-2`
                       works by setting `display: -webkit-box`, and `.block`
                       ships later in the stylesheet, so a static `block`
                       silently won and nothing was ever clamped. The chevron
                       turned, the text did not move, and the control looked
                       broken because it was. -->
                  <span
                    class="mt-1 text-xs text-gray-500 dark:text-gray-400 leading-relaxed"
                    :class="openTools.has(tool.name) ? 'block' : 'line-clamp-2'"
                  >{{ tool.description }}</span>
                </span>
              </button>

              <!-- What puts it back. Derived from the capability in Rust rather
                   than declared twice, so the two can never disagree. -->
              <p class="mt-2 pl-5 text-[11px] text-gray-400">
                <template v-if="tool.reversal?.kind === 'nothing'">
                  {{ t('syn.tools_undo_nothing') }}
                </template>
                <template v-else-if="tool.reversal?.kind === 'irreversible'">
                  <span class="text-amber-600 dark:text-amber-500">{{ t('syn.tools_undo_irreversible') }}</span>
                </template>
                <template v-else-if="tool.reversal && 'how' in tool.reversal">
                  {{ t('syn.tools_undo') }}: {{ tool.reversal.how }}
                </template>
                <!-- Only once the row is open: what it costs every turn. A
                     number on every collapsed row would be twenty-nine numbers
                     nobody asked for. -->
                <span v-if="openTools.has(tool.name)" class="ml-2 text-gray-300 dark:text-gray-600">
                  · {{ t('syn.tools_cost', { chars: tool.chars, tokens: Math.round(tool.chars / 4) }) }}
                </span>
              </p>
            </li>
          </ul>
        </div>

        <p class="mt-6 text-[11px] text-gray-400 leading-relaxed max-w-prose">
          {{ t('syn.tools_verbatim') }}
        </p>
      </div>

      <div v-else-if="tab === 'skills'" class="flex-1 overflow-y-auto p-6">
        <p class="text-sm text-gray-500">{{ t('syn.skills_explainer') }}</p>

        <div class="mt-4 flex gap-2">
          <input
            v-model="newSkillName"
            type="text"
            class="flex-1 px-3 py-1.5 text-sm rounded-lg border border-gray-200 dark:border-gray-700
                   bg-white dark:bg-gray-900 text-text dark:text-text-dark"
            :placeholder="t('syn.skill_new_placeholder')"
            @keyup.enter="startSkill"
          >
          <button
            class="inline-flex items-center gap-1.5 px-3 py-1.5 text-xs rounded-lg
                   bg-gray-100 dark:bg-gray-800 hover:bg-gray-200 dark:hover:bg-gray-700
                   disabled:opacity-50"
            :disabled="!newSkillName.trim()"
            @click="startSkill"
          >
            {{ t('syn.skill_new') }}
          </button>
        </div>

        <p v-if="!orderedSkills.length" class="mt-6 text-sm text-gray-500">
          {{ t('syn.skills_empty') }}
        </p>

        <ul class="mt-5 space-y-3">
          <li
            v-for="skill in orderedSkills"
            :key="skill.id"
            :id="`syn-item-${cssId(skill.id)}`"
            class="rounded-xl border p-3"
            :class="[
              skill.enabled
                ? 'border-violet-200 dark:border-violet-900/60'
                : 'border-gray-100 dark:border-gray-800/60 opacity-70',
              highlight === skill.id ? 'ring-2 ring-violet-400 ring-offset-2 dark:ring-offset-[#13141a]' : '',
            ]"
          >
            <div class="flex items-center gap-2 text-[11px] text-gray-500">
              <span class="px-1.5 py-0.5 rounded bg-gray-100 dark:bg-gray-800">{{ skill.tier }}</span>
              <span
                v-if="skill.author === 'syn'"
                class="px-1.5 py-0.5 rounded bg-amber-100 text-amber-700
                       dark:bg-amber-950/50 dark:text-amber-400"
              >{{ t('syn.skill_by_syn') }}</span>
              <span class="text-gray-400">v{{ skill.version }}</span>
              <span class="ml-auto text-gray-400">
                {{ usageOf(skill)
                  ? t('syn.skill_used', { n: usageOf(skill)!.runs })
                  : t('syn.skill_never_used') }}
              </span>
            </div>

            <p class="mt-2 text-sm font-medium text-text dark:text-text-dark">{{ skill.name }}</p>

            <!-- Said on the card, not only in the paragraph at the top. The
                 first skill anybody writes gets edited, saved, and asked for —
                 and never switched on, because nothing at the point of use says
                 that off means invisible rather than merely idle. -->
            <p v-if="!skill.enabled" class="mt-1 text-[11px] text-gray-500">
              {{ t('syn.skill_is_off') }}
            </p>
            <p
              v-else-if="!skill.description.trim() || !skill.when_to_use.trim()"
              class="mt-1 text-[11px] text-amber-600"
            >
              {{ t('syn.skill_has_no_summary') }}
            </p>
            <p v-if="skill.description" class="mt-0.5 text-sm text-gray-500">{{ skill.description }}</p>
            <p v-if="skill.when_to_use" class="mt-1 text-[11px] text-gray-500 italic">
              {{ t('syn.skill_when') }}: {{ skill.when_to_use }}
            </p>
            <!-- Said here, before it is ever switched on. A recipe that only
                 reports its problems when the model reaches for it fails half
                 way through a job, where the explanation is a tool result. -->
            <div
              v-if="recipeProblems[skill.id]"
              class="mt-2 rounded-lg bg-red-50 dark:bg-red-950/30 px-3 py-2"
            >
              <p class="text-[11px] font-medium text-red-700 dark:text-red-300">
                {{ t('syn.recipe_wont_run') }}
              </p>
              <ul class="mt-1 space-y-0.5">
                <li
                  v-for="(problem, i) in recipeProblems[skill.id]"
                  :key="i"
                  class="text-[11px] text-red-700 dark:text-red-300"
                >
                  {{ problem }}
                </li>
              </ul>
            </div>

            <p v-if="skill.tools.length" class="mt-1 text-[11px] text-gray-400">
              {{ t('syn.skill_tools') }}: {{ skill.tools.join(', ') }}
            </p>

            <!-- Proposed, not applied: the skill is on, so writing the new
                 steps in would change behaviour before anybody read them. -->
            <div
              v-if="skill.pending_revision"
              class="mt-3 rounded-lg border border-amber-200 dark:border-amber-900/60
                     bg-amber-50/50 dark:bg-amber-950/20 p-3"
            >
              <p class="text-[11px] font-medium text-amber-700 dark:text-amber-400">
                {{ t('syn.skill_revision_title') }}
              </p>
              <p v-if="skill.revision_because" class="mt-1 text-[11px] text-gray-600 dark:text-gray-400 italic">
                {{ skill.revision_because }}
              </p>
              <div class="mt-2 grid gap-3 sm:grid-cols-2">
                <div>
                  <p class="text-[11px] font-medium text-gray-500">{{ t('syn.skill_revision_now') }}</p>
                  <p class="mt-1 text-xs whitespace-pre-wrap text-text dark:text-text-dark">{{ skill.body }}</p>
                </div>
                <div>
                  <p class="text-[11px] font-medium text-amber-700 dark:text-amber-400">{{ t('syn.skill_revision_proposed') }}</p>
                  <p class="mt-1 text-xs whitespace-pre-wrap text-text dark:text-text-dark">{{ skill.pending_revision }}</p>
                </div>
              </div>
              <div class="mt-3 flex gap-2">
                <button
                  class="inline-flex items-center gap-1.5 px-2.5 py-1 text-xs rounded-lg
                         bg-violet-600 text-white hover:bg-violet-700"
                  @click="decideRevision(skill, true)"
                >
                  <Check class="w-3 h-3" /> {{ t('syn.skill_revision_accept') }}
                </button>
                <button
                  class="inline-flex items-center gap-1.5 px-2.5 py-1 text-xs rounded-lg
                         bg-white dark:bg-gray-800 hover:bg-gray-100 dark:hover:bg-gray-700"
                  @click="decideRevision(skill, false)"
                >
                  <XIcon class="w-3 h-3" /> {{ t('syn.skill_revision_keep') }}
                </button>
              </div>
            </div>

            <p v-if="!mayBeEnabled(skill)" class="mt-2 text-[11px] text-amber-600">
              {{ t('syn.skill_needs_trial') }}
            </p>

            <!-- Shown, not scored: which answer is better is a judgement about
                 this person's work, and the app has no business making it. -->
            <div v-if="trials[skill.id]" class="mt-3 rounded-lg bg-gray-50 dark:bg-gray-900/60 p-3">
              <p class="text-[11px] text-gray-500">
                {{ t('syn.skill_trial_question') }}: {{ trials[skill.id].question }}
              </p>
              <div class="mt-2 grid gap-3 sm:grid-cols-2">
                <div>
                  <p class="text-[11px] font-medium text-gray-500">{{ t('syn.skill_trial_without') }}</p>
                  <p class="mt-1 text-xs whitespace-pre-wrap text-text dark:text-text-dark">{{ trials[skill.id].without }}</p>
                </div>
                <div>
                  <p class="text-[11px] font-medium text-violet-600">{{ t('syn.skill_trial_with') }}</p>
                  <p class="mt-1 text-xs whitespace-pre-wrap text-text dark:text-text-dark">{{ trials[skill.id].with }}</p>
                </div>
              </div>
            </div>

            <!-- The steps, readable without leaving the screen. The panel says
                 these are files you can edit; saying it and offering no way to
                 read one is worse than not saying it. -->
            <div v-if="editingSkill === skill.id" class="mt-3 space-y-2">
              <input
                v-model="draft.description"
                type="text"
                class="w-full px-3 py-1.5 text-xs rounded-lg border border-gray-200 dark:border-gray-700
                       bg-white dark:bg-gray-900 text-text dark:text-text-dark"
                :placeholder="t('syn.skill_edit_description')"
              >
              <input
                v-model="draft.when_to_use"
                type="text"
                class="w-full px-3 py-1.5 text-xs rounded-lg border border-gray-200 dark:border-gray-700
                       bg-white dark:bg-gray-900 text-text dark:text-text-dark"
                :placeholder="t('syn.skill_edit_when')"
              >
              <select
                v-model="draft.tier"
                class="w-full px-3 py-1.5 text-xs rounded-lg border border-gray-200 dark:border-gray-700
                       bg-white dark:bg-gray-900 text-text dark:text-text-dark"
              >
                <option value="prose">{{ t('syn.skill_tier_prose') }}</option>
                <option value="recipe">{{ t('syn.skill_tier_recipe') }}</option>
              </select>
              <textarea
                v-model="draft.body"
                rows="12"
                class="w-full px-3 py-2 text-xs font-mono rounded-lg border border-gray-200
                       dark:border-gray-700 bg-white dark:bg-gray-900 text-text dark:text-text-dark"
                :placeholder="t('syn.skill_edit_body')"
              />
              <div class="flex gap-2">
                <button
                  class="inline-flex items-center gap-1.5 px-2.5 py-1 text-xs rounded-lg
                         bg-violet-600 text-white hover:bg-violet-700"
                  @click="commitEdit(skill)"
                >
                  <Check class="w-3 h-3" /> {{ t('syn.skill_edit_save') }}
                </button>
                <button
                  class="inline-flex items-center gap-1.5 px-2.5 py-1 text-xs rounded-lg
                         bg-gray-100 dark:bg-gray-800 hover:bg-gray-200 dark:hover:bg-gray-700"
                  @click="editingSkill = null"
                >
                  <XIcon class="w-3 h-3" /> {{ t('syn.skill_edit_cancel') }}
                </button>
              </div>
            </div>

            <pre
              v-else-if="skill.body.trim()"
              class="mt-3 px-3 py-2 text-[11px] rounded-lg bg-gray-50 dark:bg-gray-900/60
                     text-text dark:text-text-dark whitespace-pre-wrap overflow-x-auto"
            >{{ skill.body.trim() }}</pre>

            <div class="mt-3 flex gap-2">
              <button
                v-if="editingSkill !== skill.id"
                class="inline-flex items-center gap-1.5 px-2.5 py-1 text-xs rounded-lg
                       bg-gray-100 dark:bg-gray-800 hover:bg-gray-200 dark:hover:bg-gray-700"
                @click="beginEdit(skill)"
              >
                {{ t('syn.skill_edit') }}
              </button>
              <button
                v-if="!mayBeEnabled(skill)"
                class="inline-flex items-center gap-1.5 px-2.5 py-1 text-xs rounded-lg
                       bg-violet-600 text-white hover:bg-violet-700 disabled:opacity-50"
                :disabled="trialling === skill.id"
                @click="trial(skill)"
              >
                <Sparkles class="w-3 h-3" />
                {{ trialling === skill.id ? t('syn.skill_trial_running') : t('syn.skill_trial') }}
              </button>
              <button
                v-else
                class="inline-flex items-center gap-1.5 px-2.5 py-1 text-xs rounded-lg
                       bg-gray-100 dark:bg-gray-800 hover:bg-gray-200 dark:hover:bg-gray-700"
                @click="setEnabled(skill, !skill.enabled)"
              >
                <component :is="skill.enabled ? PinOff : Pin" class="w-3 h-3" />
                {{ skill.enabled ? t('syn.skill_disable') : t('syn.skill_enable') }}
              </button>
              <button
                v-if="skill.enabled"
                class="inline-flex items-center gap-1.5 px-2.5 py-1 text-xs rounded-lg
                       bg-violet-600 text-white hover:bg-violet-700"
                @click="emit('use', skill.name); emit('close')"
              >
                <Sparkles class="w-3 h-3" /> {{ t('syn.skill_try') }}
              </button>
            </div>
          </li>
        </ul>
      </div>

      <!-- ── Permissions ──────────────────────────────────── -->
      <div v-else-if="tab === 'permissions'" class="flex-1 overflow-y-auto p-6">
        <p class="text-sm text-gray-500">{{ t('syn.permissions_explainer') }}</p>

        <h3 class="mt-6 mb-2 text-sm font-medium text-text dark:text-text-dark">
          {{ t('syn.permissions_granted') }}
        </h3>
        <p v-if="!orderedGrants.length" class="text-sm text-gray-500">
          {{ t('syn.permissions_none') }}
        </p>
        <ul class="space-y-2">
          <li
            v-for="grant in orderedGrants"
            :key="grant.scope"
            class="rounded-xl border border-gray-100 dark:border-gray-800/60 p-3"
          >
            <div class="flex items-center gap-2 text-[11px]">
              <span
                class="px-1.5 py-0.5 rounded"
                :class="grant.answer === 'never'
                  ? 'bg-red-100 text-red-700 dark:bg-red-950/50 dark:text-red-400'
                  : 'bg-gray-100 dark:bg-gray-800 text-gray-500'"
              >{{ t(`syn.consent_${grant.answer}`) }}</span>
              <span v-if="hasLapsed(grant)" class="text-amber-600">{{ t('syn.permission_lapsed') }}</span>
              <span class="ml-auto text-gray-400 font-mono">{{ grant.granted_at.slice(0, 10) }}</span>
            </div>
            <p class="mt-1.5 text-sm text-text dark:text-text-dark">{{ grant.about }}</p>
            <p class="mt-0.5 text-[11px] text-gray-400 font-mono">{{ grant.scope }}</p>
            <button
              class="mt-2 inline-flex items-center gap-1.5 px-2.5 py-1 text-xs rounded-lg
                     bg-gray-100 dark:bg-gray-800 hover:bg-gray-200 dark:hover:bg-gray-700"
              @click="revokeGrant(grant)"
            >
              <XIcon class="w-3 h-3" /> {{ t('syn.permission_revoke') }}
            </button>
          </li>
        </ul>

        <h3 class="mt-8 mb-1 text-sm font-medium text-text dark:text-text-dark">
          {{ t('syn.audit_title') }}
        </h3>
        <p class="text-xs text-gray-500 mb-3">{{ t('syn.audit_explainer') }}</p>
        <p v-if="!auditEntries.length" class="text-sm text-gray-500">
          {{ t('syn.audit_none') }}
        </p>
        <ul class="space-y-1.5">
          <li
            v-for="(entry, i) in auditEntries"
            :key="`${entry.at}-${i}`"
            class="rounded-lg bg-gray-50 dark:bg-gray-900/60 px-3 py-2"
          >
            <div class="flex items-center gap-2 text-[11px]">
              <span
                class="px-1.5 py-0.5 rounded font-mono"
                :class="entry.outcome === 'refused'
                  ? 'bg-red-100 text-red-700 dark:bg-red-950/50 dark:text-red-400'
                  : entry.outcome === 'asked'
                    ? 'bg-amber-100 text-amber-700 dark:bg-amber-950/50 dark:text-amber-400'
                    : 'bg-gray-100 dark:bg-gray-800 text-gray-500'"
              >{{ t(`syn.audit_${entry.outcome}`) }}</span>
              <span class="font-mono text-gray-500">{{ entry.tool }}</span>
              <span class="ml-auto text-gray-400 font-mono">{{ entry.at.slice(0, 16).replace('T', ' ') }}</span>
            </div>
            <p class="mt-1 text-xs text-text dark:text-text-dark">{{ entry.about }}</p>
            <p v-if="entry.reversal" class="mt-0.5 text-[11px] text-gray-500 italic">
              {{ t('syn.audit_undo') }}: {{ entry.reversal }}
            </p>
          </li>
        </ul>
      </div>

      <!-- ── Memory ───────────────────────────────────────── -->
      <div v-else-if="tab === 'memory'" class="flex-1 overflow-y-auto p-6">
        <p class="text-sm text-gray-500">{{ t('syn.memory_explainer') }}</p>

        <div v-if="budget" class="mt-4">
          <div class="flex items-baseline justify-between text-sm">
            <span class="text-text dark:text-text-dark font-medium">
              {{ t('syn.memory_count', { n: budget.total }) }}
            </span>
            <span class="text-xs text-gray-400">
              <span v-if="budget.pinned">{{ t('syn.memory_pinned_count', { n: budget.pinned }) }} · </span>
              {{ t('syn.memory_budget', { used: budget.chars, total: budget.budget_chars }) }}
            </span>
          </div>
          <div class="mt-2 h-1.5 rounded-full bg-gray-100 dark:bg-gray-800 overflow-hidden">
            <div class="h-full rounded-full"
                 :class="budget.dropped > 0 ? 'bg-amber-500' : 'bg-violet-500'"
                 :style="{ width: `${memoryUsed}%` }" />
          </div>
          <p v-if="budget.dropped > 0" class="mt-1 text-[11px] text-amber-600">
            {{ t('syn.memory_dropped', { n: budget.dropped }) }}
          </p>
        </div>

        <!-- Waiting on a decision, so it goes above what is already settled. -->
        <div v-if="proposals.length" class="mt-6">
          <div class="flex items-center gap-2 mb-2">
            <Sparkles class="w-3.5 h-3.5 text-violet-500" />
            <h3 class="text-sm font-medium text-text dark:text-text-dark">
              {{ t('syn.proposals_title', { n: proposals.length }) }}
            </h3>
          </div>
          <p class="text-xs text-gray-500 mb-3">{{ t('syn.proposals_explainer') }}</p>

          <ul class="space-y-2">
            <li
              v-for="proposal in proposals"
              :key="proposal.id"
              class="rounded-xl border border-violet-200 dark:border-violet-900/60
                     bg-violet-50/40 dark:bg-violet-950/20 p-3"
            >
              <div class="flex items-center gap-2 text-[11px] text-gray-500">
                <span class="px-1.5 py-0.5 rounded bg-white dark:bg-gray-800">{{ proposal.kind }}</span>
                <span v-if="proposal.subject" class="text-gray-400">{{ proposal.subject }}</span>
                <span
                  v-if="proposal.from_correction"
                  class="px-1.5 py-0.5 rounded bg-amber-100 text-amber-700
                         dark:bg-amber-950/50 dark:text-amber-400"
                >{{ t('syn.proposal_from_correction') }}</span>
                <span class="ml-auto text-gray-400">{{ Math.round(proposal.confidence * 100) }}%</span>
              </div>
              <p class="mt-2 text-sm text-text dark:text-text-dark">{{ proposal.body }}</p>
              <p v-if="proposal.supersedes" class="mt-1 text-[11px] text-gray-500 line-through">
                {{ t('syn.proposal_replaces', { body: proposal.supersedes }) }}
              </p>
              <p class="mt-1 text-[11px] text-gray-500 italic">{{ proposal.because }}</p>
              <div class="mt-3 flex gap-2">
                <button
                  class="inline-flex items-center gap-1.5 px-2.5 py-1 text-xs rounded-lg
                         bg-violet-600 text-white hover:bg-violet-700"
                  @click="accept(proposal)"
                >
                  <Check class="w-3 h-3" /> {{ t('syn.proposal_accept') }}
                </button>
                <button
                  class="inline-flex items-center gap-1.5 px-2.5 py-1 text-xs rounded-lg
                         bg-white dark:bg-gray-800 hover:bg-gray-100 dark:hover:bg-gray-700"
                  @click="dismiss(proposal)"
                >
                  <XIcon class="w-3 h-3" /> {{ t('syn.proposal_dismiss') }}
                </button>
              </div>
            </li>
          </ul>
        </div>

        <p v-if="!memories.length" class="mt-6 text-sm text-gray-500">
          {{ t('syn.memory_empty') }}
        </p>

        <ul class="mt-5 space-y-3">
          <li
            v-for="memory in orderedMemories"
            :key="memory.id"
            :id="`syn-item-${cssId(memory.id)}`"
            class="rounded-xl border border-gray-100 dark:border-gray-800/60 p-3"
            :class="[
              memory.pinned ? 'border-violet-200 dark:border-violet-900/60' : '',
              highlight === memory.id ? 'ring-2 ring-violet-400 ring-offset-2 dark:ring-offset-[#13141a]' : '',
            ]"
          >
            <div class="flex items-center gap-2 text-[11px] text-gray-500">
              <span class="px-1.5 py-0.5 rounded bg-gray-100 dark:bg-gray-800">{{ memory.kind }}</span>
              <span v-if="memory.subject" class="text-gray-400">{{ memory.subject }}</span>
              <span class="text-gray-400">{{ confidenceLabel(memory) }}</span>
              <span v-if="isStale(memory)" class="text-amber-600">{{ t('syn.memory_stale') }}</span>
              <span class="ml-auto text-gray-400">{{ memory.last_confirmed }}</span>
            </div>

            <p class="mt-2 text-sm text-text dark:text-text-dark whitespace-pre-wrap">{{ memory.body }}</p>

            <p v-if="isStale(memory)" class="mt-2 text-[11px] text-amber-600">
              {{ t('syn.memory_review_prompt') }}
            </p>

            <p v-if="memory.source_nodes.length" class="mt-1 text-[11px] text-gray-400">
              {{ t('syn.memory_from') }}: {{ memory.source_nodes.join(', ') }}
            </p>

            <div class="mt-3 flex gap-2">
              <button
                class="inline-flex items-center gap-1.5 px-2.5 py-1 text-xs rounded-lg
                       bg-gray-100 dark:bg-gray-800 hover:bg-gray-200 dark:hover:bg-gray-700"
                @click="setPinned(memory, !memory.pinned)"
              >
                <component :is="memory.pinned ? PinOff : Pin" class="w-3 h-3" />
                {{ memory.pinned ? t('syn.memory_unpin') : t('syn.memory_pin') }}
              </button>
              <button
                class="inline-flex items-center gap-1.5 px-2.5 py-1 text-xs rounded-lg
                       bg-gray-100 dark:bg-gray-800 hover:bg-gray-200 dark:hover:bg-gray-700"
                @click="confirmMemory(memory)"
              >
                <Check class="w-3 h-3" /> {{ t('syn.memory_confirm') }}
              </button>
              <button
                v-if="memory.source_run"
                class="inline-flex items-center gap-1.5 px-2.5 py-1 text-xs rounded-lg
                       bg-gray-100 dark:bg-gray-800 hover:bg-gray-200 dark:hover:bg-gray-700"
                @click="showTheRunBehind(memory)"
              >
                <Info class="w-3 h-3" /> {{ t('syn.memory_source_run') }}
              </button>
              <button
                class="inline-flex items-center gap-1.5 px-2.5 py-1 text-xs rounded-lg text-red-600
                       hover:bg-red-50 dark:hover:bg-red-950/40"
                @click="forget(memory)"
              >
                <Trash2 class="w-3 h-3" /> {{ t('syn.memory_forget') }}
              </button>
            </div>
          </li>
        </ul>
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
            @keydown.enter="showPrompt(previewQuestion)"
          />
          <button
            class="px-3 py-2 text-sm rounded-lg bg-violet-600 text-white hover:bg-violet-700"
            @click="showPrompt(previewQuestion)"
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

          <!-- The tool declarations, which are not in the prompt text and were
               therefore not on this screen at all. They were the largest single
               thing a turn spends — more than the whole fixed prompt — and the
               panel whose job is to say what a turn costs said nothing about
               them. See `tools::PAYLOAD_BUDGET_CHARS`. -->
          <div class="mt-5">
            <div class="flex items-baseline justify-between text-sm">
              <span class="text-text dark:text-text-dark font-medium">
                {{ t('syn.tools_payload', {
                  n: preview.tools.count,
                  chars: preview.tools.chars,
                  tokens: preview.tools.est_tokens,
                }) }}
              </span>
              <span class="text-xs text-gray-400">
                {{ t('syn.prompt_budget', { chars: preview.tools.budget_chars }) }}
              </span>
            </div>
            <div class="mt-2 h-1.5 rounded-full bg-gray-100 dark:bg-gray-800 overflow-hidden">
              <div class="h-full bg-violet-400 rounded-full" :style="{ width: `${toolsUsed}%` }" />
            </div>
            <p class="mt-1 text-[11px] text-gray-400">{{ t('syn.tools_payload_note') }}</p>
          </div>

          <!-- Neither half is the answer on its own. Against Ollama's default
               8,192-token window the sum is what decides whether the
               conversation fits at all. -->
          <p
            class="mt-4 text-[13px] font-medium"
            :class="overWindow ? 'text-amber-600 dark:text-amber-500' : 'text-text dark:text-text-dark'"
          >
            {{ t('syn.prompt_turn_total', { tokens: totalTokens }) }}
          </p>
          <p v-if="overWindow" class="mt-0.5 text-[11px] text-amber-600 dark:text-amber-500">
            {{ t('syn.prompt_over_window', { window: SMALL_WINDOW }) }}
          </p>

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
