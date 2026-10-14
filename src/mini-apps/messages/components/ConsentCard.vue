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
import { toolLabel } from '../../../shared/syn/toolLabel';
import SynCard from '../../../shared/syn/SynCard.vue';
import type { ConsentAnswer, ConsentAsk } from '../types';

const props = defineProps<{
  ask: ConsentAsk;
  /**
   * The answer is on its way. The buttons wait for it: a second press used to
   * send a second answer to a question the first one had already closed.
   */
  busy?: boolean;
  /** Why the last answer did not go through, said rather than swallowed. */
  error?: string | null;
}>();
const emit = defineEmits<{ answer: [choice: ConsentAnswer] }>();

const { t } = useI18n();

const sentence = computed(() => {
  const { key, values } = askPhrase(props.ask.capability);
  return t(key, values);
});
</script>

<template>
  <SynCard
    tone="ask"
    :icon="ShieldQuestion"
    :title="t('syn.consent_title')"
    :error="error ? t('syn.consent_failed', { reason: error }) : null"
    :note="ask.per_call ? t('syn.consent_explainer_per_call') : t('syn.consent_explainer')"
  >
    <template #lead>
      <p class="text-sm text-text dark:text-text-dark">{{ sentence }}</p>
      <p class="text-xs text-gray-500 dark:text-gray-400">{{ t('syn.consent_tool', { doing: toolLabel(t, ask.tool) }) }}</p>
      <code
        v-if="ask.detail"
        class="block rounded-md bg-gray-50 dark:bg-white/5 px-2 py-1 font-mono text-xs text-text dark:text-text-dark break-all"
        data-test="consent-detail"
      >{{ ask.detail }}</code>
    </template>

    <!-- One exact call — an ssh command line. "Never" would switch off every
         command to come for the sake of this one, and "always" would be every
         command; so: not now, this time, this exact line for the session. -->
    <template v-if="ask.per_call" #actions>
      <button
        class="inline-flex items-center gap-1.5 px-3 py-1.5 rounded-lg text-sm text-gray-600 dark:text-gray-300
               hover:bg-gray-50 dark:hover:bg-white/5
               disabled:opacity-50 disabled:cursor-default"
        :disabled="busy"
        data-test="consent-not-now"
        @click="emit('answer', 'not_now')"
      >
        <XIcon class="w-3.5 h-3.5" /> {{ t('syn.consent_not_now') }}
      </button>
      <button
        class="inline-flex items-center gap-1.5 px-3 py-1.5 rounded-lg text-sm font-medium
               border border-gray-200 dark:border-gray-700/50 text-text dark:text-text-dark
               hover:bg-gray-50 dark:hover:bg-white/5
               disabled:opacity-50 disabled:cursor-default"
        :disabled="busy"
        data-test="consent-session"
        @click="emit('answer', 'session')"
      >
        <Check class="w-3.5 h-3.5" /> {{ t('syn.consent_session') }}
      </button>
      <button class="btn-primary inline-flex items-center gap-1.5" :disabled="busy" data-test="consent-this-time" @click="emit('answer', 'once')">
        <Clock class="w-3.5 h-3.5" /> {{ t('syn.consent_this_time') }}
      </button>
    </template>
    <template v-else #actions>
      <button
        class="inline-flex items-center gap-1.5 px-3 py-1.5 rounded-lg text-sm text-red-600
               hover:bg-red-50 dark:hover:bg-red-950/40
               disabled:opacity-50 disabled:cursor-default"
        :disabled="busy"
        @click="emit('answer', 'never')"
      >
        <XIcon class="w-3.5 h-3.5" /> {{ t('syn.consent_never') }}
      </button>
      <!-- Absent where it would not be honoured. A ledger showing a permission
           that never applies lies to whoever reads it to find out what they
           agreed to, and a button that produces one is where that starts. -->
      <button
        v-if="ask.can_be_remembered"
        class="inline-flex items-center gap-1.5 px-3 py-1.5 rounded-lg text-sm font-medium
               border border-gray-200 dark:border-gray-700/50 text-text dark:text-text-dark
               hover:bg-gray-50 dark:hover:bg-white/5
               disabled:opacity-50 disabled:cursor-default"
        :disabled="busy"
        @click="emit('answer', 'always')"
      >
        <Check class="w-3.5 h-3.5" /> {{ t('syn.consent_always') }}
      </button>
      <button class="btn-primary inline-flex items-center gap-1.5" :disabled="busy" @click="emit('answer', 'once')">
        <Clock class="w-3.5 h-3.5" /> {{ t('syn.consent_once') }}
      </button>
    </template>
  </SynCard>
</template>
