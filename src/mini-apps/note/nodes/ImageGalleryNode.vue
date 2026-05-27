<script setup lang="ts">
import { ref, computed, watch, onMounted, onUnmounted } from 'vue';
import { NodeViewWrapper, nodeViewProps } from '@tiptap/vue-3';
import { LayoutGrid, Columns, PanelTop, Images, Trash2, Plus, X, Maximize2, ChevronLeft, ChevronRight } from 'lucide-vue-next';
import { GalleryImage } from '../extensions/ImageGallery';
import { open } from '@tauri-apps/plugin-dialog';
import { readFile } from '@tauri-apps/plugin-fs';
import { invoke, convertFileSrc } from '@tauri-apps/api/core';

const props = defineProps(nodeViewProps);



const templateStyle = computed({
  get: () => props.node.attrs.template || 'classic',
  set: (val) => props.updateAttributes({ template: val })
});

const globalCaption = computed({
  get: () => props.node.attrs.caption || '',
  set: (val) => props.updateAttributes({ caption: val })
});

const activeImageIndex = ref<number | null>(null);
const focusedImageIndex = ref<number | null>(null);
const hoveredInputIndex = ref<number | null>(null);

// Local state for images to prevent focus loss during typing
const localImages = ref<GalleryImage[]>(JSON.parse(JSON.stringify(props.node.attrs.images || [])));

// Sync from Tiptap to local if it changes externally (e.g. undo/redo)
watch(() => props.node.attrs.images, (newVal) => {
  if (JSON.stringify(newVal) !== JSON.stringify(localImages.value)) {
    localImages.value = JSON.parse(JSON.stringify(newVal));
  }
}, { deep: true });

const syncImagesToTiptap = () => {
  // Deep clone to ensure Tiptap detects the attribute change
  props.updateAttributes({ images: JSON.parse(JSON.stringify(localImages.value)) });
};

// --- Lightbox State & Logic ---
const lightboxOpen = ref(false);
const lightboxIndex = ref(0);

const openLightbox = (index: number) => {
  lightboxIndex.value = index;
  lightboxOpen.value = true;
};

const closeLightbox = () => {
  lightboxOpen.value = false;
};

const nextImage = () => {
  if (lightboxIndex.value < localImages.value.length - 1) {
    lightboxIndex.value++;
  } else {
    lightboxIndex.value = 0;
  }
};

const prevImage = () => {
  if (lightboxIndex.value > 0) {
    lightboxIndex.value--;
  } else {
    lightboxIndex.value = localImages.value.length - 1;
  }
};

const handleKeydown = (e: KeyboardEvent) => {
  if (!lightboxOpen.value) return;
  if (e.key === 'Escape') closeLightbox();
  if (e.key === 'ArrowRight') nextImage();
  if (e.key === 'ArrowLeft') prevImage();
};

onMounted(() => {
  window.addEventListener('keydown', handleKeydown);
});

onUnmounted(() => {
  window.removeEventListener('keydown', handleKeydown);
});

const containerClass = computed(() => {
  const t = templateStyle.value;
  if (t === 'masonry') return 'columns-2 md:columns-3 gap-2 space-y-2 block';
  if (t === 'carousel') return 'flex overflow-x-auto gap-2 snap-x snap-mandatory pb-4 scrollbar-thin scrollbar-thumb-border';
  if (t === 'hero') return 'grid grid-cols-2 md:grid-cols-3 gap-2';
  
  // Classic
  const len = localImages.value.length;
  if (len === 1) return 'grid grid-cols-1 gap-2';
  if (len === 2 || len === 3 || len === 4) return 'grid grid-cols-2 gap-2';
  return 'grid grid-cols-2 md:grid-cols-6 gap-2'; // 5+
});

const getItemClass = (index: number) => {
  const t = templateStyle.value;
  const len = localImages.value.length;
  
  let classes = 'relative group/item overflow-hidden rounded-lg bg-surface-hover dark:bg-surface-hover-dark cursor-pointer ';
  
  if (t === 'masonry') return classes + 'break-inside-avoid w-full mb-2 inline-block h-auto';
  if (t === 'carousel') return classes + 'flex-none w-[85%] sm:w-[60%] md:w-[45%] snap-center h-64 md:h-80';
  
  if (t === 'hero') {
    if (index === 0) return classes + 'col-span-2 md:col-span-3 aspect-[4/3] sm:aspect-video md:aspect-[21/9]';
    return classes + 'col-span-1 aspect-square md:aspect-[4/3]';
  }
  
  // Classic
  if (len === 1) return classes + 'col-span-1 aspect-auto max-h-[70vh]';
  if (len === 2) return classes + 'col-span-1 aspect-square md:aspect-[4/3]';
  if (len === 3) {
    if (index === 0) return classes + 'col-span-2 aspect-[2/1] md:aspect-[21/9]';
    return classes + 'col-span-1 aspect-square md:aspect-[4/3]';
  }
  if (len === 4) return classes + 'col-span-1 aspect-square';
  
  if (index === 0 || index === 1) return classes + 'col-span-1 md:col-span-3 aspect-square md:aspect-[4/3]';
  return classes + 'col-span-1 md:col-span-2 aspect-square';
};

const deleteGallery = () => {
  props.deleteNode();
};

const removeImage = (index: number) => {
  localImages.value.splice(index, 1);
  if (localImages.value.length === 0) {
    deleteGallery();
  } else {
    syncImagesToTiptap();
  }
};

const moveImage = (index: number, direction: number) => {
  const newIndex = index + direction;
  if (newIndex < 0 || newIndex >= localImages.value.length) return;
  
  const arr = localImages.value;
  const temp = arr[index];
  arr[index] = arr[newIndex];
  arr[newIndex] = temp;
  
  syncImagesToTiptap();
};



const addImage = async () => {
  try {
    const selectedPaths = await open({
      multiple: true,
      filters: [{
        name: 'Image',
        extensions: ['png', 'jpeg', 'jpg', 'gif', 'webp']
      }]
    });
    
    if (selectedPaths && Array.isArray(selectedPaths) && props.extension.options.vaultPath) {

      const vaultPath = props.extension.options.vaultPath;
      
      for (const pathStr of selectedPaths) {
        const match = pathStr.match(/[\\\/]([^\\\/]+)$/);
        const filename = match ? match[1] : `image-${Date.now()}.png`;
        const buffer = await readFile(pathStr);
        
        const relativePath = await invoke<string>('save_asset', {
            vaultPath: vaultPath,
            filename: filename,
            bytes: Array.from(buffer)
        });
        const sep = vaultPath.includes('\\') ? '\\' : '/';
        const absPath = `${vaultPath}${sep}${relativePath}`;
        const renderUrl = convertFileSrc(absPath);
        
        localImages.value.push({
          src: renderUrl,
          alt: filename,
          caption: ''
        });
      }
      syncImagesToTiptap();
    }
  } catch (e) {
    console.error("Failed to add image to gallery", e);
  }
};

</script>

<template>
  <NodeViewWrapper class="gallery-node-wrapper my-6 relative group/wrapper">
    <!-- Toolbar -->
    <div v-if="selected" class="gallery-toolbar absolute -top-12 left-1/2 -translate-x-1/2 z-50 flex max-w-[90vw] overflow-x-auto scrollbar-none items-center gap-1 p-1 bg-surface dark:bg-surface-dark border border-border dark:border-border-dark rounded-lg shadow-lg">
      <button @mousedown.stop.prevent="templateStyle = 'classic'" @touchstart.stop.prevent="templateStyle = 'classic'" class="p-1.5 shrink-0 rounded text-text-secondary dark:text-text-secondary-dark hover:bg-surface-hover dark:hover:bg-surface-hover-dark hover:text-text dark:hover:text-text-dark transition-colors" :class="{ 'bg-accent/10 text-accent dark:text-accent-dark': templateStyle === 'classic' }" title="Classic Grid"><LayoutGrid class="w-4 h-4"/></button>
      <button @mousedown.stop.prevent="templateStyle = 'masonry'" @touchstart.stop.prevent="templateStyle = 'masonry'" class="p-1.5 shrink-0 rounded text-text-secondary dark:text-text-secondary-dark hover:bg-surface-hover dark:hover:bg-surface-hover-dark hover:text-text dark:hover:text-text-dark transition-colors" :class="{ 'bg-accent/10 text-accent dark:text-accent-dark': templateStyle === 'masonry' }" title="Masonry Waterfall"><Columns class="w-4 h-4"/></button>
      <button @mousedown.stop.prevent="templateStyle = 'hero'" @touchstart.stop.prevent="templateStyle = 'hero'" class="p-1.5 shrink-0 rounded text-text-secondary dark:text-text-secondary-dark hover:bg-surface-hover dark:hover:bg-surface-hover-dark hover:text-text dark:hover:text-text-dark transition-colors" :class="{ 'bg-accent/10 text-accent dark:text-accent-dark': templateStyle === 'hero' }" title="Hero Top"><PanelTop class="w-4 h-4"/></button>
      <button @mousedown.stop.prevent="templateStyle = 'carousel'" @touchstart.stop.prevent="templateStyle = 'carousel'" class="p-1.5 shrink-0 rounded text-text-secondary dark:text-text-secondary-dark hover:bg-surface-hover dark:hover:bg-surface-hover-dark hover:text-text dark:hover:text-text-dark transition-colors" :class="{ 'bg-accent/10 text-accent dark:text-accent-dark': templateStyle === 'carousel' }" title="Carousel"><Images class="w-4 h-4"/></button>
      <div class="w-[1px] h-4 bg-border dark:bg-border-dark mx-1"></div>
      <button @mousedown.stop.prevent="addImage" @touchstart.stop.prevent="addImage" class="p-1.5 rounded text-text-secondary dark:text-text-secondary-dark hover:bg-surface-hover dark:hover:bg-surface-hover-dark hover:text-text dark:hover:text-text-dark transition-colors" title="Add Images"><Plus class="w-4 h-4"/></button>
      <div class="w-[1px] h-4 bg-border dark:bg-border-dark mx-1"></div>
      <button @mousedown.stop.prevent="deleteGallery" @touchstart.stop.prevent="deleteGallery" class="p-1.5 rounded text-text-secondary dark:text-text-secondary-dark transition-colors text-danger hover:bg-danger/10" title="Delete Collection"><Trash2 class="w-4 h-4"/></button>
    </div>

    <!-- Gallery Container -->
    <div 
      class="gallery-container w-full transition-all duration-300 rounded-xl border border-transparent"
      :class="[containerClass, { 'ring-4 ring-accent/30 dark:ring-accent-dark/30': selected }]"
    >
      <div 
        v-for="(img, index) in localImages" 
        :key="index"
        :class="[
          getItemClass(index)
        ]"
        @mouseenter="activeImageIndex = index"
        @mouseleave="activeImageIndex = null"
        @dblclick.stop="openLightbox(index)"
      >
        <!-- Image -->
        <img 
          :src="img.src" 
          :alt="img.alt" 
          class="w-full object-cover object-center transition-transform duration-300 group-hover/item:scale-105"
          :class="templateStyle === 'masonry' ? 'h-auto' : 'h-full'"
        />
        
        <div 
          class="absolute inset-0 opacity-0 transition-opacity duration-200 pointer-events-none"
          :class="{ 'opacity-100': activeImageIndex === index || focusedImageIndex === index || img.caption, 'pointer-events-auto': activeImageIndex === index || focusedImageIndex === index }"
        >
          <!-- Expand Image Button -->
          <button 
            v-show="activeImageIndex === index"
            @mousedown.stop.prevent="openLightbox(index)"
            @touchstart.stop.prevent="openLightbox(index)"
            class="absolute top-2 left-2 p-1.5 bg-black/50 hover:bg-black/80 text-white rounded-full backdrop-blur-sm transition-colors pointer-events-auto"
            title="Expand Image"
          >
            <Maximize2 class="w-3.5 h-3.5" />
          </button>

          <!-- Reorder Buttons -->
          <div 
            v-show="activeImageIndex === index && localImages.length > 1"
            class="absolute top-2 left-1/2 -translate-x-1/2 flex items-center gap-1 p-1 bg-black/50 rounded-full backdrop-blur-sm pointer-events-auto"
          >
            <button 
              @mousedown.stop.prevent="moveImage(index, -1)"
              @touchstart.stop.prevent="moveImage(index, -1)"
              class="p-0.5 hover:bg-white/20 text-white rounded-full transition-colors disabled:opacity-30 disabled:hover:bg-transparent disabled:cursor-not-allowed"
              :disabled="index === 0"
              title="Move Left/Up"
            >
              <ChevronLeft class="w-3.5 h-3.5" />
            </button>
            <button 
              @mousedown.stop.prevent="moveImage(index, 1)"
              @touchstart.stop.prevent="moveImage(index, 1)"
              class="p-0.5 hover:bg-white/20 text-white rounded-full transition-colors disabled:opacity-30 disabled:hover:bg-transparent disabled:cursor-not-allowed"
              :disabled="index === localImages.length - 1"
              title="Move Right/Down"
            >
              <ChevronRight class="w-3.5 h-3.5" />
            </button>
          </div>

          <!-- Delete Single Image Button -->
          <button 
            v-show="activeImageIndex === index"
            @mousedown.stop.prevent="removeImage(index)"
            @touchstart.stop.prevent="removeImage(index)"
            class="absolute top-2 right-2 p-1.5 bg-black/50 hover:bg-danger/80 text-white rounded-full backdrop-blur-sm transition-colors pointer-events-auto"
            title="Remove Image"
          >
            <X class="w-3.5 h-3.5" />
          </button>

          <!-- Individual Caption Input -->
          <div class="absolute bottom-2 left-2 right-2 pointer-events-auto" @mousedown.stop @click.stop>
            <input
              v-show="activeImageIndex === index || focusedImageIndex === index || img.caption"
              v-model="img.caption"
              @focus="focusedImageIndex = index"
              @blur="focusedImageIndex = null; syncImagesToTiptap()"
              @keydown.enter.prevent="syncImagesToTiptap"
              type="text"
              placeholder="Image caption..."
              class="caption-input w-full bg-black/40 text-white placeholder-white/50 text-xs px-2 py-1.5 rounded border border-white/10 backdrop-blur-md outline-none focus:bg-black/60 transition-colors select-text cursor-text"
              draggable="false"
              @keydown.stop
              @mousedown.stop
              @click.stop="($event.target as HTMLInputElement)?.focus()"
              @mouseenter="hoveredInputIndex = index"
              @mouseleave="hoveredInputIndex = null"
            />
          </div>
        </div>
      </div>
    </div>
    
    <!-- Global Caption -->
    <input
      v-show="selected || globalCaption"
      v-model="globalCaption"
      type="text"
      placeholder="Collection caption..."
      class="caption-input select-text cursor-text mt-3 text-sm text-center bg-transparent border-none outline-none text-text-secondary dark:text-text-secondary-dark placeholder-muted dark:placeholder-muted-dark w-full max-w-sm mx-auto block transition-opacity"
      :class="{ 'opacity-50': !selected && !globalCaption }"
      @keydown.stop
      @mousedown.stop
    />

    <!-- Lightbox Modal -->
    <Teleport to="body">
      <div 
        v-if="lightboxOpen" 
        class="fixed inset-0 z-[9999] bg-black/95 backdrop-blur-sm flex items-center justify-center"
        @click="closeLightbox"
      >
        <!-- Close Button -->
        <button 
          @click.stop="closeLightbox" 
          class="absolute top-4 right-4 p-2 text-white/70 hover:text-white bg-white/10 hover:bg-white/20 rounded-full backdrop-blur-md transition-all z-50"
        >
          <X class="w-6 h-6" />
        </button>

        <!-- Navigation Prev -->
        <button 
          v-if="localImages.length > 1"
          @click.stop="prevImage" 
          class="absolute left-4 top-1/2 -translate-y-1/2 p-3 text-white/70 hover:text-white bg-white/10 hover:bg-white/20 rounded-full backdrop-blur-md transition-all z-50"
        >
          <ChevronLeft class="w-8 h-8" />
        </button>

        <!-- Navigation Next -->
        <button 
          v-if="localImages.length > 1"
          @click.stop="nextImage" 
          class="absolute right-4 top-1/2 -translate-y-1/2 p-3 text-white/70 hover:text-white bg-white/10 hover:bg-white/20 rounded-full backdrop-blur-md transition-all z-50"
        >
          <ChevronRight class="w-8 h-8" />
        </button>

        <!-- Main Image -->
        <div class="relative max-w-[95vw] max-h-[95vh] flex flex-col items-center justify-center" @click.stop>
          <img 
            :src="localImages[lightboxIndex].src" 
            :alt="localImages[lightboxIndex].alt"
            class="max-w-full max-h-[85vh] object-contain rounded shadow-2xl transition-all duration-300"
          />
          <!-- Caption -->
          <div class="mt-4 max-w-2xl text-center">
            <p v-if="localImages[lightboxIndex].caption" class="text-white text-lg font-medium bg-black/50 px-4 py-2 rounded-lg backdrop-blur-sm inline-block shadow-lg">
              {{ localImages[lightboxIndex].caption }}
            </p>
            <p v-else-if="globalCaption" class="text-white/70 text-md">
              {{ globalCaption }}
            </p>
          </div>
        </div>
      </div>
    </Teleport>
  </NodeViewWrapper>
</template>

<style scoped>
.gallery-node-wrapper {
  display: flex;
  flex-direction: column;
  align-items: center;
  width: 100%;
}

.caption-input {
  cursor: text !important;
  user-select: text !important;
  -webkit-user-select: text !important;
}
</style>
