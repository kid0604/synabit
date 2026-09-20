<script setup lang="ts">
/**
 * The question the assistant wrote, offered back to the person.
 *
 * Step 7 of `docs/nexus-lenses-2026-09-19.md`, and §6.2's whole point: the
 * assistant **writes a query, it does not replace one.** Asking in words is a
 * third way into the same machine, not a second machine — so what it asked has
 * to be visible, and keepable.
 *
 * Without this the model's query was a thing that happened inside a tool call
 * and vanished. Somebody who cannot write `events | seq gaps by who` could
 * only ever ask for it again in words; now they ask once and keep it.
 *
 * It is also the only teaching surface for the language that does not require
 * reading any documentation: you asked a question, here is how the machine
 * says it.
 */
import { computed, ref } from 'vue';
import { BookMarked, Check } from 'lucide-vue-next';
import { useNodeService } from '../../../composables/useNodeService';
import { lensPath, nameFor, propertiesOf } from '../../../shared/lenses';
import { logger } from '../../../utils/logger';
import type { SynToolCallEvent } from '../types';

const props = defineProps<{
  vaultPath: string;
  toolCalls?: SynToolCallEvent[];
}>();

const ns = useNodeService();
const kept = ref<Set<string>>(new Set());

/**
 * The questions it asked, each once.
 *
 * Deduplicated because a turn often asks the same thing twice — once narrow,
 * then again wider when the first found nothing (`tool_query_nodes` widens on
 * its own). Offering the same question twice would read as two findings.
 */
const questions = computed(() => {
  const seen = new Set<string>();
  for (const call of props.toolCalls ?? []) {
    if (call.tool_name !== 'query_nodes') continue;
    const query = String(call.tool_args?.query ?? '').trim();
    if (query) seen.add(query);
  }
  return [...seen];
});

const keep = async (query: string) => {
  const title = nameFor(query);
  try {
    await ns.writeNode({
      relPath: lensPath(),
      nodeType: 'lens' as never,
      title,
      properties: propertiesOf({ title, query, render: 'auto', icon: '' }),
      content: '',
      eventType: 'created',
    });
    kept.value = new Set([...kept.value, query]);
  } catch (e) {
    logger.error('Could not keep the question', e);
  }
};
</script>

<template>
  <div v-if="questions.length" data-kept-questions class="mt-2 flex flex-col gap-1">
    <div
      v-for="query in questions"
      :key="query"
      data-kept-question
      class="flex items-center gap-2 rounded-lg border border-gray-200 bg-gray-50 px-2.5 py-1.5 dark:border-[#3a3a3c] dark:bg-[#242426]"
    >
      <!-- The query itself, not a description of it. Somebody who cannot write
           one learns the language by seeing theirs written out. -->
      <code
        class="min-w-0 flex-grow truncate font-mono text-[11px] text-gray-600 dark:text-gray-300"
        :title="query"
        >{{ query }}</code
      >
      <button
        v-if="!kept.has(query)"
        type="button"
        data-keep-question
        class="inline-flex flex-shrink-0 items-center gap-1 rounded-full border border-gray-300 px-2 py-0.5 text-[10px] font-semibold text-gray-600 transition-colors hover:border-indigo-400 hover:text-indigo-600 dark:border-[#48484a] dark:text-gray-300"
        @click="keep(query)"
      >
        <BookMarked class="h-3 w-3" /> {{ $t('nexus.lens_keep') }}
      </button>
      <span
        v-else
        data-question-kept
        class="inline-flex flex-shrink-0 items-center gap-1 text-[10px] font-semibold text-emerald-600 dark:text-emerald-400"
      >
        <Check class="h-3 w-3" /> {{ $t('nexus.lens_kept') }}
      </span>
    </div>
  </div>
</template>
