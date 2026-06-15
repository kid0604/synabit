<script setup lang="ts">
import { ref, computed } from 'vue';
import { ChevronDown, Check, Cpu } from 'lucide-vue-next';
import type { ModelInfo } from '../types';

const props = defineProps<{
  models: ModelInfo[];
  modelValue: string;
  formatSize: (bytes: number) => string;
}>();

const emit = defineEmits<{
  'update:modelValue': [value: string];
}>();

const isOpen = ref(false);

const selectedModelInfo = computed(() => {
  return props.models.find(m => m.name === props.modelValue);
});

const displayName = computed(() => {
  if (!selectedModelInfo.value) return '';
  const name = selectedModelInfo.value.name;
  // Show short name: e.g., "gemma2:7b" → "Gemma2 7B"
  return name.split(':').map(p => p.charAt(0).toUpperCase() + p.slice(1)).join(' ');
});

const selectModel = (name: string) => {
  emit('update:modelValue', name);
  isOpen.value = false;
};

const handleClickOutside = () => {
  isOpen.value = false;
};
</script>

<template>
  <div class="relative" v-if="models.length > 0">
    <!-- Trigger -->
    <button
      @click.stop="isOpen = !isOpen"
      class="flex items-center gap-2 px-3 py-1.5 rounded-lg bg-white/60 dark:bg-white/5 border border-border dark:border-border-dark hover:bg-white dark:hover:bg-white/10 transition-all cursor-pointer text-sm"
    >
      <Cpu class="w-3.5 h-3.5 text-violet-500" />
      <span class="font-medium text-text dark:text-text-dark max-w-[140px] truncate">
        {{ displayName || $t('syn.select_model') }}
      </span>
      <span
        v-if="selectedModelInfo?.details?.parameter_size"
        class="text-xs text-gray-400 dark:text-gray-500"
      >
        {{ selectedModelInfo.details.parameter_size }}
      </span>
      <ChevronDown class="w-3.5 h-3.5 text-gray-400 transition-transform" :class="{ 'rotate-180': isOpen }" />
    </button>

    <!-- Dropdown overlay -->
    <div v-if="isOpen" class="fixed inset-0 z-40" @click="handleClickOutside" />

    <!-- Dropdown -->
    <Transition
      enter-active-class="transition ease-out duration-150"
      enter-from-class="opacity-0 scale-95 -translate-y-1"
      enter-to-class="opacity-100 scale-100 translate-y-0"
      leave-active-class="transition ease-in duration-100"
      leave-from-class="opacity-100 scale-100 translate-y-0"
      leave-to-class="opacity-0 scale-95 -translate-y-1"
    >
      <div
        v-if="isOpen"
        class="absolute right-0 top-full mt-2 w-72 bg-white dark:bg-[#1a1a1f] border border-border dark:border-border-dark rounded-xl shadow-xl z-50 overflow-hidden"
      >
        <div class="p-2 border-b border-border dark:border-border-dark">
          <p class="text-xs font-medium text-gray-400 dark:text-gray-500 uppercase tracking-wider px-2 py-1">
            {{ $t('syn.installed_models') }}
          </p>
        </div>
        <div class="max-h-64 overflow-y-auto p-1">
          <button
            v-for="model in models"
            :key="model.name"
            @click="selectModel(model.name)"
            class="w-full flex items-center gap-3 px-3 py-2.5 rounded-lg text-left transition-colors cursor-pointer"
            :class="model.name === modelValue
              ? 'bg-violet-50 dark:bg-violet-500/10 text-violet-700 dark:text-violet-300'
              : 'hover:bg-gray-50 dark:hover:bg-white/5 text-text dark:text-text-dark'"
          >
            <div class="w-8 h-8 rounded-lg bg-gradient-to-br from-violet-500/10 to-purple-500/10 dark:from-violet-500/20 dark:to-purple-500/20 flex items-center justify-center flex-shrink-0">
              <Cpu class="w-4 h-4 text-violet-500" />
            </div>
            <div class="flex-1 min-w-0">
              <div class="font-medium text-sm truncate">{{ model.name }}</div>
              <div class="flex items-center gap-2 text-xs text-gray-400 dark:text-gray-500">
                <span>{{ formatSize(model.size) }}</span>
                <span v-if="model.details?.family">· {{ model.details.family }}</span>
              </div>
            </div>
            <Check v-if="model.name === modelValue" class="w-4 h-4 text-violet-500 flex-shrink-0" />
          </button>
        </div>
      </div>
    </Transition>
  </div>
</template>
