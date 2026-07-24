<script setup lang="ts">
import { ref, computed, onMounted, onBeforeUnmount, onUnmounted } from 'vue';
import { NodeViewWrapper, nodeViewProps } from '@tiptap/vue-3';
import { AlignLeft, AlignCenter, AlignRight, RotateCcw, RotateCw, Trash2, Maximize2, X } from 'lucide-vue-next';

const props = defineProps(nodeViewProps);

const imageRef = ref<HTMLElement | null>(null);
const isResizing = ref(false);

const width = computed({
  get: () => props.node.attrs.width || 'auto',
  set: (val) => props.updateAttributes({ width: val })
});

const height = computed({
  get: () => props.node.attrs.height || 'auto',
  set: (val) => props.updateAttributes({ height: val })
});

const align = computed({
  get: () => props.node.attrs.align || 'center',
  set: (val) => props.updateAttributes({ align: val })
});

const rotation = computed({
  get: () => props.node.attrs.rotation || 0,
  set: (val) => props.updateAttributes({ rotation: val })
});

const caption = computed({
  get: () => props.node.attrs.caption || '',
  set: (val) => props.updateAttributes({ caption: val })
});

const intrinsicWidth = ref<number | null>(null);
const intrinsicHeight = ref<number | null>(null);

const onImageLoad = (e: Event) => {
  const img = e.target as HTMLImageElement;
  intrinsicWidth.value = img.offsetWidth;
  intrinsicHeight.value = img.offsetHeight;
};

const finalWidth = computed(() => {
  const w = width.value;
  if (w !== 'auto' && !isNaN(Number(w))) return Number(w);
  return intrinsicWidth.value || 'auto';
});

const finalHeight = computed(() => {
  const h = height.value;
  if (h !== 'auto' && !isNaN(Number(h))) return Number(h);
  return intrinsicHeight.value || 'auto';
});

const isRotated90 = computed(() => {
  const rot = ((rotation.value % 360) + 360) % 360;
  return rot === 90 || rot === 270;
});

const containerRef = ref<HTMLElement | null>(null);
const containerWidth = ref(10000);
let resizeObserver: ResizeObserver | null = null;

onMounted(() => {
  if (containerRef.value && containerRef.value.parentElement) {
    const parentEl = containerRef.value.parentElement;
    containerWidth.value = parentEl.clientWidth;
    resizeObserver = new ResizeObserver((entries) => {
      for (const entry of entries) {
        if (entry.contentRect.width > 0) {
          containerWidth.value = entry.contentRect.width;
        }
      }
    });
    resizeObserver.observe(parentEl);
  }
});

onBeforeUnmount(() => {
  if (resizeObserver) resizeObserver.disconnect();
});

const scaleFactor = computed(() => {
  const currentRotatedWidth = isRotated90.value ? finalHeight.value : finalWidth.value;
  const maxW = containerWidth.value - 4; // Add tiny padding to prevent precision overflow
  if (typeof currentRotatedWidth === 'number' && currentRotatedWidth > maxW && maxW > 0) {
    return maxW / currentRotatedWidth;
  }
  return 1;
});

const displayedWidth = computed(() => {
  const w = finalWidth.value;
  if (w === 'auto') return 'auto';
  return w * scaleFactor.value;
});

const displayedHeight = computed(() => {
  const h = finalHeight.value;
  if (h === 'auto') return 'auto';
  return h * scaleFactor.value;
});

// Resizing logic
let startX = 0;
let startY = 0;
let startWidth = 0;
let startHeight = 0;
let aspectRatio = 1;
let resizeDirection = '';

const startResize = (e: MouseEvent, direction: string) => {
  e.preventDefault();
  e.stopPropagation();
  isResizing.value = true;
  resizeDirection = direction;
  startX = e.clientX;
  startY = e.clientY;
  
  if (imageRef.value) {
    startWidth = typeof finalWidth.value === 'number' ? finalWidth.value : imageRef.value.offsetWidth;
    startHeight = typeof finalHeight.value === 'number' ? finalHeight.value : imageRef.value.offsetHeight;
    aspectRatio = startWidth / startHeight;
  }

  window.addEventListener('mousemove', onMouseMove);
  window.addEventListener('mouseup', stopResize);
};

const onMouseMove = (e: MouseEvent) => {
  if (!isResizing.value) return;
  
  // Calculate raw deltas and adjust for display scale
  let dx = (e.clientX - startX) / scaleFactor.value;
  let dy = (e.clientY - startY) / scaleFactor.value;
  
  // Adjust deltas based on rotation to keep resizing somewhat intuitive
  // For 90 or 270 deg, dx and dy effectively swap meaning for the element's actual width/height
  const currentRotation = ((rotation.value % 360) + 360) % 360;
  if (currentRotation === 90) {
    const temp = dx;
    dx = dy;
    dy = -temp;
  } else if (currentRotation === 180) {
    dx = -dx;
    dy = -dy;
  } else if (currentRotation === 270) {
    const temp = dx;
    dx = -dy;
    dy = temp;
  }

  let newWidth = startWidth;
  let newHeight = startHeight;

  if (resizeDirection.includes('right')) {
    newWidth = startWidth + dx;
  } else if (resizeDirection.includes('left')) {
    newWidth = startWidth - dx;
  }

  if (resizeDirection.includes('bottom')) {
    newHeight = startHeight + dy;
  } else if (resizeDirection.includes('top')) {
    newHeight = startHeight - dy;
  }

  // Enforce aspect ratio
  if (Math.abs(dx) > Math.abs(dy)) {
    newHeight = newWidth / aspectRatio;
  } else {
    newWidth = newHeight * aspectRatio;
  }

  // Minimum size
  width.value = Math.max(50, newWidth);
  height.value = Math.max(50, newHeight);
};

const stopResize = () => {
  isResizing.value = false;
  window.removeEventListener('mousemove', onMouseMove);
  window.removeEventListener('mouseup', stopResize);
};

const deleteNode = () => {
  props.deleteNode();
};

const rotate = (deg: number) => {
  rotation.value = (rotation.value + deg) % 360;
};

// --- Lightbox ---
const lightboxOpen = ref(false);

const openLightbox = () => lightboxOpen.value = true;
const closeLightbox = () => lightboxOpen.value = false;

const handleKeydown = (e: KeyboardEvent) => {
  if (!lightboxOpen.value) return;
  if (e.key === 'Escape') closeLightbox();
};

onMounted(() => window.addEventListener('keydown', handleKeydown));
onUnmounted(() => window.removeEventListener('keydown', handleKeydown));
</script>

<template>
  <NodeViewWrapper
    class="image-node-wrapper"
    :class="{ 'is-selected': selected, [`align-${align}`]: true }"
  >
    <div ref="containerRef" class="image-container" :class="{ 'is-resizing': isResizing }">
      <!-- Toolbar -->
      <div v-if="selected" class="image-toolbar w-max flex items-center gap-1 p-1 bg-surface dark:bg-surface-dark border border-border dark:border-border-dark rounded-lg shadow-lg">
        <button @mousedown.stop.prevent="align = 'left'" @touchstart.stop.prevent="align = 'left'" class="p-1.5 rounded text-text-secondary dark:text-text-secondary-dark hover:bg-surface-hover dark:hover:bg-surface-hover-dark hover:text-text dark:hover:text-text-dark transition-colors" :class="{ 'bg-accent/10 text-accent dark:text-accent-dark': align === 'left' }" title="Align Left"><AlignLeft class="w-4 h-4"/></button>
        <button @mousedown.stop.prevent="align = 'center'" @touchstart.stop.prevent="align = 'center'" class="p-1.5 rounded text-text-secondary dark:text-text-secondary-dark hover:bg-surface-hover dark:hover:bg-surface-hover-dark hover:text-text dark:hover:text-text-dark transition-colors" :class="{ 'bg-accent/10 text-accent dark:text-accent-dark': align === 'center' }" title="Align Center"><AlignCenter class="w-4 h-4"/></button>
        <button @mousedown.stop.prevent="align = 'right'" @touchstart.stop.prevent="align = 'right'" class="p-1.5 rounded text-text-secondary dark:text-text-secondary-dark hover:bg-surface-hover dark:hover:bg-surface-hover-dark hover:text-text dark:hover:text-text-dark transition-colors" :class="{ 'bg-accent/10 text-accent dark:text-accent-dark': align === 'right' }" title="Align Right"><AlignRight class="w-4 h-4"/></button>
        <div class="w-[1px] h-4 bg-border dark:bg-border-dark mx-1"></div>
        <button @mousedown.stop.prevent="rotate(-90)" @touchstart.stop.prevent="rotate(-90)" class="p-1.5 rounded text-text-secondary dark:text-text-secondary-dark hover:bg-surface-hover dark:hover:bg-surface-hover-dark hover:text-text dark:hover:text-text-dark transition-colors" title="Rotate Left"><RotateCcw class="w-4 h-4"/></button>
        <button @mousedown.stop.prevent="rotate(90)" @touchstart.stop.prevent="rotate(90)" class="p-1.5 rounded text-text-secondary dark:text-text-secondary-dark hover:bg-surface-hover dark:hover:bg-surface-hover-dark hover:text-text dark:hover:text-text-dark transition-colors" title="Rotate Right"><RotateCw class="w-4 h-4"/></button>
        <div class="w-[1px] h-4 bg-border dark:bg-border-dark mx-1"></div>
        <button @mousedown.stop.prevent="deleteNode" @touchstart.stop.prevent="deleteNode" class="p-1.5 rounded text-text-secondary dark:text-text-secondary-dark transition-colors text-danger hover:bg-danger/10" title="Delete"><Trash2 class="w-4 h-4"/></button>
      </div>

      <!-- Bounding Box -->
      <div class="bounding-box relative flex items-center justify-center transition-all duration-300"
           :style="{
             width: isRotated90 ? (displayedHeight === 'auto' ? 'auto' : `${displayedHeight}px`) : (displayedWidth === 'auto' ? 'auto' : `${displayedWidth}px`),
             height: isRotated90 ? (displayedWidth === 'auto' ? 'auto' : `${displayedWidth}px`) : (displayedHeight === 'auto' ? 'auto' : `${displayedHeight}px`),
           }">
        
        <!-- Rotator wrapper -->
        <div class="image-wrapper relative group inline-flex"
             :style="{
               width: displayedWidth === 'auto' ? 'auto' : `${displayedWidth}px`,
               height: displayedHeight === 'auto' ? 'auto' : `${displayedHeight}px`,
               transform: `rotate(${rotation}deg)`,
               transition: isResizing ? 'none' : 'transform 0.3s ease'
             }">
          <img
            ref="imageRef"
            :src="node.attrs.src"
            :alt="node.attrs.alt"
            @load="onImageLoad"
            @dblclick.stop="openLightbox"
            :style="{
               width: '100%',
               height: '100%'
            }"
            class="block rounded-lg shadow-sm border border-transparent object-fill"
            :class="{ 'border-accent dark:border-accent-dark ring-2 ring-accent/20 dark:ring-accent-dark/20': selected }"
          />

          <div 
            class="absolute inset-0 opacity-0 transition-opacity duration-200 pointer-events-none rounded-lg"
            :class="{ 'opacity-100 pointer-events-auto': selected }"
          >
            <button 
              v-show="selected"
              @mousedown.stop.prevent="openLightbox"
              @touchstart.stop.prevent="openLightbox"
              class="absolute top-2 right-2 p-1.5 bg-black/50 hover:bg-black/80 text-white rounded-full backdrop-blur-sm transition-colors pointer-events-auto"
              title="Expand Image"
            >
              <Maximize2 class="w-4 h-4" />
            </button>
          </div>
          
          <!-- Resize Handles -->
          <template v-if="selected">
            <div class="resize-handle top-left" @mousedown.stop="startResize($event, 'top-left')"></div>
            <div class="resize-handle top-right" @mousedown.stop="startResize($event, 'top-right')"></div>
            <div class="resize-handle bottom-left" @mousedown.stop="startResize($event, 'bottom-left')"></div>
            <div class="resize-handle bottom-right" @mousedown.stop="startResize($event, 'bottom-right')"></div>
          </template>
        </div>
      </div>
      
      <!-- Caption -->
      <input
        v-if="selected || caption"
        v-model="caption"
        type="text"
        placeholder="Add a caption..."
        class="caption-input select-text cursor-text mt-2 text-sm text-center bg-transparent border-none outline-none text-text-secondary dark:text-text-secondary-dark placeholder-muted dark:placeholder-muted-dark w-full max-w-sm transition-opacity"
        :class="{ 'opacity-50': !selected && !caption }"
        @keydown.stop
        @mousedown.stop
      />
    </div>

    <!-- Lightbox Modal -->
    <Teleport to="body">
      <div 
        v-if="lightboxOpen" 
        class="fixed inset-0 z-[9999] bg-black/95 backdrop-blur-sm flex items-center justify-center"
        @click="closeLightbox"
      >
        <button 
          @click.stop="closeLightbox" 
          class="absolute top-4 right-4 p-2 text-white/70 hover:text-white bg-white/10 hover:bg-white/20 rounded-full backdrop-blur-md transition-all z-50"
         aria-label="Close Lightbox">
          <X class="w-6 h-6" />
        </button>

        <div class="relative max-w-[95vw] max-h-[95vh] flex flex-col items-center justify-center" @click.stop>
          <img 
            :src="node.attrs.src" 
            :alt="node.attrs.alt"
            :style="{ transform: `rotate(${rotation}deg)` }"
            class="max-w-full max-h-[85vh] object-contain rounded shadow-2xl transition-all duration-300"
          />
          <div v-if="caption" class="mt-4 max-w-2xl text-center">
            <p class="text-white text-lg font-medium bg-black/50 px-4 py-2 rounded-lg backdrop-blur-sm inline-block shadow-lg">
              {{ caption }}
            </p>
          </div>
        </div>
      </div>
    </Teleport>
  </NodeViewWrapper>
</template>

<style scoped>
.image-node-wrapper {
  display: flex;
  width: 100%;
  margin: 1.5rem 0;
}

.caption-input {
  cursor: text !important;
  user-select: text !important;
  -webkit-user-select: text !important;
}

.image-node-wrapper.align-left { justify-content: flex-start; }
.image-node-wrapper.align-center { justify-content: center; }
.image-node-wrapper.align-right { justify-content: flex-end; }

.image-container {
  position: relative;
  display: flex;
  flex-direction: column;
  align-items: center;
  max-width: 100%;
}

.image-toolbar {
  position: absolute;
  top: -44px;
  left: 50%;
  transform: translateX(-50%);
  z-index: 50;
}


.resize-handle {
  position: absolute;
  width: 10px;
  height: 10px;
  background: var(--color-surface, #ffffff);
  border: 2px solid var(--color-accent, #7c3aed);
  border-radius: 50%;
  z-index: 10;
}

.top-left { top: -5px; left: -5px; cursor: nwse-resize; }
.top-right { top: -5px; right: -5px; cursor: nesw-resize; }
.bottom-left { bottom: -5px; left: -5px; cursor: nesw-resize; }
.bottom-right { bottom: -5px; right: -5px; cursor: nwse-resize; }

.caption-input {
  transition: all 0.2s;
}
.caption-input:focus {
  opacity: 1;
}
</style>
