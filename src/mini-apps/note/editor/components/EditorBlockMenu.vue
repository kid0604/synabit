<script setup lang="ts">
import { Link as LinkIcon } from 'lucide-vue-next';
import synAvatar from '../../../../assets/syn-avatar.jpg';

defineProps<{
  show: boolean;
  top: number;
  left: number;
  /**
   * The label for "Ask Syn about this", or `null` to leave the item out —
   * Syn off, or the app locked. The right-click menu is where somebody who
   * never found the key or the toolbar button goes looking for what else can
   * be done with a paragraph.
   */
  askSynLabel?: string | null;
}>();

const emit = defineEmits<{
  (e: 'copy-block-link'): void;
  (e: 'ask-syn'): void;
}>();
</script>

<template>
  <Transition name="bubble">
    <div
      v-if="show"
      class="tc-ctx-menu"
      :style="{ position: 'absolute', top: top + 'px', left: left + 'px', zIndex: 100 }"
      @mousedown.prevent
    >
      <button @click="emit('copy-block-link')" class="flex items-center gap-2">
        <LinkIcon class="w-3.5 h-3.5" />
        {{ $t('note.editor.copy_block_link') }}
      </button>
      <button v-if="askSynLabel" @click="emit('ask-syn')" class="flex items-center gap-2">
        <img :src="synAvatar" alt="" class="w-3.5 h-3.5 rounded-full object-cover" />
        {{ askSynLabel }}
      </button>
    </div>
  </Transition>
</template>
