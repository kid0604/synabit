<script setup lang="ts">
import { Video as VideoIcon, Music as MusicIcon } from 'lucide-vue-next';

defineProps<{
  show: boolean;
  url: string;
  type: 'video' | 'audio';
}>();

const emit = defineEmits<{
  (e: 'update:show', value: boolean): void;
  (e: 'update:url', value: string): void;
  (e: 'confirm'): void;
  (e: 'browse-local'): void;
}>();

const config = {
  video: {
    title: 'Embed Video',
    label: 'YouTube or Web URL',
    placeholder: 'https://youtube.com/watch?v=...',
    browseLabel: 'Browse Local File',
    icon: VideoIcon,
  },
  audio: {
    title: 'Embed Audio',
    label: 'Spotify, SoundCloud or Web URL',
    placeholder: 'https://open.spotify.com/track/...',
    browseLabel: 'Browse Local File',
    icon: MusicIcon,
  },
};
</script>

<template>
  <Teleport to="body">
    <div v-if="show" class="fixed inset-0 z-[999] flex items-center justify-center bg-black/40 backdrop-blur-sm" @click.self="emit('update:show', false)">
      <div class="bg-white dark:bg-[#2a2a2a] rounded-2xl shadow-2xl p-6 w-96 border border-[#e6e6e6] dark:border-[#3a3a3a]">
        <h3 class="text-base font-semibold text-[#1c1c1e] dark:text-[#f4f4f5] mb-4">{{ config[type].title }}</h3>
        
        <div class="space-y-4">
          <div>
            <label class="block text-xs font-medium text-gray-500 dark:text-gray-400 mb-1">{{ config[type].label }}</label>
            <input
              :value="url"
              @input="emit('update:url', ($event.target as HTMLInputElement).value)"
              type="url"
              :placeholder="config[type].placeholder"
              class="w-full px-3 py-2 rounded-lg border border-[#e0e0e0] dark:border-[#444] bg-white dark:bg-[#1e1e1e] text-[#1c1c1e] dark:text-[#f4f4f5] text-sm focus:outline-none focus:ring-2 focus:ring-black/10 dark:focus:ring-white/20"
              @keydown.enter="emit('confirm')"
              autofocus
            />
          </div>
          
          <div class="flex items-center justify-center">
            <div class="h-px bg-gray-200 dark:bg-[#444] flex-1"></div>
            <span class="text-xs text-gray-400 px-3 uppercase tracking-wider font-semibold">Or</span>
            <div class="h-px bg-gray-200 dark:bg-[#444] flex-1"></div>
          </div>
          
          <button @click="emit('browse-local')" class="w-full py-2 px-4 rounded-lg bg-[#f4f4f5] dark:bg-[#333] text-sm text-[#1c1c1e] dark:text-[#f4f4f5] font-medium hover:bg-[#e4e4e7] dark:hover:bg-[#444] transition-colors border border-[#e0e0e0] dark:border-[#444] flex items-center justify-center gap-2">
            <component :is="config[type].icon" class="w-4 h-4" />
            {{ config[type].browseLabel }}
          </button>
        </div>
        
        <div class="flex justify-end gap-2 mt-6">
          <button @click="emit('update:show', false)" class="px-4 py-1.5 text-sm rounded-lg text-gray-500 hover:bg-gray-100 dark:hover:bg-[#333] transition-colors">Cancel</button>
          <button @click="emit('confirm')" class="px-4 py-1.5 text-sm rounded-lg bg-black dark:bg-white text-white dark:text-black font-medium hover:opacity-80 transition-opacity">Embed</button>
        </div>
      </div>
    </div>
  </Teleport>
</template>
