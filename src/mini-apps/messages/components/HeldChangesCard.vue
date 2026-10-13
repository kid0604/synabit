<script setup lang="ts">
import { computed, ref, watch } from 'vue';
import { useI18n } from 'vue-i18n';
import { FilePen } from 'lucide-vue-next';
import { useSynChanges, type ChangeOutcome, type ChangesEvent, type HeldChange } from '../composables/useSynChanges';
import SynCard from '../../../shared/syn/SynCard.vue';

/**
 * Changes Syn held after reading something from outside, waiting for the
 * person — one block per run in this conversation.
 *
 * Every word about what a change does is Rust's, from the call and the vault
 * as it is: the note's title, a field now and after, the body now and after.
 * Nothing the model wrote says what a change is. What the run had read is
 * named, because that is why it is waiting. See `syn::changes`.
 *
 * Owns its own state, so Messages and the AskBar mount the same card the same
 * way.
 */
const props = defineProps<{ vaultPath: string; conversationId: string | null | undefined }>();
const { t, te } = useI18n();
const { heldIn, answer, error, answering } = useSynChanges(() => props.vaultPath);

const runs = computed<ChangesEvent[]>(() => heldIn(props.conversationId));

const held = (c: HeldChange) => c.state === 'held';
/** What is ticked, by run. Everything held starts ticked. */
const chosen = ref<Record<string, Set<string>>>({});
watch(
  runs,
  (all) => {
    const next: Record<string, Set<string>> = {};
    for (const r of all) next[r.run_id] = chosen.value[r.run_id] ?? new Set(r.changes.changes.filter(held).map((c) => c.hash));
    chosen.value = next;
  },
  { immediate: true },
);

const toggle = (runId: string, hash: string, on: boolean) => {
  const next = new Set(chosen.value[runId] ?? []);
  if (on) next.add(hash);
  else next.delete(hash);
  chosen.value = { ...chosen.value, [runId]: next };
};

/** What the last answer came to, shown until the conversation changes. */
const results = ref<ChangeOutcome[] | null>(null);
watch(() => props.conversationId, () => { results.value = null; });

const decide = async (runId: string, approved: string[]) => {
  const out = await answer(runId, approved);
  if (out) results.value = out;
};

const tally = computed(() => {
  const r = results.value ?? [];
  return {
    applied: r.filter((o) => o.state === 'applied').length,
    failed: r.filter((o) => o.state === 'failed').length,
    declined: r.filter((o) => o.state === 'declined').length,
    errors: r.filter((o) => o.state === 'failed' && o.error).map((o) => o.error as string),
  };
});

/** A transaction's fields are the app's, and are named in the person's
 *  language; a note's are the person's own frontmatter keys, shown as written. */
const fieldName = (c: HeldChange, name: string) =>
  c.kind.startsWith('transaction_') && te(`syn.changes_field_${name}`) ? t(`syn.changes_field_${name}`) : name;

/** Why a change was not made: a note that changed meanwhile is said in the
 *  person's language; anything else is the tool's own words. */
const CHANGED_SINCE = 'changed_since:';
const why = (e: string) =>
  e.startsWith(CHANGED_SINCE) ? t('syn.changes_changed_since', { what: e.slice(CHANGED_SINCE.length) }) : e;

const shown = (v: string | null | undefined) => (v === null || v === undefined || v === '' ? t('syn.changes_empty') : v);
</script>

<template>
  <div v-if="runs.length || results" class="space-y-2" data-test="held-changes">
    <SynCard
      v-for="r in runs"
      :key="r.run_id"
      tone="change"
      :icon="FilePen"
      :title="t('syn.changes_title', { n: r.changes.changes.length }, r.changes.changes.length)"
      :lead="r.changes.because.length
        ? t('syn.changes_because', { what: r.changes.because.join(', ') })
        : t('syn.changes_because_unknown')"
      :error="error"
      :data-test="`held-${r.run_id.slice(0, 8)}`"
    >

      <ul class="space-y-2">
        <li
          v-for="c in r.changes.changes"
          :key="c.hash"
          class="p-2.5 rounded-lg bg-white/70 dark:bg-white/5 border border-gray-200 dark:border-gray-700/50 space-y-1.5"
          :data-test="`change-${c.hash.slice(0, 8)}`"
        >
          <label class="flex items-start gap-2 cursor-pointer">
            <input
              type="checkbox"
              class="mt-0.5 accent-amber-600"
              :disabled="!held(c) || answering"
              :checked="chosen[r.run_id]?.has(c.hash)"
              @change="toggle(r.run_id, c.hash, ($event.target as HTMLInputElement).checked)"
            />
            <span class="min-w-0 flex-1 text-xs">
              <span class="font-medium text-text dark:text-text-dark">{{ t(`syn.changes_kind_${c.kind}`) }}</span>
              <span v-if="c.titles.length" class="text-text dark:text-text-dark">: {{ c.titles.join(', ') }}</span>
            </span>
          </label>
          <dl v-if="c.fields.length" class="pl-6 space-y-0.5 text-xs">
            <div v-for="f in c.fields" :key="f.name" class="flex flex-wrap gap-x-2 min-w-0">
              <dt class="text-gray-500 dark:text-gray-400 shrink-0">{{ fieldName(c, f.name) }}</dt>
              <dd class="min-w-0 break-words text-text dark:text-text-dark">
                <template v-if="f.before !== undefined && f.before !== null">
                  <span class="line-through text-gray-500 dark:text-gray-400">{{ f.before }}</span> →
                </template>
                {{ shown(f.after) }}
              </dd>
            </div>
          </dl>
          <details v-if="c.after !== undefined && c.after !== null" class="pl-6 text-xs">
            <summary class="cursor-pointer text-gray-600 dark:text-gray-300">{{ t('syn.changes_content') }}</summary>
            <div class="mt-1 grid gap-2" :class="c.before !== undefined && c.before !== null ? 'sm:grid-cols-2' : ''">
              <div v-if="c.before !== undefined && c.before !== null" class="min-w-0">
                <p class="text-gray-500 dark:text-gray-400 mb-0.5">{{ t('syn.changes_before') }}</p>
                <pre class="whitespace-pre-wrap break-words max-h-48 overflow-auto p-2 rounded bg-gray-50 dark:bg-white/5 text-text dark:text-text-dark">{{ shown(c.before) }}</pre>
              </div>
              <div class="min-w-0">
                <p class="text-gray-500 dark:text-gray-400 mb-0.5">{{ t('syn.changes_after') }}</p>
                <pre class="whitespace-pre-wrap break-words max-h-48 overflow-auto p-2 rounded bg-gray-50 dark:bg-white/5 text-text dark:text-text-dark">{{ shown(c.after) }}</pre>
              </div>
            </div>
          </details>
        </li>
      </ul>

      <template #actions>
        <button
          type="button"
          class="px-3 py-1.5 rounded-lg text-sm font-medium cursor-pointer border border-gray-200 dark:border-gray-700/50 text-text dark:text-text-dark hover:bg-gray-50 dark:hover:bg-white/5 disabled:opacity-40"
          :disabled="answering"
          data-test="changes-decline"
          @click="decide(r.run_id, [])"
        >
          {{ t('syn.changes_decline') }}
        </button>
        <button
          type="button"
          class="btn-primary"
          :disabled="answering || !(chosen[r.run_id]?.size)"
          data-test="changes-apply"
          @click="decide(r.run_id, [...(chosen[r.run_id] ?? [])])"
        >
          {{ t('syn.changes_apply', { n: chosen[r.run_id]?.size ?? 0 }, chosen[r.run_id]?.size ?? 0) }}
        </button>
      </template>
    </SynCard>

    <div v-if="results && !runs.length" class="text-xs text-gray-600 dark:text-gray-300 px-1" data-test="changes-result">
      <p>{{ t('syn.changes_result', tally) }}</p>
      <p v-for="(e, i) in tally.errors" :key="i" class="text-red-600 dark:text-red-400">{{ t('syn.changes_failed', { why: why(e) }) }}</p>
    </div>
  </div>
</template>
