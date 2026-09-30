<script setup lang="ts">
/**
 * What to do when Syn cannot reach its model.
 *
 * The strings for this existed long before anything showed them, so a first
 * run with no Ollama was a banner saying "disconnected" and nothing to press.
 * This is the path from there: what is missing, in plain words, and a button
 * for each thing that fixes it.
 *
 * Three cases, because the fix is different in each:
 *
 * * **Ollama, on a computer** — install it, open it, download a model. The
 *   last step is a button, not a terminal command: the app already knows how
 *   to ask Ollama for a model (`syn_pull_model`), so the card does that and
 *   shows the progress, instead of asking somebody to type into a shell.
 * * **Ollama, on a phone** — it cannot run there, and the default address
 *   (`localhost`) is the phone itself. The provider is not switched on the
 *   person's behalf: sending notes to a cloud model is their decision, so the
 *   card says so and points at the setting.
 * * **A hosted provider** — the network or the key.
 */
import { computed, ref } from 'vue';
import { useI18n } from 'vue-i18n';
import { openUrl } from '@tauri-apps/plugin-opener';
import { WifiOff, Download, Settings, RefreshCw, Check } from 'lucide-vue-next';
import { logger } from '../../../utils/logger';
import { useSynModels } from '../composables/useSynModels';
import { RECOMMENDED_LOCAL_MODEL } from '../models';

const props = defineProps<{
  /** Whether the provider in use is Ollama, which runs on this machine. */
  local: boolean;
  /** The provider's name, as the reader knows it. */
  providerName: string;
  /** A phone or tablet, where Ollama cannot run. */
  onPhone?: boolean;
  /** The vault, whose Ollama address the download goes to. */
  vaultPath?: string;
}>();

const emit = defineEmits<{
  'open-settings': [];
  retry: [];
}>();

const { t } = useI18n();

const kind = computed(() => (!props.local ? 'remote' : props.onPhone ? 'phone' : 'ollama'));

// The same pull the model picker uses — `syn_pull_model` and its
// `syn-pull-progress` events — through its own instance, so this card needs
// nothing from the settings panel.
const { pullModel, pullingModel, pullProgress } = useSynModels();
const pulled = ref(false);
const pullFailed = ref(false);

const downloadModel = async () => {
  pullFailed.value = false;
  const ok = await pullModel(RECOMMENDED_LOCAL_MODEL, props.vaultPath);
  if (ok) {
    pulled.value = true;
    // Ollama answered, so the connection the card was shown for is back.
    emit('retry');
  } else {
    pullFailed.value = true;
  }
};

const downloadOllama = () => {
  openUrl('https://ollama.com/download').catch(e => logger.error('[Syn] Could not open the Ollama download page', e));
};
</script>

<template>
  <div
    data-syn-setup
    :data-kind="kind"
    class="w-full max-w-md mx-auto rounded-2xl border border-gray-200 dark:border-gray-700/50 bg-white dark:bg-white/5 p-5 text-left"
  >
    <div class="flex items-center gap-3 mb-3">
      <div class="w-9 h-9 rounded-xl bg-amber-500/10 flex items-center justify-center flex-shrink-0">
        <WifiOff class="w-4.5 h-4.5 text-amber-600 dark:text-amber-400" />
      </div>
      <p class="text-sm font-semibold text-gray-900 dark:text-white">
        <template v-if="kind === 'ollama'">{{ t('syn.setup_title') }}</template>
        <template v-else-if="kind === 'phone'">{{ t('syn.setup_phone_title') }}</template>
        <template v-else>{{ t('syn.setup_remote_title', { provider: providerName }) }}</template>
      </p>
    </div>

    <template v-if="kind === 'ollama'">
      <p class="text-[13px] text-gray-600 dark:text-gray-300 leading-relaxed">{{ t('syn.setup_missing_ollama') }}</p>
      <ol class="mt-3 space-y-2 text-[13px]">
        <li v-for="step in [1, 2, 3]" :key="step" class="flex gap-2.5">
          <span class="w-5 h-5 rounded-full bg-violet-500/10 text-violet-600 dark:text-violet-400 text-xs font-semibold flex items-center justify-center flex-shrink-0">{{ step }}</span>
          <span class="min-w-0">
            <span class="block font-medium text-gray-800 dark:text-gray-200">{{ t(`syn.setup_step${step}_title`) }}</span>
            <span class="block text-gray-500 dark:text-gray-400 break-words">{{ t(`syn.setup_step${step}_desc`) }}</span>
            <template v-if="step === 3">
              <button
                type="button"
                class="btn-secondary mt-2"
                data-syn-pull
                :disabled="pullingModel || pulled"
                @click="downloadModel"
              >
                <Check v-if="pulled" class="w-3.5 h-3.5" aria-hidden="true" />
                <Download v-else class="w-3.5 h-3.5" aria-hidden="true" />
                {{ pulled ? t('syn.setup_model_ready') : pullingModel ? t('syn.pulling_model') : t('syn.setup_download_model') }}
              </button>
              <span v-if="pullingModel" class="mt-2 block" role="status">
                <span
                  class="block h-1.5 rounded-full bg-gray-100 dark:bg-gray-800 overflow-hidden"
                  role="progressbar"
                  :aria-valuenow="Math.round(pullProgress)"
                  aria-valuemin="0"
                  aria-valuemax="100"
                  :aria-label="t('syn.pulling_model')"
                >
                  <span class="block h-full bg-violet-500 transition-[width]" :style="{ width: `${pullProgress}%` }" />
                </span>
                <span class="mt-1 block text-xs text-gray-500 dark:text-gray-400">{{ t('syn.setup_pull_percent', { n: Math.round(pullProgress) }) }}</span>
              </span>
              <span v-else-if="pullFailed" class="mt-2 block text-xs text-red-600 dark:text-red-400" role="alert">
                {{ t('syn.setup_pull_failed') }}
              </span>
            </template>
          </span>
        </li>
      </ol>
    </template>
    <p v-else-if="kind === 'phone'" class="text-[13px] text-gray-600 dark:text-gray-300 leading-relaxed">
      {{ t('syn.setup_phone_body') }}
    </p>
    <p v-else class="text-[13px] text-gray-600 dark:text-gray-300 leading-relaxed">
      {{ t('syn.setup_remote_body') }}
    </p>

    <div class="mt-4 flex flex-wrap gap-2">
      <button
        v-if="kind === 'ollama'"
        type="button"
        class="btn-primary"
        @click="downloadOllama"
      >
        <Download class="w-3.5 h-3.5" />
        {{ t('syn.download_ollama') }}
      </button>
      <button
        type="button"
        :class="kind === 'ollama' ? 'btn-secondary' : 'btn-primary'"
        @click="emit('open-settings')"
      >
        <Settings class="w-3.5 h-3.5" />
        {{ t('syn.setup_open_settings') }}
      </button>
      <button
        v-if="kind !== 'phone'"
        type="button"
        class="btn-secondary"
        @click="emit('retry')"
      >
        <RefreshCw class="w-3.5 h-3.5" />
        {{ t('syn.retry_connection') }}
      </button>
    </div>
  </div>
</template>
