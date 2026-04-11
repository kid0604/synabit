<script setup lang="ts">
import { NodeViewWrapper, nodeViewProps } from '@tiptap/vue-3';
import { ref, computed, nextTick, onMounted } from 'vue';
import katex from 'katex';
import 'katex/dist/katex.min.css';

const props = defineProps(nodeViewProps);

const isEditing = ref(false);
const inputRef = ref<HTMLInputElement | null>(null);

// By default, open edit mode if the content is empty.
onMounted(() => {
    if (!props.node.attrs.latex) {
        isEditing.value = true;
        nextTick(() => {
            inputRef.value?.focus();
        });
    }
});

const latexContent = computed({
  get: () => props.node.attrs.latex,
  set: (val) => {
    props.updateAttributes({ latex: val });
  }
});

const renderedHtml = computed(() => {
   if (!latexContent.value) return '<span class="text-gray-400 text-sm">Empty equation</span>';
   try {
      return katex.renderToString(latexContent.value, {
         throwOnError: false,
         displayMode: false // Inline display mode
      });
   } catch (e: any) {
      return `<span class="text-red-500 text-sm">${e.message}</span>`;
   }
});

const startEditing = () => {
    if (!props.editor.isEditable) return;
    isEditing.value = true;
    nextTick(() => {
        inputRef.value?.focus();
    });
};

const finishEditing = () => {
    isEditing.value = false;
    // Add focus back to editor, just past the node if needed?
    // Usually standard nodes just blur their internals.
};
</script>

<template>
  <NodeViewWrapper class="equation-node inline-block align-middle relative group z-10">
    <span 
        v-if="!isEditing"
        @click="startEditing"
        class="cursor-pointer hover:bg-black/5 dark:hover:bg-white/10 rounded px-1 min-h-[1.5em] min-w-[2em] inline-flex items-center justify-center transition-colors border border-transparent hover:border-gray-200 dark:hover:border-gray-700"
        v-html="renderedHtml"
    ></span>
    
    <span v-else class="inline-flex items-center gap-1 bg-gray-50 dark:bg-[#1a1a1a] rounded px-2 py-0.5 shadow-sm border border-blue-200 dark:border-blue-900 ring-2 ring-blue-100 dark:ring-blue-900/40" contenteditable="false">
        <span class="text-gray-400 font-mono text-xs select-none pointer-events-none">$$</span>
        <input 
            ref="inputRef"
            v-model="latexContent"
            @blur="finishEditing"
            @keydown.enter.prevent="finishEditing"
            @keydown.esc.prevent="finishEditing"
            class="bg-transparent border-none outline-none font-mono text-sm leading-none text-blue-600 dark:text-blue-400 py-1"
            :style="{ width: Math.max(60, latexContent.length * 8) + 'px' }"
            placeholder="x_1"
        />
        <span class="text-gray-400 font-mono text-xs select-none pointer-events-none">$$</span>
    </span>
  </NodeViewWrapper>
</template>

<style>
.equation-node .katex-display {
    margin: 0 !important;
}
.equation-node .katex {
    font-size: 1.1em;
}
</style>
