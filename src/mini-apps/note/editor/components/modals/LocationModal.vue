<script setup lang="ts">
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
  <Teleport to="body">
    <div v-if="modelValue.show" class="fixed inset-0 z-[999] flex items-center justify-center bg-black/40 backdrop-blur-sm" @click.self="emit('close')">
      <div class="bg-white dark:bg-[#2a2a2a] rounded-2xl shadow-2xl p-6 w-[420px] border border-[#e6e6e6] dark:border-[#3a3a3a]">
        <h3 class="text-base font-semibold text-[#1c1c1e] dark:text-[#f4f4f5] mb-1 flex items-center gap-2">
          <MapPinIcon class="w-4 h-4 text-red-500" />
          Insert Location
        </h3>
        <p class="text-xs text-gray-400 dark:text-gray-500 mb-4">Paste a map URL, enter coordinates, or search an address</p>
        
        <div class="space-y-3">
          <!-- Input -->
          <div class="relative">
            <input
              :value="modelValue.input"
              @input="(e: Event) => emit('input', (e.target as HTMLInputElement).value)"
              type="text"
              placeholder="Paste URL, lat/lng, or type an address..."
              class="w-full px-3 py-2.5 rounded-lg border border-[#e0e0e0] dark:border-[#444] bg-white dark:bg-[#1e1e1e] text-[#1c1c1e] dark:text-[#f4f4f5] text-sm focus:outline-none focus:ring-2 focus:ring-red-500/20 dark:focus:ring-red-400/20 focus:border-red-400 dark:focus:border-red-500 pr-8"
              @keydown.enter="emit('confirm')"
              autofocus
            />
            <div v-if="modelValue.searching" class="absolute right-3 top-1/2 -translate-y-1/2">
              <div class="w-4 h-4 border-2 border-gray-300 dark:border-gray-600 border-t-red-500 rounded-full animate-spin" />
            </div>
          </div>

          <!-- Suggestions -->
          <div v-if="modelValue.suggestions.length > 0" class="rounded-lg border border-[#e0e0e0] dark:border-[#444] bg-[#fafafa] dark:bg-[#1a1a1a] max-h-[160px] overflow-y-auto">
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
            <label class="block text-xs font-medium text-gray-500 dark:text-gray-400 mb-1">Label (optional)</label>
            <input
              :value="modelValue.label"
              @input="updateField('label', ($event.target as HTMLInputElement).value)"
              type="text"
              placeholder="e.g., Hanoi Opera House"
              class="w-full px-3 py-2 rounded-lg border border-[#e0e0e0] dark:border-[#444] bg-white dark:bg-[#1e1e1e] text-[#1c1c1e] dark:text-[#f4f4f5] text-sm focus:outline-none focus:ring-2 focus:ring-black/10 dark:focus:ring-white/20"
              @keydown.enter="emit('confirm')"
            />
          </div>

          <!-- Map Provider -->
          <div v-if="modelValue.lat !== null">
            <label class="block text-xs font-medium text-gray-500 dark:text-gray-400 mb-1.5">Map Provider</label>
            <div class="flex gap-2">
              <button
                @click="updateField('provider', 'osm')"
                class="flex-1 py-2 px-3 rounded-lg text-xs font-medium transition-all border"
                :class="modelValue.provider === 'osm'
                  ? 'bg-emerald-50 dark:bg-emerald-900/20 border-emerald-300 dark:border-emerald-700 text-emerald-700 dark:text-emerald-300'
                  : 'bg-[#fafafa] dark:bg-[#1a1a1a] border-[#e0e0e0] dark:border-[#444] text-gray-500 dark:text-gray-400 hover:bg-[#f3f4f6] dark:hover:bg-[#252525]'"
              >
                🗺️ OpenStreetMap
                <span class="block text-[10px] mt-0.5 opacity-70">Free · Privacy-first</span>
              </button>
              <button
                @click="updateField('provider', 'google')"
                class="flex-1 py-2 px-3 rounded-lg text-xs font-medium transition-all border"
                :class="modelValue.provider === 'google'
                  ? 'bg-blue-50 dark:bg-blue-900/20 border-blue-300 dark:border-blue-700 text-blue-700 dark:text-blue-300'
                  : 'bg-[#fafafa] dark:bg-[#1a1a1a] border-[#e0e0e0] dark:border-[#444] text-gray-500 dark:text-gray-400 hover:bg-[#f3f4f6] dark:hover:bg-[#252525]'"
              >
                📍 Google Maps
                <span class="block text-[10px] mt-0.5 opacity-70">Detailed · Satellite</span>
              </button>
            </div>
          </div>
        </div>
        
        <div class="flex justify-end gap-2 mt-5">
          <button @click="emit('close')" class="px-4 py-1.5 text-sm rounded-lg text-gray-500 hover:bg-gray-100 dark:hover:bg-[#333] transition-colors">Cancel</button>
          <button
            @click="emit('confirm')"
            :disabled="modelValue.lat === null || modelValue.lng === null"
            class="px-4 py-1.5 text-sm rounded-lg bg-red-500 text-white font-medium hover:bg-red-600 transition-colors disabled:opacity-40 disabled:cursor-not-allowed flex items-center gap-1.5"
          >
            <MapPinIcon class="w-3.5 h-3.5" />
            Insert
          </button>
        </div>
      </div>
    </div>
  </Teleport>
</template>
