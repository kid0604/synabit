<template>
  <node-view-wrapper class="code-block-wrapper relative group">
    <!-- Language Selector & Copy Button Container -->
    <div 
      class="absolute top-2 right-2 flex items-center gap-1.5 opacity-0 group-hover:opacity-100 transition-opacity duration-200 z-10"
      contenteditable="false"
    >
      <!-- Mermaid Mode Toggle -->
      <div v-if="selectedLanguage === 'mermaid'" class="flex items-center bg-white/90 dark:bg-[#2c2c2c]/90 border border-gray-200 dark:border-[#3f3f46] rounded p-0.5 backdrop-blur-sm">
        <button @click.prevent="displayMode = 'code'" :class="['px-2 py-0.5 text-[10px] rounded uppercase font-semibold tracking-wider transition-colors', displayMode === 'code' ? 'bg-gray-200 dark:bg-[#444] text-gray-800 dark:text-gray-200' : 'text-gray-500 hover:text-gray-700 dark:hover:text-gray-300']" title="Code only">Code</button>
        <button @click.prevent="displayMode = 'split'" :class="['px-2 py-0.5 text-[10px] rounded uppercase font-semibold tracking-wider transition-colors', displayMode === 'split' ? 'bg-gray-200 dark:bg-[#444] text-gray-800 dark:text-gray-200' : 'text-gray-500 hover:text-gray-700 dark:hover:text-gray-300']" title="Split view">Split</button>
        <button @click.prevent="displayMode = 'preview'" :class="['px-2 py-0.5 text-[10px] rounded uppercase font-semibold tracking-wider transition-colors', displayMode === 'preview' ? 'bg-gray-200 dark:bg-[#444] text-gray-800 dark:text-gray-200' : 'text-gray-500 hover:text-gray-700 dark:hover:text-gray-300']" title="Preview only">Preview</button>
      </div>

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

    <pre v-show="displayMode !== 'preview'"><node-view-content as="code" :class="languageClass" /></pre>

    <!-- Mermaid Preview -->
    <div v-if="selectedLanguage === 'mermaid' && displayMode !== 'code'" class="mermaid-preview mt-2 p-4 rounded-lg border border-gray-200 dark:border-[#3f3f46] bg-white dark:bg-[#1e1e1e] flex flex-col items-center justify-center min-h-[100px]" contenteditable="false">
      <div v-if="mermaidError" class="text-red-500 text-xs w-full overflow-x-auto p-2 bg-red-50 dark:bg-red-900/20 rounded font-mono border border-red-100 dark:border-red-900/50">{{ mermaidError }}</div>
      <div v-html="mermaidSvg" class="mermaid-svg-container w-full overflow-x-auto flex justify-center text-black dark:text-white"></div>
    </div>
  </node-view-wrapper>
</template>

<script setup lang="ts">
import { NodeViewWrapper, NodeViewContent, nodeViewProps } from '@tiptap/vue-3';
import { computed, ref } from 'vue';

const props = defineProps(nodeViewProps);

const copied = ref(false);
const displayMode = ref<'code' | 'split' | 'preview'>('split');

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

// --- Mermaid Rendering Logic ---
import mermaid from 'mermaid';
import { watch, onMounted, onUnmounted } from 'vue';

const mermaidSvg = ref('');
const mermaidError = ref('');
let renderTimeout: number | null = null;
let diagramIdCounter = 0;
let observer: MutationObserver | null = null;

const applyMermaidTheme = () => {
  const isDark = document.documentElement.classList.contains('dark');
  mermaid.initialize({ 
    startOnLoad: false, 
    theme: isDark ? 'dark' : 'default',
    fontFamily: 'inherit'
  });
};

const renderMermaid = async () => {
  if (selectedLanguage.value !== 'mermaid') return;
  const content = props.node.textContent;
  if (!content.trim()) {
    mermaidSvg.value = '';
    mermaidError.value = '';
    return;
  }
  
  try {
    mermaidError.value = '';
    const id = `mermaid-diagram-${Date.now()}-${diagramIdCounter++}`;
    const { svg } = await mermaid.render(id, content);
    mermaidSvg.value = svg;
  } catch (err: any) {
    mermaidError.value = err.message || 'Syntax Error in Mermaid graph';
    // Remove the error SVG that mermaid sometimes injects into the body
    const errorEl = document.querySelector(`#${err.hash || id}`);
    if (errorEl) errorEl.remove();
  }
};

watch(() => props.node.textContent, () => {
  if (selectedLanguage.value !== 'mermaid') return;
  if (renderTimeout) clearTimeout(renderTimeout);
  renderTimeout = window.setTimeout(() => {
    renderMermaid();
  }, 500); // Debounce user typing
});

watch(selectedLanguage, (newLang) => {
  if (newLang === 'mermaid') {
    applyMermaidTheme();
    renderMermaid();
  }
});

onMounted(() => {
  applyMermaidTheme();
  
  if (selectedLanguage.value === 'mermaid') {
    renderMermaid();
  }

  // Watch for dark mode changes on the HTML element
  observer = new MutationObserver((mutations) => {
    mutations.forEach((mutation) => {
      if (mutation.attributeName === 'class') {
        applyMermaidTheme();
        if (selectedLanguage.value === 'mermaid') {
          renderMermaid(); // re-render with new theme
        }
      }
    });
  });
  observer.observe(document.documentElement, { attributes: true });
});

onUnmounted(() => {
  if (observer) observer.disconnect();
  if (renderTimeout) clearTimeout(renderTimeout);
});
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
