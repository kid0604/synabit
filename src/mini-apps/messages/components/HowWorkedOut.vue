<script setup lang="ts">
import { computed, ref } from 'vue';
import { ChevronDown, ChevronRight, Calculator } from 'lucide-vue-next';
import { howWorkedOut, type WorkedStep } from '../declared';
import type { SynToolCallEvent } from '../types';

/**
 * "How this was worked out": the tables an answer stood on and the `crunch`
 * pipelines that made its figures — built from what the run did, not from
 * anything the model said about it. Shown only when something was computed.
 */
const props = defineProps<{ toolCalls?: SynToolCallEvent[] }>();
const open = ref(false);
const steps = computed<WorkedStep[]>(() => howWorkedOut(props.toolCalls));
</script>

<template>
  <div v-if="steps.length" class="mt-2" data-test="how-worked-out">
    <button
      type="button"
      :aria-expanded="open"
      class="flex items-center gap-1.5 min-h-6 -mx-1 px-1 rounded-md text-xs font-medium text-gray-500 dark:text-gray-400 hover:text-gray-700 dark:hover:text-gray-300 cursor-pointer"
      @click="open = !open"
    >
      <Calculator class="w-3 h-3" aria-hidden="true" />
      <span>{{ $t('syn.how_worked_out') }}</span>
      <ChevronDown v-if="open" class="w-3 h-3" />
      <ChevronRight v-else class="w-3 h-3" />
    </button>
    <ol v-if="open" class="mt-2 space-y-1">
      <li v-for="(s, i) in steps" :key="i" class="px-2.5 py-1.5 rounded-md bg-gray-50 dark:bg-gray-800/30 text-xs space-y-0.5">
        <div class="flex items-center gap-2 min-w-0">
          <span class="font-medium text-text dark:text-text-dark shrink-0">
            {{ s.kind === 'crunched' ? $t('syn.how_crunched', { from: s.from }) : $t('syn.how_fetched', { from: s.from }) }}
          </span>
          <span v-if="s.handle" class="ml-auto font-mono text-gray-500 dark:text-gray-400 shrink-0">
            → {{ s.handle }}<template v-if="s.rows !== null"> · {{ $t('syn.how_rows', { n: s.rows }, s.rows) }}</template>
          </span>
        </div>
        <code class="block font-mono text-gray-500 dark:text-gray-400 break-all">{{ s.asked }}</code>
      </li>
    </ol>
  </div>
</template>
