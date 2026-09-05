<script setup lang="ts">
/**
 * Syn stopped and is asking.
 *
 * A card in the conversation rather than a modal over it. Three answers, and
 * the middle one is missing for the powers that cannot be granted permanently —
 * money and running code — because offering a button that will not be honoured
 * is worse than not offering it.
 *
 * The sentence comes from i18n keyed on the capability, never from the English
 * `about` field the backend also sends. That field is for the audit log.
 */
import { computed } from 'vue';
import { useI18n } from 'vue-i18n';
import { ShieldQuestion, Check, Clock, X as XIcon } from 'lucide-vue-next';

import { askPhrase } from '../composables/useSynConsent';
import type { ConsentAnswer, ConsentAsk } from '../types';

const props = defineProps<{ ask: ConsentAsk }>();
const emit = defineEmits<{ answer: [choice: ConsentAnswer] }>();

const { t } = useI18n();

const sentence = computed(() => {
  const { key, values } = askPhrase(props.ask.capability);
  return t(key, values);
});
</script>

<template>
  <div
    class="rounded-xl border border-amber-300 dark:border-amber-900/70
           bg-amber-50/60 dark:bg-amber-950/25 p-4"
  >
    <div class="flex items-center gap-2 text-[11px] font-medium text-amber-700 dark:text-amber-400">
      <ShieldQuestion class="w-3.5 h-3.5" />
      {{ t('syn.consent_title') }}
    </div>

    <p class="mt-2 text-sm text-text dark:text-text-dark">{{ sentence }}</p>
    <p class="mt-1 text-[11px] text-gray-500">
      {{ t('syn.consent_tool', { tool: ask.tool }) }}
    </p>

    <div class="mt-3 flex flex-wrap gap-2">
      <button
        class="inline-flex items-center gap-1.5 px-2.5 py-1 text-xs rounded-lg
               bg-violet-600 text-white hover:bg-violet-700"
        @click="emit('answer', 'once')"
      >
        <Clock class="w-3 h-3" /> {{ t('syn.consent_once') }}
      </button>

      <!-- Absent where it would not be honoured. A ledger showing a permission
           that never applies lies to whoever reads it to find out what they
           agreed to, and a button that produces one is where that starts. -->
      <button
        v-if="ask.can_be_remembered"
        class="inline-flex items-center gap-1.5 px-2.5 py-1 text-xs rounded-lg
               bg-gray-100 dark:bg-gray-800 hover:bg-gray-200 dark:hover:bg-gray-700"
        @click="emit('answer', 'always')"
      >
        <Check class="w-3 h-3" /> {{ t('syn.consent_always') }}
      </button>

      <button
        class="inline-flex items-center gap-1.5 px-2.5 py-1 text-xs rounded-lg text-red-600
               hover:bg-red-50 dark:hover:bg-red-950/40"
        @click="emit('answer', 'never')"
      >
        <XIcon class="w-3 h-3" /> {{ t('syn.consent_never') }}
      </button>
    </div>

    <p class="mt-2 text-[11px] text-gray-500">{{ t('syn.consent_explainer') }}</p>
  </div>
</template>
