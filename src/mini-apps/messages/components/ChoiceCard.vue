<script setup lang="ts">
/**
 * Syn found several and is asking which.
 *
 * Deliberately **not** amber like `ConsentCard`. That card means a permission
 * was never granted and something is being refused; this one means the work is
 * going fine and Syn will not guess one detail. Two cards that look alike and
 * mean opposite things is how somebody learns to answer both with the same
 * reflex.
 *
 * The model's pick is marked rather than hidden. It is usually right, and a
 * question that makes somebody redo the search from scratch is a question they
 * stop answering — which would leave the guess in place, unasked, which is
 * where this started.
 */
import { useI18n } from 'vue-i18n';
import { HelpCircle, Sparkles } from 'lucide-vue-next';

import type { AmbiguousChoice } from '../types';

const props = defineProps<{ choice: AmbiguousChoice }>();
const emit = defineEmits<{ answer: [nodeId: string] }>();

const { t } = useI18n();

/** *Delete* or *change*, so the question says what is about to happen. */
const verb = () =>
  props.choice.tool === 'trash_node' ? t('syn.choice_remove') : t('syn.choice_change');
</script>

<template>
  <div
    class="rounded-xl border border-violet-200 dark:border-violet-900/60
           bg-violet-50/50 dark:bg-violet-950/20 p-4"
  >
    <div class="flex items-center gap-2 text-[11px] font-medium text-violet-700 dark:text-violet-400">
      <HelpCircle class="w-3.5 h-3.5" />
      {{ t('syn.choice_title') }}
    </div>

    <p class="mt-2 text-sm text-text dark:text-text-dark">
      {{ t('syn.choice_body', { n: choice.candidates.length, verb: verb() }) }}
    </p>

    <ul class="mt-3 space-y-1.5">
      <li v-for="candidate in choice.candidates" :key="candidate.id">
        <button
          class="w-full flex items-center gap-2 px-3 py-2 rounded-lg text-left text-sm
                 border transition-colors cursor-pointer
                 hover:bg-white dark:hover:bg-white/5"
          :class="candidate.id === choice.chose
            ? 'border-violet-300 dark:border-violet-700 bg-white/70 dark:bg-white/5'
            : 'border-transparent'"
          @click="emit('answer', candidate.id)"
        >
          <span class="flex-1 min-w-0 truncate text-text dark:text-text-dark">
            {{ candidate.title || candidate.id }}
          </span>
          <span v-if="candidate.node_type" class="shrink-0 text-[10px] text-gray-400">
            {{ candidate.node_type }}
          </span>
          <!-- What Syn was about to do, named rather than silently applied. -->
          <Sparkles
            v-if="candidate.id === choice.chose"
            class="w-3.5 h-3.5 shrink-0 text-violet-500"
            :aria-label="t('syn.choice_syn_picked')"
          />
        </button>
      </li>
    </ul>

    <p class="mt-2.5 text-[11px] text-gray-500">{{ t('syn.choice_explainer') }}</p>
  </div>
</template>
