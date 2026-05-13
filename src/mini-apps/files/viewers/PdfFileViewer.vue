<script setup lang="ts">
import { ref, watch, onMounted, onBeforeUnmount, nextTick } from 'vue';
import { convertFileSrc } from '@tauri-apps/api/core';
import { usePdfRenderer } from '../composables/usePdfRenderer';
import { ZoomIn, ZoomOut, ChevronLeft, ChevronRight, Moon, Sun } from 'lucide-vue-next';

const props = defineProps<{
  filePath: string;
  vaultPath: string;
}>();

const renderer = usePdfRenderer();
const darkMode = ref(false);

// ─── Load PDF when filePath changes ──────────────────────────
watch(() => props.filePath, async (path) => {
  if (path) {
    const src = convertFileSrc(path);
    await renderer.loadPdf(src);
  }
}, { immediate: true });

// ─── Lazy rendering with IntersectionObserver ────────────────
const containerRef = ref<HTMLDivElement | null>(null);
const pageRefs = ref<Map<number, { canvas: HTMLCanvasElement; textLayer: HTMLDivElement }>>(new Map());
const renderedPages = new Map<number, number>();
const BUFFER_PAGES = 2;
let observer: IntersectionObserver | null = null;
const visiblePages = new Set<number>();
let renderQueue: number[] = [];
let isRendering = false;

const processRenderQueue = async () => {
  if (isRendering) return;
  isRendering = true;
  while (renderQueue.length > 0) {
    const pageNum = renderQueue.shift()!;
    const currentScale = renderer.scale.value;
    if (renderedPages.get(pageNum) === currentScale) continue;
    const refs = pageRefs.value.get(pageNum);
    if (!refs) continue;
    try {
      await renderer.renderPage(pageNum, refs.canvas, refs.textLayer);
      renderedPages.set(pageNum, currentScale);
    } catch (e) {
      console.warn(`Failed to render page ${pageNum}:`, e);
    }
  }
  isRendering = false;
};

const queuePageRender = (pageNum: number) => {
  if (!renderQueue.includes(pageNum)) renderQueue.push(pageNum);
  processRenderQueue();
};

const queueVisibleAndBuffer = (centerPage: number) => {
  const total = renderer.totalPages.value;
  const pages: number[] = [];
  for (let offset = 0; offset <= BUFFER_PAGES; offset++) {
    if (centerPage + offset <= total) pages.push(centerPage + offset);
    if (offset > 0 && centerPage - offset >= 1) pages.push(centerPage - offset);
  }
  for (const p of pages) queuePageRender(p);
};

let scrollSyncInProgress = false;

const setupObserver = () => {
  if (observer) observer.disconnect();
  if (!containerRef.value) return;
  observer = new IntersectionObserver((entries) => {
    for (const entry of entries) {
      const pageNum = parseInt(entry.target.getAttribute('data-page') || '0');
      if (!pageNum) continue;
      if (entry.isIntersecting) {
        visiblePages.add(pageNum);
        queueVisibleAndBuffer(pageNum);
      } else {
        visiblePages.delete(pageNum);
      }
    }
    if (visiblePages.size > 0) {
      const sorted = Array.from(visiblePages).sort((a, b) => a - b);
      scrollSyncInProgress = true;
      renderer.currentPage.value = sorted[0];
    }
  }, { root: containerRef.value, rootMargin: '200px 0px', threshold: 0.01 });
  const pageEls = containerRef.value.querySelectorAll('[data-page]');
  pageEls.forEach((el) => observer!.observe(el));
};

// ─── Page dimensions for placeholder sizing ──────────────────
const pageDimensions = ref<Map<number, { width: number; height: number }>>(new Map());

const precomputePageSizes = async () => {
  const { pdfDoc, scale } = renderer;
  if (!pdfDoc.value) return;
  const total = pdfDoc.value.numPages;
  const dims = new Map<number, { width: number; height: number }>();
  for (let i = 1; i <= total; i++) {
    try {
      const page = await pdfDoc.value.getPage(i);
      const viewport = page.getViewport({ scale: scale.value });
      dims.set(i, { width: viewport.width, height: viewport.height });
    } catch (_) {
      dims.set(i, { width: 612 * scale.value, height: 792 * scale.value });
    }
  }
  pageDimensions.value = dims;
};

const getPageStyle = (pageNum: number) => {
  const dim = pageDimensions.value.get(pageNum);
  if (dim) return { width: `${dim.width}px`, height: `${dim.height}px`, maxWidth: '100%' };
  return { maxWidth: '100%' };
};

// ─── Watchers ────────────────────────────────────────────────
watch(() => renderer.totalPages.value, async (total) => {
  if (total > 0) {
    renderedPages.clear(); renderQueue = []; visiblePages.clear();
    await precomputePageSizes(); await nextTick(); setupObserver();
  }
});

watch(() => renderer.scale.value, async () => {
  renderedPages.clear(); renderQueue = [];
  await precomputePageSizes(); await nextTick();
  for (const p of visiblePages) queueVisibleAndBuffer(p);
});

const scrollToPage = (page: number) => {
  const el = containerRef.value?.querySelector(`[data-page="${page}"]`);
  if (el) { el.scrollIntoView({ behavior: 'smooth', block: 'start' }); queueVisibleAndBuffer(page); }
};

watch(() => renderer.currentPage.value, (page) => {
  if (scrollSyncInProgress) { scrollSyncInProgress = false; return; }
  scrollToPage(page);
});

const registerPageRef = (pageNum: number, canvas: HTMLCanvasElement, textLayer: HTMLDivElement) => {
  pageRefs.value.set(pageNum, { canvas, textLayer });
};

onBeforeUnmount(() => { if (observer) observer.disconnect(); });

// ─── Toolbar actions ─────────────────────────────────────────
const zoomIn = () => { renderer.scale.value = Math.min(renderer.scale.value + 0.25, 4); };
const zoomOut = () => { renderer.scale.value = Math.max(renderer.scale.value - 0.25, 0.5); };
const prevPage = () => { if (renderer.currentPage.value > 1) renderer.currentPage.value--; };
const nextPage = () => { if (renderer.currentPage.value < renderer.totalPages.value) renderer.currentPage.value++; };
const zoomPercent = () => Math.round(renderer.scale.value / 1.5 * 100);
</script>

<template>
  <div class="flex-1 flex flex-col overflow-hidden bg-[#f0f0f0] dark:bg-[#1a1a1a]">
    <!-- Toolbar -->
    <div class="flex items-center justify-center gap-2 px-4 py-2 bg-white/80 dark:bg-[#222]/80 backdrop-blur border-b border-gray-200/50 dark:border-white/5 flex-shrink-0">
      <button @click="prevPage" :disabled="renderer.currentPage.value <= 1" class="p-1.5 rounded-lg hover:bg-gray-100 dark:hover:bg-white/10 text-gray-600 dark:text-gray-300 disabled:opacity-30 cursor-pointer"><ChevronLeft class="w-4 h-4" /></button>
      <span class="text-xs font-mono text-gray-500 min-w-[60px] text-center">{{ renderer.currentPage.value }} / {{ renderer.totalPages.value }}</span>
      <button @click="nextPage" :disabled="renderer.currentPage.value >= renderer.totalPages.value" class="p-1.5 rounded-lg hover:bg-gray-100 dark:hover:bg-white/10 text-gray-600 dark:text-gray-300 disabled:opacity-30 cursor-pointer"><ChevronRight class="w-4 h-4" /></button>
      <div class="w-px h-5 bg-gray-200 dark:bg-white/10 mx-1" />
      <button @click="zoomOut" class="p-1.5 rounded-lg hover:bg-gray-100 dark:hover:bg-white/10 text-gray-600 dark:text-gray-300 cursor-pointer"><ZoomOut class="w-4 h-4" /></button>
      <span class="text-xs font-mono text-gray-500 w-10 text-center">{{ zoomPercent() }}%</span>
      <button @click="zoomIn" class="p-1.5 rounded-lg hover:bg-gray-100 dark:hover:bg-white/10 text-gray-600 dark:text-gray-300 cursor-pointer"><ZoomIn class="w-4 h-4" /></button>
      <div class="w-px h-5 bg-gray-200 dark:bg-white/10 mx-1" />
      <button @click="darkMode = !darkMode" class="p-1.5 rounded-lg hover:bg-gray-100 dark:hover:bg-white/10 text-gray-600 dark:text-gray-300 cursor-pointer">
        <Moon v-if="!darkMode" class="w-4 h-4" /><Sun v-else class="w-4 h-4" />
      </button>
    </div>

    <!-- PDF Content -->
    <div ref="containerRef" class="pdf-viewer-container flex-1 overflow-y-auto overflow-x-hidden px-4 py-6" :class="{ 'pdf-dark-mode': darkMode }">
      <div v-if="renderer.isLoading.value" class="flex items-center justify-center h-full">
        <div class="flex flex-col items-center gap-3">
          <div class="w-8 h-8 border-2 border-indigo-500 border-t-transparent rounded-full animate-spin" />
          <span class="text-sm text-gray-500">Loading PDF…</span>
        </div>
      </div>
      <div v-else-if="renderer.error.value" class="flex items-center justify-center h-full">
        <p class="text-sm text-red-500">{{ renderer.error.value }}</p>
      </div>
      <div v-else class="flex flex-col items-center gap-4">
        <div v-for="pageNum in renderer.totalPages.value" :key="pageNum" :data-page="pageNum"
          class="pdf-page relative bg-white shadow-lg rounded-sm" :style="getPageStyle(pageNum)">
          <canvas :ref="(el: any) => { if (el) registerPageRef(pageNum, el as HTMLCanvasElement, (el as HTMLCanvasElement)?.nextElementSibling as HTMLDivElement) }" class="block w-full h-full" />
          <div class="textLayer absolute top-0 left-0 overflow-hidden opacity-25 leading-none" />
          <div class="absolute bottom-2 right-3 text-[10px] text-gray-400 font-mono select-none pointer-events-none">{{ pageNum }}</div>
        </div>
      </div>
    </div>
  </div>
</template>

<style scoped>
.pdf-viewer-container { background: #f0f0f0; scroll-behavior: smooth; }
:is(.dark) .pdf-viewer-container { background: #1a1a1a; }
.pdf-dark-mode .pdf-page canvas { filter: invert(0.88) hue-rotate(180deg); }
.textLayer { line-height: 1; }
.textLayer :deep(span) { position: absolute; white-space: pre; color: transparent; pointer-events: all; }
.textLayer :deep(span::selection) { background: rgba(0, 100, 200, 0.3); }
.textLayer :deep(br) { display: none; }
</style>
