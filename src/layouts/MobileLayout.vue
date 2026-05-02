<script setup lang="ts">
import { ref } from 'vue';
import { useSwipe } from '@vueuse/core';

const props = defineProps<{
  activeTool: string;
}>();

const emit = defineEmits<{
  (e: 'update:activeTool', val: string): void;
}>();

const tools = ['nexus', 'quickcap', 'note', 'task', 'calendar', 'whiteboard'];

const mainRef = ref<HTMLElement | null>(null);

const { isSwiping, direction } = useSwipe(mainRef, {
  threshold: 50,
  onSwipeEnd: (e, dir) => {
    const currentIndex = tools.indexOf(props.activeTool);
    if (currentIndex === -1) return;

    if (dir === 'left') {
      // Swipe left means go to the next tool (right)
      if (currentIndex < tools.length - 1) {
        emit('update:activeTool', tools[currentIndex + 1]);
      }
    } else if (dir === 'right') {
      // Swipe right means go to the prev tool (left)
      if (currentIndex > 0) {
        emit('update:activeTool', tools[currentIndex - 1]);
      }
    }
  }
});
</script>

<template>
  <div class="flex flex-col h-screen w-full bg-base text-text dark:bg-base-dark dark:text-text-dark font-sans overflow-hidden select-none" style="padding-top: max(env(safe-area-inset-top), 36px);">
    <div ref="mainRef" class="flex-1 flex flex-col overflow-hidden relative">
      <!-- Main Content Area -->
      <slot />
    </div>
    
    <!-- Bottom Navigation Bar for Mobile -->
    <nav class="w-full z-[100] bg-sidebar dark:bg-sidebar-dark border-t border-border dark:border-border-dark flex justify-around items-center h-16 pb-[env(safe-area-inset-bottom)] px-2">
      <slot name="bottombar" />
    </nav>
    
    <!-- Modals -->
    <slot name="modal" />
  </div>
</template>
