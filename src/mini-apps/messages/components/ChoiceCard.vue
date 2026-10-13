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
import SynCard from '../../../shared/syn/SynCard.vue';

const props = defineProps<{ choice: AmbiguousChoice }>();
const emit = defineEmits<{ answer: [nodeId: string] }>();

const { t } = useI18n();

/** *Delete* or *change*, so the question says what is about to happen. */
const verb = () =>
  props.choice.tool === 'trash_node' ? t('syn.choice_remove') : t('syn.choice_change');
</script>

<template>
  <SynCard
    tone="choose"
    :icon="HelpCircle"
    :title="t('syn.choice_title')"
    :lead="t('syn.choice_body', { n: choice.candidates.length, verb: verb() })"
    :note="t('syn.choice_explainer')"
  >
    <ul class="space-y-1.5">
      <li v-for="candidate in choice.candidates" :key="candidate.id">
        <button
          class="w-full flex items-center gap-2 px-3 py-2 rounded-lg text-left text-sm
                 border transition-colors cursor-pointer
                 hover:bg-white dark:hover:bg-white/5"
          :class="candidate.id === choice.chose
            ? 'border-accent/50 dark:border-accent-dark/50 bg-white/70 dark:bg-white/5'
            : 'border-transparent'"
          @click="emit('answer', candidate.id)"
        >
          <span class="flex-1 min-w-0 truncate text-text dark:text-text-dark">
            {{ candidate.title || candidate.id }}
          </span>
          <span v-if="candidate.node_type" class="shrink-0 text-xs text-gray-500 dark:text-gray-400">
            {{ candidate.node_type }}
          </span>
          <!-- What Syn was about to do, named rather than silently applied. -->
          <Sparkles
            v-if="candidate.id === choice.chose"
            class="w-3.5 h-3.5 shrink-0 text-accent dark:text-accent-dark"
            :aria-label="t('syn.choice_syn_picked')"
          />
        </button>
      </li>
    </ul>
  </SynCard>
</template>
