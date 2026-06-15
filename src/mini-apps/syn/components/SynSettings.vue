<script setup lang="ts">
import { onMounted, onUnmounted, watch } from 'vue';
import { useI18n } from 'vue-i18n';
import { X, RotateCcw, Save, Loader2 } from 'lucide-vue-next';
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
  isLoading,
  isSaving,
  loadSettings,
  saveSettings,
  resetToDefaults,
} = useSynSettings(props.vaultPath);

const handleSave = async () => {
  await saveSettings();
  emit('saved');
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
    <Transition
      enter-active-class="transition-opacity duration-200 ease-out"
      enter-from-class="opacity-0"
      enter-to-class="opacity-100"
      leave-active-class="transition-opacity duration-150 ease-in"
      leave-from-class="opacity-100"
      leave-to-class="opacity-0"
    >
      <div
        class="fixed inset-0 z-[998] bg-black/40 backdrop-blur-sm"
        @click="emit('close')"
      />
    </Transition>

    <!-- Panel -->
    <Transition
      enter-active-class="transition-transform duration-250 ease-out"
      enter-from-class="translate-x-full"
      enter-to-class="translate-x-0"
      leave-active-class="transition-transform duration-200 ease-in"
      leave-from-class="translate-x-0"
      leave-to-class="translate-x-full"
    >
      <div
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
          >
            <X class="w-5 h-5" />
          </button>
        </div>

        <!-- Loading -->
        <div v-if="isLoading" class="flex-1 flex items-center justify-center">
          <Loader2 class="w-6 h-6 text-violet-500 animate-spin" />
        </div>

        <!-- Settings content -->
        <div v-else class="flex-1 overflow-y-auto px-6 py-5 space-y-6">
          <!-- CONNECTION -->
          <section>
            <h3 class="text-[11px] font-bold uppercase tracking-wider text-gray-400 dark:text-gray-500 mb-3">
              {{ t('syn.settings_connection') }}
            </h3>
            <div class="space-y-3">
              <!-- Ollama URL -->
              <div>
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

              <!-- Default Model -->
              <div>
                <label class="block text-sm font-medium text-text dark:text-text-dark mb-1.5">
                  {{ t('syn.default_model') }}
                </label>
                <select
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
                >
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
                >
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
                >
                  <span
                    class="absolute top-0.5 left-0.5 w-5 h-5 rounded-full bg-white shadow-sm transition-transform duration-200"
                    :class="settings.include_feeds ? 'translate-x-5' : 'translate-x-0'"
                  />
                </button>
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

          <!-- PERSONALITY -->
          <section>
            <h3 class="text-[11px] font-bold uppercase tracking-wider text-gray-400 dark:text-gray-500 mb-3">
              {{ t('syn.settings_personality') }}
            </h3>
            <div class="space-y-3">
              <!-- Radio buttons -->
              <div class="space-y-2">
                <label
                  class="flex items-center gap-3 px-3 py-2.5 rounded-lg cursor-pointer transition-all
                         hover:bg-gray-50 dark:hover:bg-white/5"
                  :class="settings.personality === 'auto' ? 'bg-violet-50 dark:bg-violet-500/10 ring-1 ring-violet-300 dark:ring-violet-500/30' : ''"
                >
                  <input
                    v-model="settings.personality"
                    type="radio"
                    value="auto"
                    class="sr-only"
                  />
                  <div
                    class="w-4 h-4 rounded-full border-2 flex items-center justify-center transition-colors"
                    :class="settings.personality === 'auto'
                      ? 'border-violet-500'
                      : 'border-gray-300 dark:border-gray-600'"
                  >
                    <div
                      v-if="settings.personality === 'auto'"
                      class="w-2 h-2 rounded-full bg-violet-500"
                    />
                  </div>
                  <span class="text-sm text-text dark:text-text-dark">{{ t('syn.personality_auto') }}</span>
                </label>

                <label
                  class="flex items-center gap-3 px-3 py-2.5 rounded-lg cursor-pointer transition-all
                         hover:bg-gray-50 dark:hover:bg-white/5"
                  :class="settings.personality === 'casual' ? 'bg-violet-50 dark:bg-violet-500/10 ring-1 ring-violet-300 dark:ring-violet-500/30' : ''"
                >
                  <input
                    v-model="settings.personality"
                    type="radio"
                    value="casual"
                    class="sr-only"
                  />
                  <div
                    class="w-4 h-4 rounded-full border-2 flex items-center justify-center transition-colors"
                    :class="settings.personality === 'casual'
                      ? 'border-violet-500'
                      : 'border-gray-300 dark:border-gray-600'"
                  >
                    <div
                      v-if="settings.personality === 'casual'"
                      class="w-2 h-2 rounded-full bg-violet-500"
                    />
                  </div>
                  <span class="text-sm text-text dark:text-text-dark">{{ t('syn.personality_casual') }}</span>
                </label>

                <label
                  class="flex items-center gap-3 px-3 py-2.5 rounded-lg cursor-pointer transition-all
                         hover:bg-gray-50 dark:hover:bg-white/5"
                  :class="settings.personality === 'professional' ? 'bg-violet-50 dark:bg-violet-500/10 ring-1 ring-violet-300 dark:ring-violet-500/30' : ''"
                >
                  <input
                    v-model="settings.personality"
                    type="radio"
                    value="professional"
                    class="sr-only"
                  />
                  <div
                    class="w-4 h-4 rounded-full border-2 flex items-center justify-center transition-colors"
                    :class="settings.personality === 'professional'
                      ? 'border-violet-500'
                      : 'border-gray-300 dark:border-gray-600'"
                  >
                    <div
                      v-if="settings.personality === 'professional'"
                      class="w-2 h-2 rounded-full bg-violet-500"
                    />
                  </div>
                  <span class="text-sm text-text dark:text-text-dark">{{ t('syn.personality_professional') }}</span>
                </label>
              </div>

              <!-- Custom system prompt -->
              <div>
                <label class="block text-sm font-medium text-text dark:text-text-dark mb-1.5">
                  {{ t('syn.custom_system_prompt') }}
                </label>
                <textarea
                  v-model="settings.custom_system_prompt"
                  :placeholder="t('syn.custom_system_prompt_placeholder')"
                  rows="3"
                  class="w-full px-3 py-2.5 rounded-lg bg-gray-50 dark:bg-white/5 border border-gray-200 dark:border-gray-700/50
                         text-sm text-text dark:text-text-dark placeholder-gray-400 dark:placeholder-gray-500
                         outline-none resize-none
                         focus:border-violet-400 dark:focus:border-violet-500/50 focus:ring-1 focus:ring-violet-400/20
                         transition-all"
                />
              </div>
            </div>
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
