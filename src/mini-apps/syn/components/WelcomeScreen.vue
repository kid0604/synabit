<script setup lang="ts">
import { ref, computed } from 'vue';
import { Sparkles, Download, ExternalLink, RefreshCw, AlertCircle, CalendarDays, ListTodo, PenLine, HelpCircle } from 'lucide-vue-next';
import synAvatar from '../../../assets/syn-avatar.jpg';
import { useI18n } from 'vue-i18n';
import { invoke } from '@tauri-apps/api/core';
import { openUrl } from '@tauri-apps/plugin-opener';
import type { OllamaStatus, ModelInfo } from '../types';

defineProps<{
  status: OllamaStatus;
  models: ModelInfo[];
  pullingModel: boolean;
  pullProgress: number;
  pullError: string | null;
  isPolling: boolean;
}>();

const emit = defineEmits<{
  'new-chat': [];
  'quick-action': [prompt: string];
  'pull-model': [name: string];
  'retry': [];
}>();

const { t } = useI18n();

const customModelName = ref('');
const pendingPullName = ref<string | null>(null);

const handlePullCustom = () => {
  const name = customModelName.value.trim();
  if (!name) return;
  pendingPullName.value = name;
};

const confirmPull = () => {
  if (!pendingPullName.value) return;
  emit('pull-model', pendingPullName.value);
  customModelName.value = '';
  pendingPullName.value = null;
};

const cancelConfirm = () => {
  pendingPullName.value = null;
};

const quickActions = computed(() => [
  { icon: CalendarDays, label: t('syn.quick_weekly_summary'), prompt: t('syn.quick_weekly_summary'), color: 'text-violet-500' },
  { icon: ListTodo, label: t('syn.quick_tasks_overdue'), prompt: t('syn.quick_tasks_overdue'), color: 'text-amber-500' },
  { icon: PenLine, label: t('syn.quick_write_note'), prompt: t('syn.quick_write_note'), color: 'text-emerald-500' },
  { icon: HelpCircle, label: t('syn.quick_ask_vault'), prompt: t('syn.quick_ask_vault'), color: 'text-blue-500' },
]);

const suggestedModels = [
  { name: 'gemma4:12b', label: 'Gemma 4 12B', description: 'Google\'s latest, native tool calling', tier: 'recommended' as const },
  { name: 'qwen3:8b', label: 'Qwen 3 8B', description: 'Great for multilingual + reasoning', tier: 'recommended' as const },
  { name: 'llama3.3:8b', label: 'Llama 3.3 8B', description: 'Meta\'s versatile model', tier: 'alternative' as const },
  { name: 'mistral:7b', label: 'Mistral 7B', description: 'Fast and lightweight', tier: 'alternative' as const },
  { name: 'phi4:14b', label: 'Phi-4 14B', description: 'Microsoft\'s reasoning model', tier: 'alternative' as const },
  { name: 'deepseek-r1:8b', label: 'DeepSeek R1 8B', description: 'Strong reasoning + coding', tier: 'alternative' as const },
];

const cancelPull = async () => {
  await invoke('syn_cancel_pull');
};

const openOllamaWebsite = async () => {
  try {
    await openUrl('https://ollama.com');
  } catch (e) {
    // Fallback for dev mode
    window.open('https://ollama.com', '_blank');
  }
};
</script>

<template>
  <div class="flex-1 flex flex-col items-center justify-center px-8 py-12">
    <div class="max-w-lg w-full flex flex-col items-center text-center">
      <!-- Logo -->
      <div class="relative mb-6">
        <div class="w-20 h-20 rounded-3xl overflow-hidden shadow-2xl shadow-violet-500/30 ring-2 ring-violet-500/20">
          <img :src="synAvatar" alt="Syn" class="w-full h-full object-cover" />
        </div>
        <div class="absolute -right-1 -top-1 w-6 h-6 rounded-full bg-gradient-to-r from-amber-400 to-orange-500 flex items-center justify-center shadow-lg">
          <Sparkles class="w-3.5 h-3.5 text-white" />
        </div>
      </div>

      <!-- State: Ollama not connected -->
      <template v-if="!status.connected">
        <h2 class="text-2xl font-bold text-text dark:text-text-dark mb-2">
          {{ $t('syn.setup_title') }}
        </h2>
        <p class="text-sm text-gray-500 dark:text-gray-400 mb-6 leading-relaxed max-w-md">
          {{ $t('syn.setup_description') }}
        </p>

        <!-- Steps (simplified: 2 steps instead of 3) -->
        <div class="w-full space-y-3 mb-6">
          <!-- Step 1: Install Ollama -->
          <div class="flex items-start gap-3 bg-white dark:bg-[#1e1f25] border border-gray-100 dark:border-gray-800/60 rounded-xl p-4 text-left">
            <div class="w-7 h-7 rounded-full bg-violet-100 dark:bg-violet-500/20 flex items-center justify-center flex-shrink-0 mt-0.5">
              <span class="text-sm font-bold text-violet-600 dark:text-violet-400">1</span>
            </div>
            <div>
              <p class="text-sm font-medium text-text dark:text-text-dark">{{ $t('syn.setup_step1_title') }}</p>
              <p class="text-xs text-gray-400 dark:text-gray-500 mt-0.5">{{ $t('syn.setup_step1_desc') }}</p>
            </div>
          </div>

          <!-- Step 2: Open Ollama app -->
          <div class="flex items-start gap-3 bg-white dark:bg-[#1e1f25] border border-gray-100 dark:border-gray-800/60 rounded-xl p-4 text-left">
            <div class="w-7 h-7 rounded-full bg-violet-100 dark:bg-violet-500/20 flex items-center justify-center flex-shrink-0 mt-0.5">
              <span class="text-sm font-bold text-violet-600 dark:text-violet-400">2</span>
            </div>
            <div>
              <p class="text-sm font-medium text-text dark:text-text-dark">{{ $t('syn.setup_step2_title') }}</p>
              <p class="text-xs text-gray-400 dark:text-gray-500 mt-0.5">{{ $t('syn.setup_step2_desc') }}</p>
            </div>
          </div>
        </div>

        <!-- Primary CTA: Download Ollama -->
        <button
          @click="openOllamaWebsite"
          class="flex items-center gap-2 px-5 py-2.5 rounded-xl bg-gradient-to-r from-violet-500 to-purple-600 text-white font-medium text-sm shadow-lg shadow-violet-500/20 hover:shadow-violet-500/30 transition-all cursor-pointer"
        >
          <ExternalLink class="w-4 h-4" />
          {{ $t('syn.download_ollama') }}
        </button>

        <!-- Polling indicator -->
        <div class="flex items-center gap-2 mt-5 text-sm text-gray-400 dark:text-gray-500">
          <div class="w-2 h-2 rounded-full bg-violet-500 animate-pulse" />
          <span>{{ $t('syn.checking_connection') }}</span>
        </div>

        <!-- Manual retry button -->
        <button
          @click="emit('retry')"
          class="mt-2 text-sm text-violet-400 hover:text-violet-300 transition-colors flex items-center gap-1.5 cursor-pointer"
        >
          <RefreshCw class="w-3.5 h-3.5" />
          {{ $t('syn.retry_connection') }}
        </button>
      </template>

      <!-- State: Connected but no models -->
      <template v-else-if="models.length === 0">
        <h2 class="text-2xl font-bold text-text dark:text-text-dark mb-2">
          {{ $t('syn.no_models_title') }}
        </h2>
        <p class="text-sm text-gray-500 dark:text-gray-400 mb-6 leading-relaxed max-w-md">
          {{ $t('syn.no_models_description') }}
        </p>

        <!-- Suggested models -->
        <div class="w-full space-y-2 mb-6">
          <button
            v-for="model in suggestedModels"
            :key="model.name"
            @click="emit('pull-model', model.name)"
            :disabled="pullingModel"
            class="w-full flex items-center gap-3 bg-white dark:bg-[#1e1f25] border border-gray-100 dark:border-gray-800/60 rounded-xl p-4 text-left hover:border-violet-300 dark:hover:border-violet-500/30 transition-all cursor-pointer group disabled:opacity-50"
          >
            <div class="w-10 h-10 rounded-xl bg-gradient-to-br from-violet-500/10 to-purple-500/10 dark:from-violet-500/20 dark:to-purple-500/20 flex items-center justify-center flex-shrink-0">
              <Download class="w-5 h-5 text-violet-500 group-hover:scale-110 transition-transform" />
            </div>
            <div class="flex-1">
              <p class="text-sm font-semibold text-text dark:text-text-dark">{{ model.name }}</p>
              <p class="text-xs text-gray-400 dark:text-gray-500">{{ model.description }}</p>
            </div>
          </button>
        </div>
        <!-- Custom model input -->
        <div v-if="!pendingPullName" class="w-full flex items-center gap-2 mb-6">
          <div class="relative flex-1">
            <input
              v-model="customModelName"
              :disabled="pullingModel"
              @keydown.enter="handlePullCustom"
              :placeholder="$t('syn.custom_model_placeholder')"
              class="w-full px-4 py-2.5 text-sm bg-white dark:bg-[#1e1f25] border border-gray-200 dark:border-gray-700/60 rounded-xl text-text dark:text-text-dark placeholder-gray-400 dark:placeholder-gray-500 outline-none focus:border-violet-400 dark:focus:border-violet-500/50 transition-colors disabled:opacity-50"
            />
          </div>
          <button
            @click="handlePullCustom"
            :disabled="!customModelName.trim() || pullingModel"
            class="px-4 py-2.5 rounded-xl text-sm font-medium transition-all cursor-pointer flex-shrink-0"
            :class="customModelName.trim() && !pullingModel
              ? 'bg-gradient-to-r from-violet-500 to-purple-600 text-white shadow-lg shadow-violet-500/20 hover:shadow-violet-500/30'
              : 'bg-gray-100 dark:bg-gray-800 text-gray-400 dark:text-gray-500 cursor-not-allowed'"
          >
            <Download class="w-4 h-4" />
          </button>
        </div>

        <!-- Confirm pull -->
        <div v-else class="w-full bg-white dark:bg-[#1e1f25] border border-violet-300 dark:border-violet-500/30 rounded-xl p-4 mb-6">
          <p class="text-sm text-text dark:text-text-dark mb-1">
            {{ $t('syn.confirm_pull') }}
          </p>
          <p class="text-base font-semibold text-violet-600 dark:text-violet-400 mb-3">
            {{ pendingPullName }}
          </p>
          <div class="flex items-center gap-2">
            <button
              @click="confirmPull"
              class="flex-1 px-4 py-2 rounded-lg bg-gradient-to-r from-violet-500 to-purple-600 text-white text-sm font-medium shadow-lg shadow-violet-500/20 hover:shadow-violet-500/30 transition-all cursor-pointer"
            >
              {{ $t('syn.confirm_pull_btn') }}
            </button>
            <button
              @click="cancelConfirm"
              class="px-4 py-2 rounded-lg text-sm text-gray-500 dark:text-gray-400 hover:bg-gray-100 dark:hover:bg-white/5 transition-colors cursor-pointer"
            >
              {{ $t('syn.cancel') }}
            </button>
          </div>
        </div>

        <!-- Pull progress -->
        <div v-if="pullingModel" class="w-full">
          <div class="w-full bg-gray-100 dark:bg-gray-800 rounded-full h-2 overflow-hidden">
            <div
              class="h-full bg-gradient-to-r from-violet-500 to-purple-600 rounded-full transition-all duration-300"
              :style="{ width: pullProgress + '%' }"
            />
          </div>
          <p class="text-xs text-gray-400 dark:text-gray-500 mt-2 text-center">
            {{ $t('syn.pulling_model') }} {{ Math.round(pullProgress) }}%
          </p>
          <button
            @click="cancelPull"
            class="mt-2 text-xs text-red-400 hover:text-red-300 transition-colors"
          >
            {{ t('syn.cancel_pull') }}
          </button>
        </div>

        <!-- Pull error -->
        <div
          v-if="pullError && !pullingModel"
          class="w-full flex items-start gap-3 px-4 py-3 bg-red-500/5 border border-red-500/20 rounded-xl text-sm mt-4"
        >
          <AlertCircle class="w-4 h-4 text-red-400 flex-shrink-0 mt-0.5" />
          <div>
            <p class="text-red-400 font-medium">{{ $t('syn.pull_error_title') }}</p>
            <p class="text-red-400/70 text-xs mt-0.5">{{ $t('syn.pull_error_desc') }}</p>
          </div>
        </div>
      </template>

      <!-- State: Ready -->
      <template v-else>
        <h2 class="text-2xl font-bold text-text dark:text-text-dark mb-1">
          {{ $t('syn.greeting') }}
        </h2>
        <p class="text-sm text-gray-500 dark:text-gray-400 mb-8 leading-relaxed max-w-md">
          {{ $t('syn.greeting_sub') }}
        </p>

        <!-- Quick actions -->
        <div class="grid grid-cols-2 gap-3 w-full">
          <button
            v-for="action in quickActions"
            :key="action.label"
            @click="emit('quick-action', action.prompt)"
            class="flex items-start gap-3 bg-white dark:bg-[#1e1f25] border border-gray-100 dark:border-gray-800/60 rounded-xl p-4 text-left hover:border-violet-300 dark:hover:border-violet-500/30 hover:shadow-md hover:shadow-violet-500/5 transition-all cursor-pointer group"
          >
            <component
              :is="action.icon"
              class="w-5 h-5 flex-shrink-0 mt-0.5 transition-transform group-hover:scale-110"
              :class="action.color"
            />
            <span class="text-sm text-text dark:text-text-dark font-medium leading-snug">
              {{ action.label }}
            </span>
          </button>
        </div>
      </template>
    </div>
  </div>
</template>
