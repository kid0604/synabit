<template>
  <node-view-wrapper class="code-block-wrapper relative group my-4">
    <!-- Header Bar -->
    <div 
      class="flex items-center justify-between bg-gray-100/80 dark:bg-[#252525] px-3 py-1.5 rounded-t-lg border border-gray-200 dark:border-[#3f3f46] border-b-0"
      contenteditable="false"
    >
      <!-- Language Selector -->
      <div class="relative flex items-center">
        <select 
          class="appearance-none text-[11px] uppercase font-semibold tracking-wider bg-transparent text-gray-500 dark:text-gray-400 border-none outline-none cursor-pointer hover:text-gray-700 dark:hover:text-gray-200 transition-colors py-1 pl-1 pr-5"
          v-model="selectedLanguage"
        >
          <option :value="null">AUTO</option>
          <option disabled>—</option>
          <option v-for="(language, index) in languages" :value="language" :key="index">
            {{ language }}
          </option>
        </select>
        <div class="pointer-events-none absolute right-1 text-gray-400 dark:text-gray-500">
          <svg xmlns="http://www.w3.org/2000/svg" width="12" height="12" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><path d="m6 9 6 6 6-6"/></svg>
        </div>
      </div>

      <div class="flex items-center gap-3">
        <!-- Diagram Mode Toggle (Mermaid / Markmap) -->
        <div v-if="isDiagramLanguage" class="flex items-center bg-gray-200/50 dark:bg-[#1a1a1a] rounded p-0.5">
          <button @click.prevent="displayMode = 'code'" :class="['px-2 py-0.5 text-[10px] rounded uppercase font-semibold tracking-wider transition-colors', displayMode === 'code' ? 'bg-white dark:bg-[#333] shadow-sm text-gray-800 dark:text-gray-200' : 'text-gray-500 hover:text-gray-700 dark:hover:text-gray-300']" title="Code only">Code</button>
          <button @click.prevent="displayMode = 'split'" :class="['px-2 py-0.5 text-[10px] rounded uppercase font-semibold tracking-wider transition-colors', displayMode === 'split' ? 'bg-white dark:bg-[#333] shadow-sm text-gray-800 dark:text-gray-200' : 'text-gray-500 hover:text-gray-700 dark:hover:text-gray-300']" title="Split view">Split</button>
          <button @click.prevent="displayMode = 'preview'" :class="['px-2 py-0.5 text-[10px] rounded uppercase font-semibold tracking-wider transition-colors', displayMode === 'preview' ? 'bg-white dark:bg-[#333] shadow-sm text-gray-800 dark:text-gray-200' : 'text-gray-500 hover:text-gray-700 dark:hover:text-gray-300']" title="Preview only">Preview</button>
        </div>

        <!-- Copy Button -->
        <button 
          @click.prevent="copyCode" 
          class="text-gray-500 dark:text-gray-400 hover:text-gray-700 dark:hover:text-gray-200 transition-colors flex items-center gap-1.5 text-[11px] font-medium"
          title="Copy code"
        >
          <svg v-if="!copied" xmlns="http://www.w3.org/2000/svg" width="13" height="13" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><rect x="9" y="9" width="13" height="13" rx="2" ry="2"></rect><path d="M5 15H4a2 2 0 0 1-2-2V4a2 2 0 0 1 2-2h9a2 2 0 0 1 2 2v1"></path></svg>
          <svg v-else xmlns="http://www.w3.org/2000/svg" width="13" height="13" viewBox="0 0 24 24" fill="none" stroke="#10b981" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><polyline points="20 6 9 17 4 12"></polyline></svg>
          <span v-if="copied" class="text-emerald-500">Copied!</span>
          <span v-else>Copy</span>
        </button>
      </div>
    </div>

    <!-- Code Block Content -->
    <pre v-show="displayMode !== 'preview'" class="!mt-0 !rounded-t-none border border-gray-200 dark:border-[#3f3f46]"><node-view-content as="code" :class="languageClass" /></pre>

    <!-- Mermaid Preview -->
    <div v-if="selectedLanguage === 'mermaid' && displayMode !== 'code'" class="mermaid-preview mt-2 p-4 rounded-lg border border-gray-200 dark:border-[#3f3f46] bg-white dark:bg-[#1e1e1e] flex flex-col items-center justify-center min-h-[100px]" contenteditable="false">
      <div v-if="mermaidError" class="text-red-500 text-xs w-full overflow-x-auto p-2 bg-red-50 dark:bg-red-900/20 rounded font-mono border border-red-100 dark:border-red-900/50">{{ mermaidError }}</div>
      <div v-html="mermaidSvg" class="mermaid-svg-container w-full overflow-x-auto flex justify-center text-black dark:text-white"></div>
    </div>

    <!-- Markmap Preview -->
    <div 
      v-if="selectedLanguage === 'markmap' && displayMode !== 'code'" 
      ref="markmapContainerRef"
      :class="['markmap-container', { 'markmap-dark': isDarkMode }]"
      contenteditable="false"
    >
      <div v-if="markmapError" class="markmap-error">{{ markmapError }}</div>
      <svg ref="markmapSvgRef" class="markmap-svg"></svg>
    </div>
  </node-view-wrapper>
</template>

<script lang="ts">
// Shared state across all instances of CodeBlockComponent
let diagramIdCounter = 0;
// Global queue to prevent concurrent mermaid rendering which causes race conditions
let renderPromise = Promise.resolve();
</script>

<script setup lang="ts">
import { NodeViewWrapper, NodeViewContent, nodeViewProps } from '@tiptap/vue-3';
import { computed, ref, watch, onMounted, onUnmounted, nextTick } from 'vue';
import mermaid from 'mermaid';
import { Transformer } from 'markmap-lib';
import { Markmap, deriveOptions } from 'markmap-view';
import { Toolbar } from 'markmap-toolbar';
import 'markmap-toolbar/dist/style.css';

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

const isDiagramLanguage = computed(() => {
  return selectedLanguage.value === 'mermaid' || selectedLanguage.value === 'markmap';
});

const isDarkMode = ref(document.documentElement.classList.contains('dark'));

const copyCode = () => {
  navigator.clipboard.writeText(props.node.textContent);
  copied.value = true;
  setTimeout(() => { copied.value = false; }, 2000);
};

// --- Mermaid Rendering Logic ---
const mermaidSvg = ref('');
const mermaidError = ref('');
let renderTimeout: number | null = null;
let observer: MutationObserver | null = null;

const applyMermaidTheme = () => {
  const isDark = document.documentElement.classList.contains('dark');
  mermaid.initialize({ 
    startOnLoad: false, 
    theme: isDark ? 'dark' : 'default',
    fontFamily: 'inherit'
  });
};

const renderMermaid = () => {
  if (selectedLanguage.value !== 'mermaid') return;
  const content = props.node.textContent;
  if (!content.trim()) {
    mermaidSvg.value = '';
    mermaidError.value = '';
    return;
  }
  
  const id = `mermaid-diagram-${Date.now()}-${diagramIdCounter++}`;
  
  // Chain render calls to prevent concurrent execution bugs in mermaid
  renderPromise = renderPromise.then(async () => {
    // Re-check if content changed while waiting in queue
    if (content !== props.node.textContent) return;
    
    try {
      mermaidError.value = '';
      const { svg } = await mermaid.render(id, content);
      mermaidSvg.value = svg;
    } catch (err: any) {
      mermaidError.value = err.message || 'Syntax Error in Mermaid graph';
      // Remove the error SVG that mermaid sometimes injects into the body
      const errorEl = document.querySelector(`#${err.hash || id}`);
      if (errorEl) errorEl.remove();
    }
  }).catch(() => {});
};

// --- Markmap Rendering Logic ---

const markmapSvgRef = ref<SVGSVGElement | null>(null);
const markmapContainerRef = ref<HTMLDivElement | null>(null);
const markmapError = ref('');
let markmapInstance: Markmap | null = null;
let markmapToolbar: Toolbar | null = null;
let markmapRenderTimeout: number | null = null;
let markmapResizeObserver: ResizeObserver | null = null;
let pendingMarkmapData: { root: any, options: any } | null = null;

const markmapTransformer = new Transformer();

const renderMarkmap = async () => {
  if (selectedLanguage.value !== 'markmap') return;
  const content = props.node.textContent;

  if (!content.trim()) {
    markmapError.value = '';
    if (markmapInstance) {
      markmapInstance.destroy();
      markmapInstance = null;
    }
    pendingMarkmapData = null;
    return;
  }

  await nextTick();
  const svgEl = markmapSvgRef.value;
  const containerEl = markmapContainerRef.value;
  if (!svgEl || !containerEl) return;

  try {
    markmapError.value = '';
    const { root, features } = markmapTransformer.transform(content);
    const derivedOptions = deriveOptions(features);
    
    // Store data to be applied once we have layout
    pendingMarkmapData = { root, options: derivedOptions };

    // Initialize instance empty if not exists
    if (!markmapInstance) {
      svgEl.innerHTML = '';
      markmapInstance = Markmap.create(svgEl, {
        embedGlobalCSS: true,
        zoom: true,
        pan: true,
        initialExpandLevel: -1,
        fitRatio: 0.95,
        paddingX: 16,
        spacingHorizontal: 80,
        spacingVertical: 5,
      }); // DO NOT pass root here, so fit() is not called yet

      // Initialize and attach Toolbar for zoom in/out
      if (!markmapToolbar) {
        markmapToolbar = new Toolbar();
        markmapToolbar.showBrand = false;
        markmapToolbar.setItems(['zoomIn', 'zoomOut', 'fit']);
        markmapToolbar.attach(markmapInstance);
        const el = markmapToolbar.render();
        el.style.position = 'absolute';
        el.style.bottom = '16px';
        el.style.right = '16px';
        containerEl.appendChild(el);
      } else {
        markmapToolbar.attach(markmapInstance);
      }
    }

    const tryApplyData = () => {
      if (!markmapInstance || !pendingMarkmapData || !svgEl) return false;
      const rect = svgEl.getBoundingClientRect();
      if (rect.width > 10 && rect.height > 10) {
        const { root: pendingRoot, options: pendingOpts } = pendingMarkmapData;
        pendingMarkmapData = null; // Clear pending
        
        // Use setData to safely apply the tree, then fit
        markmapInstance.setData(pendingRoot, {
          ...pendingOpts,
          embedGlobalCSS: true,
          zoom: true,
          pan: true,
          initialExpandLevel: -1,
          fitRatio: 0.95,
          paddingX: 16,
          spacingHorizontal: 80,
          spacingVertical: 5,
        }).then(() => {
          markmapInstance?.fit();
        });
        return true;
      }
      return false;
    };

    // First try immediately
    if (!tryApplyData()) {
      // If it fails (zero size), set up an observer
      if (!markmapResizeObserver) {
        markmapResizeObserver = new ResizeObserver(() => {
          if (tryApplyData()) {
            // Once successfully applied, we can keep the observer to re-fit on resize
            // But we don't need to apply data again unless content changes
            markmapInstance?.fit();
          } else if (markmapInstance && !pendingMarkmapData) {
            // If we have size and no pending data, just ensure it fits nicely when resized
            const rect = svgEl.getBoundingClientRect();
            if (rect.width > 10 && rect.height > 10) {
              markmapInstance.fit();
            }
          }
        });
        markmapResizeObserver.observe(containerEl);
      }
    }
  } catch (err: any) {
    markmapError.value = err.message || 'Error rendering Markmap';
  }
};

// --- Watchers ---
watch(() => props.node.textContent, () => {
  if (selectedLanguage.value === 'mermaid') {
    if (renderTimeout) clearTimeout(renderTimeout);
    renderTimeout = window.setTimeout(() => {
      renderMermaid();
    }, 500);
  } else if (selectedLanguage.value === 'markmap') {
    if (markmapRenderTimeout) clearTimeout(markmapRenderTimeout);
    markmapRenderTimeout = window.setTimeout(() => {
      renderMarkmap();
    }, 500);
  }
});

watch(selectedLanguage, (newLang) => {
  if (newLang === 'mermaid') {
    applyMermaidTheme();
    renderMermaid();
  } else if (newLang === 'markmap') {
    renderMarkmap();
  }
});

onMounted(() => {
  applyMermaidTheme();
  isDarkMode.value = document.documentElement.classList.contains('dark');
  
  if (selectedLanguage.value === 'mermaid') {
    renderMermaid();
  } else if (selectedLanguage.value === 'markmap') {
    renderMarkmap();
  }

  // Watch for dark mode changes on the HTML element
  observer = new MutationObserver((mutations) => {
    mutations.forEach((mutation) => {
      if (mutation.attributeName === 'class') {
        const wasDark = isDarkMode.value;
        isDarkMode.value = document.documentElement.classList.contains('dark');

        applyMermaidTheme();
        if (selectedLanguage.value === 'mermaid') {
          renderMermaid();
        } else if (selectedLanguage.value === 'markmap' && wasDark !== isDarkMode.value) {
          renderMarkmap();
        }
      }
    });
  });
  observer.observe(document.documentElement, { attributes: true });
});

onUnmounted(() => {
  if (observer) observer.disconnect();
  if (markmapResizeObserver) {
    markmapResizeObserver.disconnect();
    markmapResizeObserver = null;
  }
  if (renderTimeout) clearTimeout(renderTimeout);
  if (markmapRenderTimeout) clearTimeout(markmapRenderTimeout);
  if (markmapInstance) {
    markmapInstance.destroy();
    markmapInstance = null;
  }
});
</script>

<style scoped>
.code-block-wrapper {
  /* Remove global wrapper margin since we use my-4 utility */
}
.code-block-wrapper pre {
  margin-top: 0 !important;
  border-top-left-radius: 0 !important;
  border-top-right-radius: 0 !important;
}
</style>

/* Unscoped styles for markmap and mermaid to prevent layout thrashing */
<style>
.mermaid-preview {
  /* 
   * CRITICAL PERFORMANCE FIX: 
   * Mermaid diagrams can contain thousands of DOM elements. 
   * When Tiptap updates or measures the DOM on keystrokes, the browser recalculates layout.
   * 'contain: content' tells the browser this element's layout doesn't affect the outside,
   * completely eliminating typing lag in notes with multiple heavy diagrams.
   */
  contain: content;
}

.markmap-container {
  margin-top: 0.5rem;
  border-radius: 0.5rem;
  border: 1px solid #e5e7eb;
  background: #ffffff;
  overflow: hidden;
  position: relative;
  contain: content; /* Prevent layout thrashing */
}

.dark .markmap-container {
  border-color: #3f3f46;
  background: #1e1e1e;
}

.markmap-container .markmap-svg {
  width: 100%;
  height: 500px;
  display: block;
  /* Reset line-height — critical! Markmap.js.org uses 'leading-none' (line-height: 1) */
  line-height: 1;
}

/*
 * CSS ISOLATION from Tailwind prose.
 * The editor wraps everything in .prose which adds typography styles
 * (margins, font-sizes, line-heights) to headings, paragraphs, etc.
 * These cascade into markmap's foreignObject content and break text
 * measurement, causing tiny/broken rendering. We reset everything.
 */
.markmap-container .markmap-foreign > div,
.markmap-container .markmap-foreign > div * {
  all: revert;
  margin: 0 !important;
  padding: 0 !important;
  line-height: 1.25 !important;
  font: 300 16px/20px sans-serif !important;
  max-width: none !important; /* CRITICAL: overrides .ProseMirror * { max-width: 100% } */
  word-break: normal !important;
  overflow-wrap: normal !important;
  white-space: nowrap !important; /* Force text on one line like markmap default */
}

.markmap-error {
  color: #ef4444;
  font-size: 0.75rem;
  width: 100%;
  overflow-x: auto;
  padding: 0.5rem;
  margin: 0.5rem;
  background: #fef2f2;
  border-radius: 0.25rem;
  font-family: monospace;
  border: 1px solid #fee2e2;
}

.dark .markmap-error {
  background: rgba(127, 29, 29, 0.2);
  border-color: rgba(127, 29, 29, 0.5);
}

/* Dark mode overrides for markmap's own CSS variables */
.markmap-dark .markmap {
  --markmap-text-color: #e4e4e7;
  --markmap-code-bg: #1a1b26;
  --markmap-code-color: #ddd;
  --markmap-circle-open-bg: #444;
  --markmap-a-color: #60a5fa;
  --markmap-a-hover-color: #93bbfd;
}

/* Dark mode overrides for markmap toolbar */
.dark .mm-toolbar {
  background: #252525 !important;
  border: 1px solid #3f3f46 !important;
  color: #e4e4e7 !important;
}
.dark .mm-toolbar-item:hover {
  background: #333 !important;
}
</style>
