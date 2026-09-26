<script setup lang="ts">
/**
 * What the chosen model is: where it runs, and what it can do.
 *
 * # Why this says where the words go, per model
 *
 * The provider's description used to say "your vault stays local; the
 * messages you send go to the endpoint". True of the files, and misleading
 * about everything else: the prompt carries the note text Syn retrieved, every
 * tool result, every memory it holds about the person and `SYN.md`, on every
 * turn. Somebody choosing a hosted model is choosing to send those, and the
 * sentence they read before choosing has to say so.
 *
 * And it is per model rather than per provider, because the OpenAI-shaped
 * provider is as happy talking to llama.cpp on this machine as to a service
 * on another continent. The capability table decides, by the address and the
 * model's name. See `provider::capability`.
 */
import { ref, watch } from 'vue';
import { invoke } from '@tauri-apps/api/core';
import { Cpu, CloudUpload } from 'lucide-vue-next';
import { logger } from '../../../utils/logger';
import { shortCount } from '../../../shared/syn/toolLabel';

interface Capability {
  context_window_tokens: number;
  tools: boolean;
  vision: boolean;
  reasoning: boolean;
  hosted: boolean;
  known: boolean;
}

const props = defineProps<{
  vaultPath: string;
  model: string | null | undefined;
  /** One chip, for the conversation header; the whole card otherwise. */
  compact?: boolean;
}>();

const capability = ref<Capability | null>(null);

watch(
  () => [props.vaultPath, props.model] as const,
  async ([vaultPath, model]) => {
    try {
      capability.value = await invoke<Capability>('syn_model_capability', {
        vaultPath,
        model: model || undefined,
      });
    } catch (e) {
      logger.error('[Syn] Could not read what the model can do', e);
      capability.value = null;
    }
  },
  { immediate: true },
);
</script>

<template>
  <template v-if="capability">
    <span
      v-if="compact"
      class="inline-flex items-center gap-1 px-2 py-1 rounded-lg text-[11px] font-medium"
      :class="capability.hosted
        ? 'bg-amber-50 dark:bg-amber-500/10 text-amber-700 dark:text-amber-300'
        : 'bg-emerald-50 dark:bg-emerald-500/10 text-emerald-700 dark:text-emerald-300'"
      :title="capability.hosted ? $t('syn.tier_hosted_says') : $t('syn.tier_local_says')"
    >
      <CloudUpload v-if="capability.hosted" class="w-3 h-3" aria-hidden="true" />
      <Cpu v-else class="w-3 h-3" aria-hidden="true" />
      {{ capability.hosted ? $t('syn.tier_hosted') : $t('syn.tier_local') }}
    </span>

    <section
      v-else
      class="mt-2 rounded-lg border px-3 py-2.5 text-xs space-y-1.5"
      :class="capability.hosted
        ? 'border-amber-200 dark:border-amber-500/20 bg-amber-50/60 dark:bg-amber-500/5'
        : 'border-emerald-200 dark:border-emerald-500/20 bg-emerald-50/60 dark:bg-emerald-500/5'"
      :aria-label="$t('syn.tier_label')"
    >
      <p class="flex items-start gap-1.5 font-medium text-text dark:text-text-dark">
        <CloudUpload v-if="capability.hosted" class="w-3.5 h-3.5 mt-px shrink-0 text-amber-600" aria-hidden="true" />
        <Cpu v-else class="w-3.5 h-3.5 mt-px shrink-0 text-emerald-600" aria-hidden="true" />
        {{ capability.hosted ? $t('syn.tier_hosted_says') : $t('syn.tier_local_says') }}
      </p>
      <ul class="flex flex-wrap gap-x-3 gap-y-0.5 text-gray-600 dark:text-gray-300">
        <li>{{ $t('syn.tier_window', { n: shortCount(capability.context_window_tokens) }) }}</li>
        <li>{{ capability.tools ? $t('syn.tier_tools') : $t('syn.tier_no_tools') }}</li>
        <li v-if="capability.vision">{{ $t('syn.tier_vision') }}</li>
        <li v-if="capability.reasoning">{{ $t('syn.tier_reasoning') }}</li>
      </ul>
      <p v-if="!capability.known" class="text-gray-500 dark:text-gray-400">{{ $t('syn.tier_guessed') }}</p>
    </section>
  </template>
</template>
