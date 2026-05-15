<script setup lang="ts">
import 'pdfjs-dist/web/pdf_viewer.css';
import { ref, watch, onMounted, onBeforeUnmount, nextTick } from 'vue';
import { convertFileSrc } from '@tauri-apps/api/core';
import { invoke } from '@tauri-apps/api/core';
import { usePdfRenderer } from '../composables/usePdfRenderer';
import { usePdfAnnotations, type PdfAnnotation } from '../composables/usePdfAnnotations';
import AnnotationOverlay from '../overlays/AnnotationOverlay.vue';
import AnnotationPopup from '../overlays/AnnotationPopup.vue';
import DrawingOverlay from '../overlays/DrawingOverlay.vue';
import AnnotationSidebar from '../overlays/AnnotationSidebar.vue';
import ConfirmModal from '../../../shared/components/ConfirmModal.vue';
import {
  ZoomIn, ZoomOut, ChevronLeft, ChevronRight, Moon, Sun,
  Highlighter, PenTool, PanelRightOpen, PanelRightClose,
  FileDown, RotateCcw
} from 'lucide-vue-next';

const props = defineProps<{
  fileId: string;
  filePath: string;
  vaultPath: string;
}>();

const renderer = usePdfRenderer();
const vaultPathRef = ref(props.vaultPath);
watch(() => props.vaultPath, (v) => { vaultPathRef.value = v; });
const annotations = usePdfAnnotations(vaultPathRef);

const darkMode = ref(false);
const highlightMode = ref(false);
const drawMode = ref(false);
const showSidebar = ref(false);

// ─── Load PDF when filePath changes ──────────────────────────
watch(() => props.filePath, async (path) => {
  if (path && props.fileId) {
    const src = convertFileSrc(path);
    await renderer.loadPdf(src);
    await annotations.loadAnnotations(props.fileId, path);
    await annotations.loadDrawings(props.fileId, path);
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
  if (dim) return { width: `${dim.width}px`, height: `${dim.height}px` };
  return {};
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

// ─── Highlight: Text Selection → Annotation ─────────────────
const popupState = ref<{
  show: boolean;
  mode: 'create' | 'edit';
  annotation: PdfAnnotation | null;
  selectedText: string;
  page: number;
  rects: { x: number; y: number; w: number; h: number }[];
  position: { top: number; left: number };
}>({
  show: false, mode: 'create', annotation: null,
  selectedText: '', page: 0, rects: [], position: { top: 0, left: 0 },
});

const handleDocumentMouseUp = (event: MouseEvent) => {
  if (!highlightMode.value) return;

  // Small delay to let WebKit finalize the selection
  setTimeout(() => {
    const sel = window.getSelection();
    if (!sel || sel.isCollapsed || !sel.toString().trim()) return;

    // Check if selection is within our PDF viewer
    const anchorNode = sel.anchorNode;
    if (!anchorNode) return;
    const anchorEl = anchorNode.nodeType === Node.TEXT_NODE ? anchorNode.parentElement : anchorNode as HTMLElement;
    if (!anchorEl) return;

    // Find the page element containing the selection
    const pageEl = anchorEl.closest('[data-page]');
    if (!pageEl || !containerRef.value?.contains(pageEl)) return;

    const pageNum = parseInt(pageEl.getAttribute('data-page') || '0');
    if (!pageNum) return;

    const text = sel.toString().trim();
    const range = sel.getRangeAt(0);
    const pageRect = pageEl.getBoundingClientRect();
    const dim = pageDimensions.value.get(pageNum);
    if (!dim) return;

    // Get all client rects for multi-line selections
    const clientRects = range.getClientRects();
    const normalizedRects: { x: number; y: number; w: number; h: number }[] = [];
    for (let i = 0; i < clientRects.length; i++) {
      const r = clientRects[i];
      normalizedRects.push({
        x: (r.left - pageRect.left) / dim.width,
        y: (r.top - pageRect.top) / dim.height,
        w: r.width / dim.width,
        h: r.height / dim.height,
      });
    }

    if (normalizedRects.length === 0) return;

    // Position popup near mouse
    popupState.value = {
      show: true,
      mode: 'create',
      annotation: null,
      selectedText: text,
      page: pageNum,
      rects: normalizedRects,
      position: { top: event.clientY - 10, left: event.clientX + 10 },
    };
  }, 10);
};

onMounted(() => {
  document.addEventListener('mouseup', handleDocumentMouseUp);
});

onBeforeUnmount(() => {
  document.removeEventListener('mouseup', handleDocumentMouseUp);
});

const handleClickAnnotation = (ann: PdfAnnotation, event?: MouseEvent) => {
  popupState.value = {
    show: true,
    mode: 'edit',
    annotation: ann,
    selectedText: ann.text,
    page: ann.page,
    rects: ann.rects,
    position: { 
      top: event ? event.clientY - 10 : 200, 
      left: event ? event.clientX + 10 : 300 
    },
  };
};

const handleSaveHighlight = async (payload: { color: PdfAnnotation['color']; note: string; text: string }) => {
  const s = popupState.value;
  await annotations.createHighlight({
    fileId: props.fileId,
    pdfPath: props.filePath,
    pdfTitle: renderer.pdfTitle.value,
    page: s.page,
    text: payload.text || s.selectedText,
    rects: s.rects,
    color: payload.color,
    note: payload.note,
  });
  popupState.value.show = false;
  window.getSelection()?.removeAllRanges();
};

const handleUpdateHighlight = async (payload: { color: PdfAnnotation['color']; note: string; text: string }) => {
  if (!popupState.value.annotation) return;
  await annotations.updateAnnotation(popupState.value.annotation.id, {
    color: payload.color,
    note: payload.note,
    text: payload.text,
  });
  popupState.value.show = false;
};

const handleDeleteHighlight = async () => {
  if (!popupState.value.annotation) return;
  await annotations.deleteAnnotation(popupState.value.annotation.id);
  popupState.value.show = false;
};

// ─── Drawing state ───────────────────────────────────────────
const drawColor = ref('#ef4444');
const drawSize = ref(3);

const handleDrawingSave = async (pageNum: number, strokes: any[]) => {
  await annotations.saveDrawing(props.fileId, props.filePath, renderer.pdfTitle.value, pageNum, strokes);
};

// ─── Mode toggles ────────────────────────────────────────────
const toggleHighlight = () => {
  highlightMode.value = !highlightMode.value;
  if (highlightMode.value) drawMode.value = false;
};

const toggleDraw = () => {
  drawMode.value = !drawMode.value;
  if (drawMode.value) highlightMode.value = false;
};

// ─── Sidebar: scroll to annotation page ──────────────────────
const handleGoToAnnotation = (ann: PdfAnnotation) => {
  scrollToPage(ann.page);
};

// ─── Export annotated PDF ────────────────────────────────────
const isExporting = ref(false);
const exportAnnotatedPdf = async () => {
  if (isExporting.value) return;
  isExporting.value = true;
  try {
    const exportPath = await invoke<string>('export_annotated_pdf', {
      vaultPath: props.vaultPath,
      pdfPath: props.filePath,
      annotations: annotations.annotations.value.map(a => ({
        page: a.page,
        color: a.color,
        text: a.text,
        rects: a.rects,
        note: a.content,
      })),
    });
    console.log('Exported to:', exportPath);
  } catch (e) {
    console.error('Failed to export annotated PDF:', e);
  } finally {
    isExporting.value = false;
  }
};

// ─── Export to Markdown note ─────────────────────────────────
const exportToNote = async () => {
  const md = annotations.exportToMarkdown(renderer.pdfTitle.value);
  if (!md) return;
  try {
    const relPath = `Notes/${renderer.pdfTitle.value} — Annotations.md`;
    await invoke('write_node_file', {
      vaultPath: props.vaultPath,
      relPath,
      title: `${renderer.pdfTitle.value} — Annotations`,
      nodeType: 'note',
      properties: {},
      content: md,
    });
  } catch (e) {
    console.error('Failed to export annotations to note:', e);
  }
};

// ─── Reset PDF ───────────────────────────────────────────────
const showConfirmReset = ref(false);

const handleResetPdf = async () => {
  await annotations.clearAllAnnotations(props.filePath);
  showConfirmReset.value = false;
};
</script>

<template>
  <div class="flex-1 flex overflow-hidden bg-[#f0f0f0] dark:bg-[#1a1a1a]">
    <!-- Main viewer area -->
    <div class="flex-1 flex flex-col overflow-hidden">
      <!-- Toolbar -->
      <div class="flex items-center justify-center gap-1.5 px-4 py-2 bg-white/80 dark:bg-[#222]/80 backdrop-blur border-b border-gray-200/50 dark:border-white/5 flex-shrink-0">
        <!-- Navigation -->
        <button @click="prevPage" :disabled="renderer.currentPage.value <= 1" class="p-1.5 rounded-lg hover:bg-gray-100 dark:hover:bg-white/10 text-gray-600 dark:text-gray-300 disabled:opacity-30 cursor-pointer"><ChevronLeft class="w-4 h-4" /></button>
        <span class="text-xs font-mono text-gray-500 min-w-[60px] text-center">{{ renderer.currentPage.value }} / {{ renderer.totalPages.value }}</span>
        <button @click="nextPage" :disabled="renderer.currentPage.value >= renderer.totalPages.value" class="p-1.5 rounded-lg hover:bg-gray-100 dark:hover:bg-white/10 text-gray-600 dark:text-gray-300 disabled:opacity-30 cursor-pointer"><ChevronRight class="w-4 h-4" /></button>
        <div class="w-px h-5 bg-gray-200 dark:bg-white/10 mx-1" />

        <!-- Zoom -->
        <button @click="zoomOut" class="p-1.5 rounded-lg hover:bg-gray-100 dark:hover:bg-white/10 text-gray-600 dark:text-gray-300 cursor-pointer"><ZoomOut class="w-4 h-4" /></button>
        <span class="text-xs font-mono text-gray-500 w-10 text-center">{{ zoomPercent() }}%</span>
        <button @click="zoomIn" class="p-1.5 rounded-lg hover:bg-gray-100 dark:hover:bg-white/10 text-gray-600 dark:text-gray-300 cursor-pointer"><ZoomIn class="w-4 h-4" /></button>
        <div class="w-px h-5 bg-gray-200 dark:bg-white/10 mx-1" />

        <!-- Dark mode -->
        <button @click="darkMode = !darkMode" class="p-1.5 rounded-lg hover:bg-gray-100 dark:hover:bg-white/10 text-gray-600 dark:text-gray-300 cursor-pointer">
          <Moon v-if="!darkMode" class="w-4 h-4" /><Sun v-else class="w-4 h-4" />
        </button>
        <div class="w-px h-5 bg-gray-200 dark:bg-white/10 mx-1" />

        <!-- Annotation Tools -->
        <button @click="toggleHighlight" :class="highlightMode ? 'bg-amber-100 dark:bg-amber-500/20 text-amber-600 dark:text-amber-400' : 'text-gray-600 dark:text-gray-300 hover:bg-gray-100 dark:hover:bg-white/10'" class="p-1.5 rounded-lg cursor-pointer transition-colors" title="Highlight mode">
          <Highlighter class="w-4 h-4" />
        </button>
        <button @click="toggleDraw" :class="drawMode ? 'bg-red-100 dark:bg-red-500/20 text-red-600 dark:text-red-400' : 'text-gray-600 dark:text-gray-300 hover:bg-gray-100 dark:hover:bg-white/10'" class="p-1.5 rounded-lg cursor-pointer transition-colors" title="Draw mode">
          <PenTool class="w-4 h-4" />
        </button>

        <!-- Draw options (visible only in draw mode) -->
        <template v-if="drawMode">
          <div class="w-px h-5 bg-gray-200 dark:bg-white/10 mx-0.5" />
          <input v-model="drawColor" type="color" class="w-6 h-6 rounded cursor-pointer border-0 p-0 bg-transparent" title="Pen color" />
          <select v-model.number="drawSize" class="text-xs bg-gray-100 dark:bg-white/10 rounded px-1.5 py-1 text-gray-600 dark:text-gray-300 border-0 cursor-pointer">
            <option :value="2">Thin</option>
            <option :value="3">Normal</option>
            <option :value="5">Thick</option>
            <option :value="8">Bold</option>
          </select>
        </template>
        <div class="w-px h-5 bg-gray-200 dark:bg-white/10 mx-1" />

        <!-- Reset -->
        <button @click="showConfirmReset = true" :disabled="annotations.annotations.value.length === 0 && annotations.drawings.value.length === 0"
          class="p-1.5 rounded-lg hover:bg-red-100 dark:hover:bg-red-500/20 text-gray-600 dark:text-gray-300 hover:text-red-500 dark:hover:text-red-400 disabled:opacity-30 cursor-pointer" title="Clear all annotations">
          <RotateCcw class="w-4 h-4" />
        </button>

        <!-- Export -->
        <button @click="exportAnnotatedPdf" :disabled="isExporting || annotations.annotations.value.length === 0"
          class="p-1.5 rounded-lg hover:bg-gray-100 dark:hover:bg-white/10 text-gray-600 dark:text-gray-300 disabled:opacity-30 cursor-pointer" title="Export annotated PDF">
          <FileDown class="w-4 h-4" />
        </button>

        <!-- Sidebar toggle -->
        <button @click="showSidebar = !showSidebar" :class="showSidebar ? 'bg-indigo-100 dark:bg-indigo-500/20 text-indigo-600 dark:text-indigo-400' : 'text-gray-600 dark:text-gray-300 hover:bg-gray-100 dark:hover:bg-white/10'" class="p-1.5 rounded-lg cursor-pointer transition-colors" title="Annotation panel">
          <PanelRightClose v-if="showSidebar" class="w-4 h-4" />
          <PanelRightOpen v-else class="w-4 h-4" />
        </button>
      </div>

      <!-- PDF Content -->
      <div ref="containerRef" class="pdf-viewer-container pdfViewer flex-1 overflow-auto px-4 py-6" :class="{ 'pdf-dark-mode': darkMode }">
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
            class="pdf-page page relative bg-white shadow-lg rounded-sm" :style="[getPageStyle(pageNum), { '--scale-factor': renderer.scale.value }]">
            <!-- PDF Canvas -->
            <canvas :ref="(el: any) => { if (el) registerPageRef(pageNum, el as HTMLCanvasElement, (el as HTMLCanvasElement)?.nextElementSibling as HTMLDivElement) }" class="block w-full h-full" />
            <!-- Text Layer -->
            <div class="textLayer absolute top-0 left-0 overflow-hidden select-text"
              :class="highlightMode ? 'z-20 cursor-text' : 'z-0'" />
            <!-- Annotation Overlay (highlight rects) -->
            <AnnotationOverlay
              :annotations="annotations.annotations.value"
              :page="pageNum"
              :scale="renderer.scale.value"
              @click-annotation="handleClickAnnotation"
            />
            <!-- Drawing Overlay -->
            <DrawingOverlay
              :page="pageNum"
              :width="pageDimensions.get(pageNum)?.width || 612"
              :height="pageDimensions.get(pageNum)?.height || 792"
              :color="drawColor"
              :size="drawSize"
              :active="drawMode"
              :initial-strokes="annotations.getPageDrawingStrokes(pageNum).value"
              @save="(strokes: any[]) => handleDrawingSave(pageNum, strokes)"
            />
            <!-- Page number -->
            <div class="absolute bottom-2 right-3 text-[10px] text-gray-400 font-mono select-none pointer-events-none">{{ pageNum }}</div>
          </div>
        </div>
      </div>
    </div>

    <!-- Annotation Sidebar -->
    <AnnotationSidebar
      v-if="showSidebar"
      :annotations="annotations.annotations.value"
      :pdf-title="renderer.pdfTitle.value"
      @go-to="handleGoToAnnotation"
      @delete="(id: string) => annotations.deleteAnnotation(id)"
      @export-note="exportToNote"
    />

    <!-- Annotation Popup -->
    <AnnotationPopup
      :show="popupState.show"
      :mode="popupState.mode"
      :annotation="popupState.annotation"
      :selected-text="popupState.selectedText"
      :position="popupState.position"
      @close="popupState.show = false"
      @save="handleSaveHighlight"
      @update="handleUpdateHighlight"
      @delete="handleDeleteHighlight"
    />

    <!-- Reset Confirm Modal -->
    <ConfirmModal
      :show="showConfirmReset"
      title="Reset PDF"
      message="Are you sure you want to clear all annotations and drawings for this PDF? This action cannot be undone."
      confirm-text="Clear All"
      :is-destructive="true"
      @confirm="handleResetPdf"
      @cancel="showConfirmReset = false"
    />
  </div>
</template>

<style scoped>
.pdf-viewer-container { background: #f0f0f0; scroll-behavior: smooth; }
:is(.dark) .pdf-viewer-container { background: #1a1a1a; }
.pdf-dark-mode .pdf-page canvas { filter: invert(0.88) hue-rotate(180deg); }
/* pdfjs official CSS is imported globally; only add selection highlight color */
.textLayer :deep(span::selection) { background: rgba(0, 100, 200, 0.3); }
</style>
