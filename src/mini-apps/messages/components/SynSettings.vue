<script setup lang="ts">
import { onMounted, onUnmounted, watch, computed } from 'vue';
import { useI18n } from 'vue-i18n';
import { X, RotateCcw, Save, Loader2 } from 'lucide-vue-next';
import { SETTINGS_SAVED } from '../../../shared/syn/useSynEnabled';
import { useSynSettings } from '../composables/useSynSettings';
import type { ModelInfo } from '../types';

const props = defineProps<{
  vaultPath: string;
  models: ModelInfo[];
}>();

const emit = defineEmits<{
  close: [];
  saved: [];
}>();

const { t } = useI18n();

const {
  settings,
  savedProvider,
  isLoading,
  isSaving,
  hasApiKey,
  apiKeyDraft,
  loadSettings,
  saveSettings,
  clearApiKey,
  resetToDefaults,
} = useSynSettings(props.vaultPath);

const usingOllama = computed(() => settings.value.provider === 'ollama');
const usingGemini = computed(() => settings.value.provider === 'gemini');

/**
 * Whether the model list below belongs to a provider other than the one
 * selected. It is fetched for the provider in use, and until this screen is
 * saved that is not the one the selector now reads.
 */
const modelsAreStale = computed(() => settings.value.provider !== savedProvider.value);

/** What an empty key field shows. Keys look different, and a hint shaped
 *  like the wrong one is a hint that the wrong key goes here. */
const keyLooksLike = computed(() =>
  usingGemini.value ? t('syn.api_key_placeholder_gemini') : t('syn.api_key_placeholder'),
);

/** What to say under the selector about where the words go. */
const providerSays = computed(() => {
  if (usingOllama.value) return t('syn.provider_ollama_desc');
  if (usingGemini.value) return t('syn.provider_gemini_desc');
  return t('syn.provider_openai_desc');
});

const handleSave = async () => {
  await saveSettings();
  emit('saved');
  // The ask bar lives in `App.vue`, above every mini-app and outside this
  // component's tree, and the switch above turns it off. Nothing else connects
  // the two. See `useSynEnabled`.
  window.dispatchEvent(new CustomEvent(SETTINGS_SAVED));
};

const handleReset = () => {
  resetToDefaults();
};

// Close on Escape
const handleKeydown = (e: KeyboardEvent) => {
  if (e.key === 'Escape') {
    emit('close');
  }
};

onMounted(async () => {
  await loadSettings();
  window.addEventListener('keydown', handleKeydown);
});

onUnmounted(() => {
  window.removeEventListener('keydown', handleKeydown);
});

watch(() => props.vaultPath, () => {
  loadSettings();
});
</script>

<template>
  <Teleport to="body">
    <!-- Backdrop -->
    <Transition appear
      enter-active-class="transition-opacity duration-200 ease-out"
      enter-from-class="opacity-0"
      enter-to-class="opacity-100"
      leave-active-class="transition-opacity duration-150 ease-in"
      leave-from-class="opacity-100"
      leave-to-class="opacity-0"
    >
      <div v-show="true"
        class="fixed inset-0 z-[998] bg-black/40 backdrop-blur-sm"
        @click="emit('close')"
      />
    </Transition>

    <!-- Panel -->
    <Transition appear
      enter-active-class="transition-transform duration-250 ease-out"
      enter-from-class="translate-x-full"
      enter-to-class="translate-x-0"
      leave-active-class="transition-transform duration-200 ease-in"
      leave-from-class="translate-x-0"
      leave-to-class="translate-x-full"
    >
      <div v-show="true"
        class="fixed right-0 top-0 bottom-0 z-[999] w-[420px] max-w-full flex flex-col
               bg-white dark:bg-[#13141a] border-l border-gray-200 dark:border-gray-800/60
               shadow-2xl shadow-black/20"
      >
        <!-- Header -->
        <div class="flex items-center justify-between px-6 py-4 border-b border-gray-100 dark:border-gray-800/60">
          <div class="flex items-center gap-2.5">
            <div class="w-8 h-8 rounded-lg bg-gradient-to-br from-violet-500 to-purple-600 flex items-center justify-center shadow-sm">
              <span class="text-sm">⚙️</span>
            </div>
            <h2 class="text-lg font-semibold text-text dark:text-text-dark">{{ t('syn.settings') }}</h2>
          </div>
          <button
            @click="emit('close')"
            class="p-1.5 rounded-lg hover:bg-gray-100 dark:hover:bg-white/5 text-gray-400 hover:text-gray-600 dark:hover:text-gray-300 transition-colors cursor-pointer"
           aria-label="More Options">
            <X class="w-5 h-5" />
          </button>
        </div>

        <!-- Loading -->
        <div v-if="isLoading" class="flex-1 flex items-center justify-center">
          <Loader2 class="w-6 h-6 text-violet-500 animate-spin" />
        </div>

        <!-- Settings content -->
        <div v-else class="flex-1 overflow-y-auto px-6 py-5 space-y-6">

          <!-- The switch, above everything.
               First because it outranks every control under it: a model, a
               temperature and a memory budget are all answers to "how should
               Syn work", and this is the answer to "should it". It stays
               visible when off, because the control that brings Syn back must
               not be somewhere Syn has to be on to reach. -->
          <label
            class="flex items-start gap-3 p-3 rounded-xl border cursor-pointer transition-colors"
            :class="settings.enabled
              ? 'border-gray-200 dark:border-gray-700/50'
              : 'border-amber-300 dark:border-amber-500/40 bg-amber-50/50 dark:bg-amber-500/5'"
          >
            <input
              type="checkbox"
              v-model="settings.enabled"
              class="mt-0.5 w-4 h-4 accent-violet-500 cursor-pointer"
            />
            <span class="min-w-0">
              <span class="block text-sm font-medium text-text dark:text-text-dark">
                {{ t('syn.enabled') }}
              </span>
              <span class="block text-[11px] text-gray-500 dark:text-gray-400 mt-0.5 leading-relaxed">
                {{ t('syn.enabled_hint') }}
              </span>
            </span>
          </label>

          <!-- CONNECTION -->
          <section>
            <h3 class="text-[11px] font-bold uppercase tracking-wider text-gray-400 dark:text-gray-500 mb-3">
              {{ t('syn.settings_connection') }}
            </h3>
            <div class="space-y-3">
              <!-- Provider -->
              <div>
                <label class="block text-sm font-medium text-text dark:text-text-dark mb-1.5">
                  {{ t('syn.provider') }}
                </label>
                <select
                  v-model="settings.provider"
                  class="w-full px-3 py-2 rounded-lg bg-gray-50 dark:bg-white/5 border border-gray-200 dark:border-gray-700/50
                         text-sm text-text dark:text-text-dark outline-none cursor-pointer
                         focus:border-violet-400 dark:focus:border-violet-500/50 focus:ring-1 focus:ring-violet-400/20
                         transition-all appearance-none"
                >
                  <option value="ollama">{{ t('syn.provider_ollama') }}</option>
                  <option value="open_ai_compat">{{ t('syn.provider_openai') }}</option>
                  <option value="gemini">{{ t('syn.provider_gemini') }}</option>
                </select>
                <p class="mt-1.5 text-xs text-gray-500 dark:text-gray-400">
                  {{ providerSays }}
                </p>
              </div>

              <!-- Ollama URL -->
              <div v-if="usingOllama">
                <label class="block text-sm font-medium text-text dark:text-text-dark mb-1.5">
                  {{ t('syn.ollama_url') }}
                </label>
                <input
                  v-model="settings.ollama_url"
                  type="text"
                  class="w-full px-3 py-2 rounded-lg bg-gray-50 dark:bg-white/5 border border-gray-200 dark:border-gray-700/50
                         text-sm text-text dark:text-text-dark placeholder-gray-400 outline-none
                         focus:border-violet-400 dark:focus:border-violet-500/50 focus:ring-1 focus:ring-violet-400/20
                         transition-all"
                  placeholder="http://localhost:11434"
                />
              </div>

              <!-- A hosted provider: an address for the OpenAI shape, and a key
                   for both. Gemini has one address and nothing to point
                   elsewhere, so it shows the key alone. -->
              <template v-else>
                <div v-if="!usingGemini">
                  <label class="block text-sm font-medium text-text dark:text-text-dark mb-1.5">
                    {{ t('syn.openai_base_url') }}
                  </label>
                  <input
                    v-model="settings.openai_base_url"
                    type="text"
                    spellcheck="false"
                    autocomplete="off"
                    class="w-full px-3 py-2 rounded-lg bg-gray-50 dark:bg-white/5 border border-gray-200 dark:border-gray-700/50
                           text-sm text-text dark:text-text-dark placeholder-gray-400 outline-none
                           focus:border-violet-400 dark:focus:border-violet-500/50 focus:ring-1 focus:ring-violet-400/20
                           transition-all"
                    placeholder="https://api.openai.com/v1"
                  />
                  <p class="mt-1.5 text-xs text-gray-500 dark:text-gray-400">
                    {{ t('syn.openai_base_url_desc') }}
                  </p>
                </div>

                <div>
                  <label class="block text-sm font-medium text-text dark:text-text-dark mb-1.5">
                    {{ t('syn.api_key') }}
                  </label>
                  <div class="flex gap-2">
                    <input
                      v-model="apiKeyDraft"
                      type="password"
                      spellcheck="false"
                      autocomplete="off"
                      class="flex-1 min-w-0 px-3 py-2 rounded-lg bg-gray-50 dark:bg-white/5 border border-gray-200 dark:border-gray-700/50
                             text-sm text-text dark:text-text-dark placeholder-gray-400 outline-none
                             focus:border-violet-400 dark:focus:border-violet-500/50 focus:ring-1 focus:ring-violet-400/20
                             transition-all"
                      :placeholder="hasApiKey ? t('syn.api_key_stored') : keyLooksLike"
                    />
                    <button
                      v-if="hasApiKey"
                      type="button"
                      @click="clearApiKey"
                      class="px-3 py-2 rounded-lg text-sm font-medium cursor-pointer
                             text-red-600 dark:text-red-400 hover:bg-red-50 dark:hover:bg-red-500/10
                             border border-gray-200 dark:border-gray-700/50 transition-colors"
                    >
                      {{ t('syn.api_key_remove') }}
                    </button>
                  </div>
                  <p class="mt-1.5 text-xs text-gray-500 dark:text-gray-400">
                    {{ usingGemini ? t('syn.api_key_desc_gemini') : t('syn.api_key_desc') }}
                  </p>
                </div>

              </template>

              <!-- Default Model -->
              <div>
                <label class="block text-sm font-medium text-text dark:text-text-dark mb-1.5">
                  {{ t('syn.default_model') }}
                </label>
                <!-- Not a list of another provider's models. Every name in it
                     would be a 404 on the one the selector now reads. What
                     this provider was last set to is shown, because it is
                     remembered — and switching back finds the other's intact. -->
                <div
                  v-if="modelsAreStale"
                  class="px-3 py-2 rounded-lg bg-gray-50 dark:bg-white/5 border border-dashed border-gray-200 dark:border-gray-700/50"
                >
                  <p v-if="settings.default_model" class="text-sm text-text dark:text-text-dark">
                    {{ settings.default_model }}
                  </p>
                  <p class="text-xs text-gray-500 dark:text-gray-400">
                    {{ t('syn.default_model_after_save') }}
                  </p>
                </div>
                <select
                  v-else
                  v-model="settings.default_model"
                  class="w-full px-3 py-2 rounded-lg bg-gray-50 dark:bg-white/5 border border-gray-200 dark:border-gray-700/50
                         text-sm text-text dark:text-text-dark outline-none cursor-pointer
                         focus:border-violet-400 dark:focus:border-violet-500/50 focus:ring-1 focus:ring-violet-400/20
                         transition-all appearance-none"
                >
                  <option :value="null">{{ t('syn.select_model') }}</option>
                  <option v-for="model in models" :key="model.name" :value="model.name">
                    {{ model.name }}
                  </option>
                </select>
              </div>
            </div>
          </section>

          <!-- GENERATION -->
          <section>
            <h3 class="text-[11px] font-bold uppercase tracking-wider text-gray-400 dark:text-gray-500 mb-3">
              {{ t('syn.settings_generation') }}
            </h3>
            <div class="space-y-4">
              <!-- Temperature -->
              <div>
                <div class="flex items-center justify-between mb-2">
                  <label class="text-sm font-medium text-text dark:text-text-dark">
                    {{ t('syn.temperature') }}
                  </label>
                  <span class="text-sm font-mono font-semibold text-violet-500">{{ settings.temperature.toFixed(1) }}</span>
                </div>
                <input
                  v-model.number="settings.temperature"
                  type="range"
                  min="0"
                  max="1"
                  step="0.1"
                  class="w-full h-1.5 rounded-full appearance-none cursor-pointer
                         bg-gray-200 dark:bg-gray-700
                         accent-violet-500"
                />
                <div class="flex justify-between text-[10px] text-gray-400 mt-1">
                  <span>{{ t('syn.temperature_precise') }}</span>
                  <span>{{ t('syn.temperature_creative') }}</span>
                </div>
              </div>

              <!-- Max Tool Calls -->
              <div>
                <label class="block text-sm font-medium text-text dark:text-text-dark mb-1.5">
                  {{ t('syn.max_tool_calls') }}
                </label>
                <input
                  v-model.number="settings.max_tool_iterations"
                  type="number"
                  min="1"
                  max="20"
                  class="w-20 px-3 py-2 rounded-lg bg-gray-50 dark:bg-white/5 border border-gray-200 dark:border-gray-700/50
                         text-sm text-text dark:text-text-dark outline-none text-center
                         focus:border-violet-400 dark:focus:border-violet-500/50 focus:ring-1 focus:ring-violet-400/20
                         transition-all"
                />
              </div>

              <!-- Context Window -->
              <div>
                <label class="block text-sm font-medium text-text dark:text-text-dark mb-1.5">
                  {{ t('syn.context_window') }}
                </label>
                <select
                  v-model.number="settings.num_ctx"
                  class="w-full px-3 py-2 rounded-lg border border-border dark:border-border-dark bg-surface-alt dark:bg-surface-alt-dark text-text dark:text-text-dark text-sm focus:outline-none focus:ring-2 focus:ring-primary"
                >
                  <option :value="4096">4K</option>
                  <option :value="8192">8K ({{ t('syn.default') }})</option>
                  <option :value="16384">16K</option>
                  <option :value="32768">32K</option>
                  <option :value="65536">64K</option>
                  <option :value="131072">128K ⚠️</option>
                </select>
                <p v-if="settings.num_ctx > 32768" class="mt-1 text-xs text-amber-500">
                  {{ t('syn.high_context_warning') }}
                </p>
              </div>

              <!-- Max History -->
              <div>
                <label class="block text-sm font-medium text-text dark:text-text-dark mb-1.5">
                  {{ t('syn.max_history') }}
                </label>
                <input
                  v-model.number="settings.max_history_messages"
                  type="number"
                  min="10"
                  max="200"
                  step="10"
                  class="w-full px-3 py-2 rounded-lg border border-border dark:border-border-dark bg-surface-alt dark:bg-surface-alt-dark text-text dark:text-text-dark text-sm focus:outline-none focus:ring-2 focus:ring-primary"
                />
              </div>
            </div>
          </section>

          <!-- CONTEXT (RAG) -->
          <section>
            <h3 class="text-[11px] font-bold uppercase tracking-wider text-gray-400 dark:text-gray-500 mb-3">
              {{ t('syn.settings_context') }}
            </h3>
            <div class="space-y-3">
              <!-- Enable vault context -->
              <div class="flex items-center justify-between py-1">
                <label class="text-sm text-text dark:text-text-dark">{{ t('syn.enable_vault_context') }}</label>
                <button
                  @click="settings.rag_enabled = !settings.rag_enabled"
                  class="relative w-11 h-6 rounded-full transition-colors duration-200 cursor-pointer"
                  :class="settings.rag_enabled ? 'bg-violet-500' : 'bg-gray-300 dark:bg-gray-600'"
                 aria-label="Settings.rag_enabled = !settings.rag_enabled">
                  <span
                    class="absolute top-0.5 left-0.5 w-5 h-5 rounded-full bg-white shadow-sm transition-transform duration-200"
                    :class="settings.rag_enabled ? 'translate-x-5' : 'translate-x-0'"
                  />
                </button>
              </div>

              <!-- Include finance -->
              <div class="flex items-center justify-between py-1">
                <label class="text-sm text-text dark:text-text-dark">{{ t('syn.include_finance') }}</label>
                <button
                  @click="settings.include_finance = !settings.include_finance"
                  class="relative w-11 h-6 rounded-full transition-colors duration-200 cursor-pointer"
                  :class="settings.include_finance ? 'bg-violet-500' : 'bg-gray-300 dark:bg-gray-600'"
                 aria-label="Settings.include_finance = !settings.include_finance">
                  <span
                    class="absolute top-0.5 left-0.5 w-5 h-5 rounded-full bg-white shadow-sm transition-transform duration-200"
                    :class="settings.include_finance ? 'translate-x-5' : 'translate-x-0'"
                  />
                </button>
              </div>

              <!-- Include feeds -->
              <div class="flex items-center justify-between py-1">
                <label class="text-sm text-text dark:text-text-dark">{{ t('syn.include_feeds') }}</label>
                <button
                  @click="settings.include_feeds = !settings.include_feeds"
                  class="relative w-11 h-6 rounded-full transition-colors duration-200 cursor-pointer"
                  :class="settings.include_feeds ? 'bg-violet-500' : 'bg-gray-300 dark:bg-gray-600'"
                 aria-label="Settings.include_feeds = !settings.include_feeds">
                  <span
                    class="absolute top-0.5 left-0.5 w-5 h-5 rounded-full bg-white shadow-sm transition-transform duration-200"
                    :class="settings.include_feeds ? 'translate-x-5' : 'translate-x-0'"
                  />
                </button>
              </div>

              <!--
                How much of a web page reaches the model.

                Empty means "let the provider decide", which is a real answer
                and not a missing one: eight thousand characters was written
                against Ollama's 8,192-token window, and a hosted model's window
                is a property of the model. It is a slice, not a ceiling — a
                longer page arrives with an outline of what is past the cut and
                a way to read on.
              -->
              <div>
                <label class="block text-sm font-medium text-text dark:text-text-dark mb-1.5">
                  {{ t('syn.page_budget') }}
                </label>
                <input
                  v-model.number="settings.max_page_chars"
                  type="number"
                  min="1000"
                  max="200000"
                  step="1000"
                  :placeholder="t('syn.page_budget_hint')"
                  class="w-32 px-3 py-2 rounded-lg bg-gray-50 dark:bg-white/5 border border-gray-200 dark:border-gray-700/50
                         text-sm text-text dark:text-text-dark outline-none text-center
                         focus:border-violet-400 dark:focus:border-violet-500/50 focus:ring-1 focus:ring-violet-400/20
                         transition-all"
                />
                <p class="mt-1.5 text-xs text-text/50 dark:text-text-dark/50 max-w-md">
                  {{ t('syn.page_budget_hint') }}
                </p>
              </div>

              <!-- Context budget -->
              <div>
                <label class="block text-sm font-medium text-text dark:text-text-dark mb-1.5">
                  {{ t('syn.context_budget') }}
                </label>
                <input
                  v-model.number="settings.max_context_chars"
                  type="number"
                  min="1000"
                  max="128000"
                  step="1000"
                  class="w-32 px-3 py-2 rounded-lg bg-gray-50 dark:bg-white/5 border border-gray-200 dark:border-gray-700/50
                         text-sm text-text dark:text-text-dark outline-none text-center
                         focus:border-violet-400 dark:focus:border-violet-500/50 focus:ring-1 focus:ring-violet-400/20
                         transition-all"
                />
              </div>

              <!-- Graph depth -->
              <div>
                <label class="block text-sm font-medium text-text dark:text-text-dark mb-1.5">
                  {{ t('syn.graph_depth') }}
                </label>
                <input
                  v-model.number="settings.graph_expansion_depth"
                  type="number"
                  min="0"
                  max="3"
                  class="w-20 px-3 py-2 rounded-lg bg-gray-50 dark:bg-white/5 border border-gray-200 dark:border-gray-700/50
                         text-sm text-text dark:text-text-dark outline-none text-center
                         focus:border-violet-400 dark:focus:border-violet-500/50 focus:ring-1 focus:ring-violet-400/20
                         transition-all"
                />
              </div>
            </div>
          </section>

          <!-- HOW WE WORK TOGETHER
               This used to be a three-voice picker and a textarea. Both are
               gone from here.

               The picker chose one of three voices, two of which hard-coded
               Vietnamese and a pronoun pair; the third was the rule that makes
               a bilingual app work, and it is unconditional in the prompt now.

               The textarea edited `{vault}/SYN.md`, which has its own screen in
               the sidebar. Two places to edit one contract is how the two come
               to disagree — and the file was created precisely to stop the
               thing that shapes every answer from living in a settings
               modal. -->
          <section>
            <h3 class="text-[11px] font-bold uppercase tracking-wider text-gray-400 dark:text-gray-500 mb-3">
              {{ t('syn.settings_instructions') }}
            </h3>
            <p class="text-xs text-gray-500 dark:text-gray-400 leading-relaxed">
              {{ t('syn.instructions_moved') }}
            </p>
          </section>
        </div>

        <!-- Footer actions -->
        <div class="flex items-center gap-3 px-6 py-4 border-t border-gray-100 dark:border-gray-800/60">
          <button
            @click="handleSave"
            :disabled="isSaving"
            class="flex-1 flex items-center justify-center gap-2 px-4 py-2.5 rounded-xl
                   bg-gradient-to-r from-violet-500 to-purple-600 text-white font-medium text-sm
                   shadow-lg shadow-violet-500/20 hover:shadow-violet-500/30
                   hover:from-violet-600 hover:to-purple-700
                   transition-all cursor-pointer disabled:opacity-50 disabled:cursor-not-allowed"
          >
            <Loader2 v-if="isSaving" class="w-4 h-4 animate-spin" />
            <Save v-else class="w-4 h-4" />
            <span>{{ t('syn.save_settings') }}</span>
          </button>
          <button
            @click="handleReset"
            class="flex items-center gap-2 px-4 py-2.5 rounded-xl
                   bg-gray-100 dark:bg-white/5 text-gray-600 dark:text-gray-300
                   hover:bg-gray-200 dark:hover:bg-white/10
                   font-medium text-sm transition-all cursor-pointer"
          >
            <RotateCcw class="w-4 h-4" />
            <span>{{ t('syn.reset_defaults') }}</span>
          </button>
        </div>
      </div>
    </Transition>
  </Teleport>
</template>
