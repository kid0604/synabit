<script setup lang="ts">
import AppDialog from '../../../../../shared/components/AppDialog.vue';
import { Navigation as NavigationIcon } from 'lucide-vue-next';

defineProps<{
  show: boolean;
  urlInput: string;
  label: string;
  error: string;
  isValid: boolean;
}>();

const emit = defineEmits<{
  (e: 'update:urlInput', value: string): void;
  (e: 'update:label', value: string): void;
  (e: 'confirm'): void;
  (e: 'close'): void;
}>();
</script>

<template>
  <AppDialog :show="show" labelledby="note-route-title" size="sm" panel-class="p-5" @close="emit('close')">
    <div class="flex items-center gap-2 mb-4">
      <NavigationIcon class="w-4 h-4 text-accent dark:text-accent-dark" />
      <h3 id="note-route-title" class="text-sm font-semibold text-gray-800 dark:text-gray-200">{{ $t('note.editor.route.title') }}</h3>
    </div>

    <!-- URL Input -->
    <div class="mb-3">
      <label class="text-xs font-semibold text-gray-500 dark:text-gray-400 uppercase tracking-wider mb-1 block">{{ $t('note.editor.route.url_label') }}</label>
      <input
        :value="urlInput"
        @input="emit('update:urlInput', ($event.target as HTMLInputElement).value)"
        type="text"
        :placeholder="$t('note.editor.route.url_placeholder')"
        class="w-full px-3 py-2 text-sm rounded-lg border border-border-subtle dark:border-[#444] bg-[#fafafa] dark:bg-[#252525] text-gray-800 dark:text-gray-200 outline-none focus:border-accent transition-colors"
        @keydown.enter.stop="emit('confirm')"
      />
      <p class="text-xs text-gray-500 dark:text-gray-400 mt-1">{{ $t('note.editor.route.url_hint') }}</p>
    </div>

    <!-- Optional Label -->
    <div class="mb-3">
      <label class="text-xs font-semibold text-gray-500 uppercase tracking-wider mb-1 block">{{ $t('note.editor.route.label') }} <span class="text-gray-500 dark:text-gray-400">{{ $t('note.editor.optional') }}</span></label>
      <input
        :value="label"
        @input="emit('update:label', ($event.target as HTMLInputElement).value)"
        type="text"
        :placeholder="$t('note.editor.route.label_placeholder')"
        class="w-full px-3 py-2 text-sm rounded-lg border border-border-subtle dark:border-[#444] bg-[#fafafa] dark:bg-[#252525] text-gray-800 dark:text-gray-200 outline-none focus:border-accent transition-colors"
        @keydown.enter.stop="emit('confirm')"
      />
    </div>

    <div v-if="error" class="text-xs text-red-400 mb-2">{{ error }}</div>

    <div class="flex justify-end gap-2 mt-4">
      <button @click="emit('close')" class="px-4 py-1.5 text-sm rounded-lg text-gray-500 dark:text-gray-400 hover:bg-gray-100 dark:hover:bg-[#333] transition-colors">{{ $t('note.cancel') }}</button>
      <button
        @click="emit('confirm')"
        :disabled="!isValid"
        class="btn-primary"
      >
        <NavigationIcon class="w-3.5 h-3.5" />
        {{ $t('note.editor.route.insert') }}
      </button>
    </div>
  </AppDialog>
</template>
