<script setup lang="ts">
/**
 * What a rehearsal would change in a folder on this Mac, waiting for the
 * person (design §9).
 *
 * Every word about what a change does is Rust's: the op, and the names as they
 * are on disk, made safe to show (A21). Documents are chosen beforehand; what
 * is not plainly a document is not, and choosing one has Rust ask again in the
 * system's own dialog, which nothing on this page can answer (A22). What cannot
 * be applied as it is is shown and cannot be chosen.
 *
 * Owns its state, like `HeldChangesCard`, so Messages and the AskBar mount it
 * the same way.
 */
import { computed, onBeforeUnmount, onMounted, ref, watch } from 'vue';
import { invoke } from '@tauri-apps/api/core';
import { listen, type UnlistenFn } from '@tauri-apps/api/event';
import { useI18n } from 'vue-i18n';
import { FolderSync } from 'lucide-vue-next';
import SynCard from '../../../shared/syn/SynCard.vue';
import { logger } from '../../../utils/logger';
import { errorText } from '../../../shared/errorText';

type What = 'rename' | 'move' | 'trash' | 'new' | 'change' | 'new_folder' | 'remove_folder';
type Class = 'data' | 'unknown' | 'blocked';

interface Op {
  n: number;
  what: What;
  from: string | null;
  to: string | null;
  size: number;
  class: Class;
  why: string | null;
}

interface Proposal {
  id: string;
  door_name: string;
  root: string;
  made: string;
  unchanged: number;
  read_outside: boolean;
  ops: Op[];
}

interface Applied {
  confirmed: boolean;
  outcome: { journal: string; done: number[]; skipped: [number, string][] } | null;
}

const props = defineProps<{ vaultPath: string; conversationId: string | null | undefined }>();
const { t, locale } = useI18n();

const proposals = ref<Proposal[]>([]);
const chosen = ref<Record<string, Set<number>>>({});
const busy = ref(false);
const error = ref<string | null>(null);
const said = ref<string[]>([]);

const load = async () => {
  if (!props.vaultPath || !props.conversationId) {
    proposals.value = [];
    return;
  }
  try {
    proposals.value = await invoke<Proposal[]>('syn_waiting_rehearsals', {
      vaultPath: props.vaultPath,
      conversationId: props.conversationId,
    });
  } catch (e) {
    logger.warn('[Syn] Could not read the waiting rehearsals', e);
  }
};

watch(proposals, (all) => {
  const next: Record<string, Set<number>> = {};
  for (const p of all) next[p.id] = chosen.value[p.id] ?? new Set(p.ops.filter((o) => o.class === 'data').map((o) => o.n));
  chosen.value = next;
});
watch(() => props.conversationId, () => {
  said.value = [];
  void load();
});

let unlisten: UnlistenFn | undefined;
onMounted(async () => {
  void load();
  unlisten = await listen<{ conversation_id: string }>('syn-rehearsal-needed', (e) => {
    if (e.payload?.conversation_id === props.conversationId) void load();
  }).catch(() => undefined);
});
onBeforeUnmount(() => unlisten?.());

const toggle = (id: string, n: number, on: boolean) => {
  const next = new Set(chosen.value[id] ?? []);
  if (on) next.add(n);
  else next.delete(n);
  chosen.value = { ...chosen.value, [id]: next };
};

const count = (p: Proposal, ...whats: What[]) => p.ops.filter((o) => whats.includes(o.what) && o.class !== 'blocked').length;
const unknown = (p: Proposal) => p.ops.filter((o) => o.class === 'unknown');
const blocked = (p: Proposal) => p.ops.filter((o) => o.class === 'blocked');
const name = (o: Op) => o.to ?? o.from ?? '';
const largest = (p: Proposal) =>
  p.ops.filter((o) => (o.what === 'move' || o.what === 'rename') && o.class !== 'blocked').sort((a, b) => b.size - a.size)[0];

const line = (o: Op) => {
  if (o.from && o.to) return `${o.from} → ${o.to}`;
  return o.from ?? o.to ?? '';
};

const summary = computed(() => (p: Proposal) =>
  t('syn.rehearsal_counts', {
    moved: count(p, 'move'),
    renamed: count(p, 'rename'),
    trashed: count(p, 'trash', 'remove_folder'),
    made: count(p, 'new', 'change'),
    folders: count(p, 'new_folder'),
    unchanged: p.unchanged,
  }),
);

const decide = async (p: Proposal, apply: boolean) => {
  busy.value = true;
  error.value = null;
  try {
    if (!apply) {
      await invoke('syn_decline_rehearsal', { vaultPath: props.vaultPath, id: p.id });
      said.value = [t('syn.rehearsal_declined')];
    } else {
      const approved = [...(chosen.value[p.id] ?? [])];
      const result = await invoke<Applied>('syn_apply_rehearsal', {
        vaultPath: props.vaultPath,
        id: p.id,
        approved,
        locale: locale.value,
      });
      if (!result.confirmed || !result.outcome) {
        said.value = [t('syn.rehearsal_cancelled')];
        return;
      }
      const skipped = result.outcome.skipped;
      said.value = [
        t('syn.rehearsal_done', { done: result.outcome.done.length, skipped: skipped.length }),
        ...skipped.map(([n, why]) => {
          const op = p.ops.find((o) => o.n === n);
          return t('syn.rehearsal_skipped_item', { name: op ? name(op) : String(n), why });
        }),
      ];
    }
    await load();
  } catch (e) {
    error.value = errorText(e);
  } finally {
    busy.value = false;
  }
};
</script>

<template>
  <div v-if="proposals.length || said.length" class="space-y-2" data-test="rehearsals">
    <SynCard
      v-for="p in proposals"
      :key="p.id"
      tone="change"
      :icon="FolderSync"
      :title="t('syn.rehearsal_title', { name: p.door_name })"
      :error="error"
      :data-test="`rehearsal-${p.id}`"
    >
      <template #lead>
        <p class="text-xs text-gray-600 dark:text-gray-300">{{ summary(p) }}</p>
        <p v-if="unknown(p).length" class="text-xs text-amber-800 dark:text-amber-200" data-test="rehearsal-unknown">
          {{ t('syn.rehearsal_unknown', { n: unknown(p).length, names: unknown(p).slice(0, 4).map(name).join(', ') }, unknown(p).length) }}
        </p>
        <p v-if="blocked(p).length" class="text-xs text-gray-600 dark:text-gray-300">
          {{ t('syn.rehearsal_blocked', { n: blocked(p).length }, blocked(p).length) }}
        </p>
        <p v-if="largest(p)" class="text-xs text-gray-600 dark:text-gray-300 break-words">
          {{ t('syn.rehearsal_largest', { what: line(largest(p)) }) }}
        </p>
        <p v-if="p.read_outside" class="text-xs text-gray-600 dark:text-gray-300">{{ t('syn.rehearsal_read') }}</p>
      </template>

      <details class="text-xs">
        <summary class="cursor-pointer text-gray-600 dark:text-gray-300">{{ t('syn.rehearsal_list', { n: p.ops.length }) }}</summary>
        <ul class="mt-2 space-y-1 max-h-80 overflow-auto">
          <li v-for="o in p.ops" :key="o.n" :data-test="`op-${o.n}`">
            <label class="flex items-start gap-2" :class="o.class === 'blocked' ? 'opacity-60' : 'cursor-pointer'">
              <input
                type="checkbox"
                class="mt-0.5 accent-amber-600"
                :disabled="o.class === 'blocked' || busy"
                :checked="chosen[p.id]?.has(o.n)"
                @change="toggle(p.id, o.n, ($event.target as HTMLInputElement).checked)"
              />
              <span class="min-w-0 flex-1">
                <span class="font-medium text-text dark:text-text-dark">{{ t(`syn.rehearsal_what_${o.what}`) }}</span>
                <span class="text-text dark:text-text-dark break-all"> {{ line(o) }}</span>
                <span v-if="o.why" class="block text-gray-500 dark:text-gray-400">{{ t(`syn.rehearsal_why_${o.why}`) }}</span>
              </span>
            </label>
          </li>
        </ul>
      </details>
      <p class="text-xs text-gray-500 dark:text-gray-400">{{ t('syn.rehearsal_note') }}</p>

      <template #actions>
        <button
          type="button"
          class="px-3 py-1.5 rounded-lg text-sm font-medium cursor-pointer border border-gray-200 dark:border-gray-700/50 text-text dark:text-text-dark hover:bg-gray-50 dark:hover:bg-white/5 disabled:opacity-40"
          :disabled="busy"
          data-test="rehearsal-decline"
          @click="decide(p, false)"
        >
          {{ t('syn.rehearsal_decline') }}
        </button>
        <button
          type="button"
          class="btn-primary"
          :disabled="busy || !chosen[p.id]?.size"
          data-test="rehearsal-apply"
          @click="decide(p, true)"
        >
          {{ t('syn.rehearsal_apply', { n: chosen[p.id]?.size ?? 0 }, chosen[p.id]?.size ?? 0) }}
        </button>
      </template>
    </SynCard>

    <div v-if="said.length" class="text-xs text-gray-600 dark:text-gray-300 px-1 space-y-0.5" data-test="rehearsal-result" role="status">
      <p v-for="(s, i) in said" :key="i" class="break-words">{{ s }}</p>
    </div>
  </div>
</template>
