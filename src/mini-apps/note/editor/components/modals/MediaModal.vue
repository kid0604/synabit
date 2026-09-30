<script setup lang="ts">
import AppDialog from '../../../../../shared/components/AppDialog.vue';
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

// i18n keys, translated where the template renders them.
const config = {
  video: {
    title: 'note.editor.media.video_title',
    label: 'note.editor.media.video_label',
    placeholder: 'https://youtube.com/watch?v=...',
    browseLabel: 'note.editor.media.browse_local',
    icon: VideoIcon,
  },
  audio: {
    title: 'note.editor.media.audio_title',
    label: 'note.editor.media.audio_label',
    placeholder: 'https://open.spotify.com/track/...',
    browseLabel: 'note.editor.media.browse_local',
    icon: MusicIcon,
  },
};
</script>

<template>
  <AppDialog :show="show" labelledby="note-media-title" size="sm" panel-class="p-6" @close="emit('update:show', false)">
    <h3 id="note-media-title" class="text-base font-semibold text-text dark:text-text-dark mb-4">{{ $t(config[type].title) }}</h3>
    
    <div class="space-y-4">
      <div>
        <label class="block text-xs font-medium text-gray-500 dark:text-gray-400 mb-1">{{ $t(config[type].label) }}</label>
        <input
          :value="url"
          @input="emit('update:url', ($event.target as HTMLInputElement).value)"
          type="url"
          :placeholder="config[type].placeholder"
          class="w-full px-3 py-2 rounded-lg border border-border-subtle dark:border-[#444] bg-white dark:bg-surface-dark text-text dark:text-text-dark text-sm focus:outline-none focus:ring-2 focus:ring-black/10 dark:focus:ring-white/20"
          @keydown.enter="emit('confirm')"
          autofocus
        />
      </div>
      
      <div class="flex items-center justify-center">
        <div class="h-px bg-gray-200 dark:bg-[#444] flex-1"></div>
        <span class="text-xs text-gray-500 dark:text-gray-400 px-3 uppercase tracking-wider font-semibold">{{ $t('note.editor.media.or') }}</span>
        <div class="h-px bg-gray-200 dark:bg-[#444] flex-1"></div>
      </div>
      
      <button @click="emit('browse-local')" class="w-full py-2 px-4 rounded-lg bg-[#f4f4f5] dark:bg-[#333] text-sm text-text dark:text-text-dark font-medium hover:bg-[#e4e4e7] dark:hover:bg-[#444] transition-colors border border-border-subtle dark:border-[#444] flex items-center justify-center gap-2">
        <component :is="config[type].icon" class="w-4 h-4" />
        {{ $t(config[type].browseLabel) }}
      </button>
    </div>
    
    <div class="flex justify-end gap-2 mt-6">
      <button @click="emit('update:show', false)" class="px-4 py-1.5 text-sm rounded-lg text-gray-500 dark:text-gray-400 hover:bg-gray-100 dark:hover:bg-[#333] transition-colors">{{ $t('note.cancel') }}</button>
      <button @click="emit('confirm')" class="btn-primary">{{ $t('note.editor.embed') }}</button>
    </div>
  </AppDialog>
</template>
