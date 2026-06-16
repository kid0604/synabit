<script setup lang="ts">
import { computed } from 'vue';
import { Sparkles, Download, ExternalLink, CalendarDays, ListTodo, PenLine, HelpCircle } from 'lucide-vue-next';
import synAvatar from '../../../assets/syn-avatar.jpg';
import { useI18n } from 'vue-i18n';
import { invoke } from '@tauri-apps/api/core';
import type { OllamaStatus, ModelInfo } from '../types';

defineProps<{
  status: OllamaStatus;
  models: ModelInfo[];
  pullingModel: boolean;
  pullProgress: number;
}>();

const emit = defineEmits<{
  'new-chat': [];
  'quick-action': [prompt: string];
  'pull-model': [name: string];
}>();

const { t } = useI18n();

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

const openOllamaWebsite = () => {
  window.open('https://ollama.com', '_blank');
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

        <!-- Steps -->
        <div class="w-full space-y-3 mb-6">
          <div class="flex items-start gap-3 bg-white dark:bg-[#1e1f25] border border-gray-100 dark:border-gray-800/60 rounded-xl p-4 text-left">
            <div class="w-7 h-7 rounded-full bg-violet-100 dark:bg-violet-500/20 flex items-center justify-center flex-shrink-0 mt-0.5">
              <span class="text-sm font-bold text-violet-600 dark:text-violet-400">1</span>
            </div>
            <div>
              <p class="text-sm font-medium text-text dark:text-text-dark">{{ $t('syn.setup_step1_title') }}</p>
              <p class="text-xs text-gray-400 dark:text-gray-500 mt-0.5">{{ $t('syn.setup_step1_desc') }}</p>
            </div>
          </div>

          <div class="flex items-start gap-3 bg-white dark:bg-[#1e1f25] border border-gray-100 dark:border-gray-800/60 rounded-xl p-4 text-left">
            <div class="w-7 h-7 rounded-full bg-violet-100 dark:bg-violet-500/20 flex items-center justify-center flex-shrink-0 mt-0.5">
              <span class="text-sm font-bold text-violet-600 dark:text-violet-400">2</span>
            </div>
            <div>
              <p class="text-sm font-medium text-text dark:text-text-dark">{{ $t('syn.setup_step2_title') }}</p>
              <p class="text-xs text-gray-400 dark:text-gray-500 mt-0.5">
                <code class="px-1.5 py-0.5 rounded bg-gray-100 dark:bg-gray-800 text-xs">ollama serve</code>
              </p>
            </div>
          </div>

          <div class="flex items-start gap-3 bg-white dark:bg-[#1e1f25] border border-gray-100 dark:border-gray-800/60 rounded-xl p-4 text-left">
            <div class="w-7 h-7 rounded-full bg-violet-100 dark:bg-violet-500/20 flex items-center justify-center flex-shrink-0 mt-0.5">
              <span class="text-sm font-bold text-violet-600 dark:text-violet-400">3</span>
            </div>
            <div>
              <p class="text-sm font-medium text-text dark:text-text-dark">{{ $t('syn.setup_step3_title') }}</p>
              <p class="text-xs text-gray-400 dark:text-gray-500 mt-0.5">{{ $t('syn.setup_step3_desc') }}</p>
            </div>
          </div>
        </div>

        <button
          @click="openOllamaWebsite"
          class="flex items-center gap-2 px-5 py-2.5 rounded-xl bg-gradient-to-r from-violet-500 to-purple-600 text-white font-medium text-sm shadow-lg shadow-violet-500/20 hover:shadow-violet-500/30 transition-all cursor-pointer"
        >
          <ExternalLink class="w-4 h-4" />
          {{ $t('syn.download_ollama') }}
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
