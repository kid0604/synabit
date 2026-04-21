<template>
  <node-view-wrapper class="code-block-wrapper relative group">
    <!-- Language Selector & Copy Button Container -->
    <div 
      class="absolute top-2 right-2 flex items-center gap-1.5 opacity-0 group-hover:opacity-100 transition-opacity duration-200 z-10"
      contenteditable="false"
    >
      <div class="relative flex items-center">
        <select 
          class="appearance-none text-[10px] uppercase font-semibold tracking-wider bg-white/90 dark:bg-[#2c2c2c]/90 text-gray-500 dark:text-gray-400 border border-gray-200 dark:border-[#3f3f46] rounded pl-2 pr-5 py-1 outline-none cursor-pointer backdrop-blur-sm hover:text-gray-700 dark:hover:text-gray-200 hover:bg-gray-50 dark:hover:bg-[#333] transition-colors"
          v-model="selectedLanguage"
        >
          <option :value="null">AUTO</option>
          <option disabled>—</option>
          <option v-for="(language, index) in languages" :value="language" :key="index">
            {{ language }}
          </option>
        </select>
        <div class="pointer-events-none absolute right-1.5 text-gray-400 dark:text-gray-500">
          <svg xmlns="http://www.w3.org/2000/svg" width="12" height="12" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><path d="m6 9 6 6 6-6"/></svg>
        </div>
      </div>
      
      <button 
        @click.prevent="copyCode" 
        class="px-1.5 py-1 bg-white/90 dark:bg-[#2c2c2c]/90 hover:bg-gray-100 dark:hover:bg-[#3f3f46] text-gray-500 dark:text-gray-400 hover:text-gray-700 dark:hover:text-gray-200 border border-gray-200 dark:border-[#3f3f46] rounded flex items-center justify-center transition-colors backdrop-blur-sm"
        title="Copy code"
      >
        <svg v-if="!copied" xmlns="http://www.w3.org/2000/svg" width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><rect x="9" y="9" width="13" height="13" rx="2" ry="2"></rect><path d="M5 15H4a2 2 0 0 1-2-2V4a2 2 0 0 1 2-2h9a2 2 0 0 1 2 2v1"></path></svg>
        <svg v-else xmlns="http://www.w3.org/2000/svg" width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="#10b981" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><polyline points="20 6 9 17 4 12"></polyline></svg>
      </button>
    </div>

    <pre><node-view-content as="code" :class="languageClass" /></pre>
  </node-view-wrapper>
</template>

<script setup lang="ts">
import { NodeViewWrapper, NodeViewContent, nodeViewProps } from '@tiptap/vue-3';
import { computed, ref } from 'vue';

const props = defineProps(nodeViewProps);

const copied = ref(false);

const languages = props.extension.options.lowlight.listLanguages();

const selectedLanguage = computed({
  get: () => props.node.attrs.language,
  set: (language) => props.updateAttributes({ language }),
});

const languageClass = computed(() => {
  return selectedLanguage.value ? `language-${selectedLanguage.value}` : '';
});

const copyCode = () => {
  navigator.clipboard.writeText(props.node.textContent);
  copied.value = true;
  setTimeout(() => { copied.value = false; }, 2000);
};
</script>

<style scoped>
.code-block-wrapper {
  margin: 1rem 0;
}
.code-block-wrapper pre {
  margin: 0;
  /* Keep global Tiptap CSS for pre, just ensure wrapper positioning works */
}
</style>
