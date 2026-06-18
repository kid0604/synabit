<script setup lang="ts">
import { Link as LinkIcon } from 'lucide-vue-next';

defineProps<{
  show: boolean;
  top: number;
  left: number;
}>();

const emit = defineEmits<{
  (e: 'copy-block-link'): void;
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
        Copy Block Link
      </button>
    </div>
  </Transition>
</template>
