<script setup lang="ts">
import AppDialog from '../../../../../shared/components/AppDialog.vue';
import { MapPin as MapPinIcon } from 'lucide-vue-next';

export interface LocationModalState {
  show: boolean;
  input: string;
  lat: number | null;
  lng: number | null;
  label: string;
  provider: 'osm' | 'google';
  searching: boolean;
  suggestions: { display: string; lat: number; lng: number }[];
  error: string;
}

const props = defineProps<{
  modelValue: LocationModalState;
}>();

const emit = defineEmits<{
  (e: 'update:modelValue', value: LocationModalState): void;
  (e: 'input', value: string): void;
  (e: 'select-suggestion', s: { display: string; lat: number; lng: number }): void;
  (e: 'confirm'): void;
  (e: 'close'): void;
}>();

const updateField = <K extends keyof LocationModalState>(key: K, value: LocationModalState[K]) => {
  emit('update:modelValue', { ...props.modelValue, [key]: value });
};
</script>

<template>
  <AppDialog :show="modelValue.show" labelledby="note-location-title" size="sm" panel-class="p-6" @close="emit('close')">
    <h3 id="note-location-title" class="text-base font-semibold text-text dark:text-text-dark mb-1 flex items-center gap-2">
      <MapPinIcon class="w-4 h-4 text-red-500" />
      {{ $t('note.editor.location.title') }}
    </h3>
    <p class="text-xs text-gray-500 dark:text-gray-400 mb-4">{{ $t('note.editor.location.hint') }}</p>
    
    <div class="space-y-3">
      <!-- Input -->
      <div class="relative">
        <input
          :value="modelValue.input"
          @input="(e: Event) => emit('input', (e.target as HTMLInputElement).value)"
          type="text"
          :placeholder="$t('note.editor.location.input_placeholder')"
          class="w-full px-3 py-2.5 rounded-lg border border-border-subtle dark:border-[#444] bg-white dark:bg-surface-dark text-text dark:text-text-dark text-sm focus:outline-none focus:ring-2 focus:ring-accent/20 focus:border-accent pr-8"
          @keydown.enter="emit('confirm')"
          autofocus
        />
        <div v-if="modelValue.searching" class="absolute right-3 top-1/2 -translate-y-1/2">
          <div class="w-4 h-4 border-2 border-gray-300 dark:border-gray-600 border-t-red-500 rounded-full animate-spin" />
        </div>
      </div>

      <!-- Suggestions -->
      <div v-if="modelValue.suggestions.length > 0" class="rounded-lg border border-border-subtle dark:border-[#444] bg-[#fafafa] dark:bg-[#1a1a1a] max-h-[160px] overflow-y-auto">
        <button
          v-for="(s, i) in modelValue.suggestions"
          :key="i"
          class="w-full text-left px-3 py-2 text-xs text-[#374151] dark:text-[#d4d4d8] hover:bg-[#f3f4f6] dark:hover:bg-[#252525] transition-colors border-b border-[#f3f4f6] dark:border-[#2a2a2a] last:border-0 flex items-start gap-2"
          @click="emit('select-suggestion', s)"
        >
          <MapPinIcon class="w-3 h-3 text-red-400 flex-shrink-0 mt-0.5" />
          <span class="line-clamp-2">{{ s.display }}</span>
        </button>
      </div>

      <!-- Error -->
      <p v-if="modelValue.error" class="text-xs text-red-500">{{ modelValue.error }}</p>

      <!-- Resolved coordinates preview -->
      <div v-if="modelValue.lat !== null && modelValue.lng !== null" class="flex items-center gap-2 px-3 py-2 rounded-lg bg-green-50 dark:bg-green-900/20 border border-green-200 dark:border-green-800/30">
        <MapPinIcon class="w-3.5 h-3.5 text-green-600 dark:text-green-400 flex-shrink-0" />
        <div class="flex-1 min-w-0">
          <span class="text-xs font-medium text-green-700 dark:text-green-300">{{ modelValue.lat.toFixed(5) }}, {{ modelValue.lng.toFixed(5) }}</span>
        </div>
      </div>

      <!-- Label -->
      <div v-if="modelValue.lat !== null">
        <label class="block text-xs font-medium text-gray-500 dark:text-gray-400 mb-1">{{ $t('note.editor.location.label_optional') }}</label>
        <input
          :value="modelValue.label"
          @input="updateField('label', ($event.target as HTMLInputElement).value)"
          type="text"
          :placeholder="$t('note.editor.location.label_placeholder')"
          class="w-full px-3 py-2 rounded-lg border border-border-subtle dark:border-[#444] bg-white dark:bg-surface-dark text-text dark:text-text-dark text-sm focus:outline-none focus:ring-2 focus:ring-black/10 dark:focus:ring-white/20"
          @keydown.enter="emit('confirm')"
        />
      </div>

      <!-- Map Provider -->
      <div v-if="modelValue.lat !== null">
        <label class="block text-xs font-medium text-gray-500 dark:text-gray-400 mb-1.5">{{ $t('note.editor.location.provider') }}</label>
        <div class="flex gap-2">
          <button
            @click="updateField('provider', 'osm')"
            class="flex-1 py-2 px-3 rounded-lg text-xs font-medium transition-all border"
            :class="modelValue.provider === 'osm'
              ? 'bg-emerald-50 dark:bg-emerald-900/20 border-emerald-300 dark:border-emerald-700 text-emerald-700 dark:text-emerald-300'
              : 'bg-[#fafafa] dark:bg-[#1a1a1a] border-border-subtle dark:border-[#444] text-gray-500 dark:text-gray-400 hover:bg-[#f3f4f6] dark:hover:bg-[#252525]'"
          >
            🗺️ OpenStreetMap
            <span class="block text-xs mt-0.5 opacity-70">{{ $t('note.editor.location.osm_hint') }}</span>
          </button>
          <button
            @click="updateField('provider', 'google')"
            class="flex-1 py-2 px-3 rounded-lg text-xs font-medium transition-all border"
            :class="modelValue.provider === 'google'
              ? 'bg-accent/10 border-accent/40 text-accent dark:text-accent-dark'
              : 'bg-[#fafafa] dark:bg-[#1a1a1a] border-border-subtle dark:border-[#444] text-gray-500 dark:text-gray-400 hover:bg-[#f3f4f6] dark:hover:bg-[#252525]'"
          >
            📍 Google Maps
            <span class="block text-xs mt-0.5 opacity-70">{{ $t('note.editor.location.google_hint') }}</span>
          </button>
        </div>
      </div>
    </div>
    
    <div class="flex justify-end gap-2 mt-5">
      <button @click="emit('close')" class="px-4 py-1.5 text-sm rounded-lg text-gray-500 dark:text-gray-400 hover:bg-gray-100 dark:hover:bg-[#333] transition-colors">{{ $t('note.cancel') }}</button>
      <button
        @click="emit('confirm')"
        :disabled="modelValue.lat === null || modelValue.lng === null"
        class="btn-primary"
      >
        <MapPinIcon class="w-3.5 h-3.5" />
        {{ $t('note.editor.insert') }}
      </button>
    </div>
  </AppDialog>
</template>
