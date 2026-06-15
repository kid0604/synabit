<script setup lang="ts">
import { computed, ref, watch } from 'vue';
import { useDebounceFn } from '@vueuse/core';
import { marked } from 'marked';
import { markedHighlight } from 'marked-highlight';
import hljs from 'highlight.js/lib/core';
import javascript from 'highlight.js/lib/languages/javascript';
import typescript from 'highlight.js/lib/languages/typescript';
import python from 'highlight.js/lib/languages/python';
import rust from 'highlight.js/lib/languages/rust';
import json from 'highlight.js/lib/languages/json';
import bash from 'highlight.js/lib/languages/bash';
import css from 'highlight.js/lib/languages/css';
import xml from 'highlight.js/lib/languages/xml';
import sql from 'highlight.js/lib/languages/sql';
import markdown from 'highlight.js/lib/languages/markdown';
import 'highlight.js/styles/github-dark.min.css';
import DOMPurify from 'dompurify';
import { Check, Bot, FileText, Image as ImageIcon, Wrench, ChevronDown, ChevronRight, RefreshCw, Clipboard } from 'lucide-vue-next';
import { convertFileSrc } from '@tauri-apps/api/core';
import { invoke } from '@tauri-apps/api/core';
import type { SynMessage } from '../types';

hljs.registerLanguage('javascript', javascript);
hljs.registerLanguage('js', javascript);
hljs.registerLanguage('typescript', typescript);
hljs.registerLanguage('ts', typescript);
hljs.registerLanguage('python', python);
hljs.registerLanguage('rust', rust);
hljs.registerLanguage('json', json);
hljs.registerLanguage('bash', bash);
hljs.registerLanguage('sh', bash);
hljs.registerLanguage('css', css);
hljs.registerLanguage('html', xml);
hljs.registerLanguage('xml', xml);
hljs.registerLanguage('sql', sql);
hljs.registerLanguage('markdown', markdown);
hljs.registerLanguage('md', markdown);

marked.use(markedHighlight({
  langPrefix: 'hljs language-',
  highlight(code, lang) {
    if (lang && hljs.getLanguage(lang)) {
      return hljs.highlight(code, { language: lang }).value;
    }
    return hljs.highlightAuto(code).value;
  },
}));

const props = defineProps<{
  message: SynMessage;
  isStreaming?: boolean;
  vaultPath?: string;
}>();

defineEmits<{
  'open-source': [source: string];
  'regenerate': [];
}>();

const copied = ref(false);
const showTools = ref(false);
const fullscreenImage = ref<string | null>(null);
const fullscreenImageType = ref<'base64' | 'file'>('base64');

const openImageFullscreen = (img: string) => {
  fullscreenImage.value = img;
  fullscreenImageType.value = 'base64';
};

const openFileImageFullscreen = (path: string) => {
  fullscreenImage.value = path;
  fullscreenImageType.value = 'file';
};

const fullscreenSrc = computed(() => {
  if (!fullscreenImage.value) return '';
  return fullscreenImageType.value === 'base64'
    ? 'data:image/png;base64,' + fullscreenImage.value
    : convertFileSrc(fullscreenImage.value);
});

const formatArgs = (args: Record<string, unknown>) => {
  try {
    return JSON.stringify(args);
  } catch {
    return String(args);
  }
};

// Configure marked for clean output
marked.setOptions({
  breaks: true,
  gfm: true,
});

const debouncedContent = ref(props.message.content);

const updateContent = useDebounceFn((content: string) => {
  debouncedContent.value = content;
}, 100);

watch(() => props.message.content, (newContent) => {
  if (props.isStreaming) {
    updateContent(newContent);
  } else {
    debouncedContent.value = newContent;
  }
}, { immediate: true });

const renderedContent = computed(() => {
  if (props.message.role === 'user') {
    return debouncedContent.value;
  }

  const rawHtml = marked.parse(debouncedContent.value) as string;
  return DOMPurify.sanitize(rawHtml, {
    ADD_TAGS: ['pre', 'code'],
    ADD_ATTR: ['class'],
  });
});

const IMAGE_EXTENSIONS = ['png', 'jpg', 'jpeg', 'gif', 'webp', 'svg', 'bmp', 'ico'];
const VIDEO_EXTENSIONS = ['mp4', 'mov', 'webm', 'avi', 'mkv'];
const MEDIA_EXTENSIONS = [...IMAGE_EXTENSIONS, ...VIDEO_EXTENSIONS];

/** Extract media file paths from search_files tool call results */
const fileMediaPreviews = ref<{ path: string; filename: string; type: 'image' | 'video' }[]>([]);

watch(() => props.message.tool_calls_log, (newLogs) => {
  if (props.message.role !== 'assistant' || !newLogs?.length) {
    if (fileMediaPreviews.value.length > 0) {
      fileMediaPreviews.value = [];
    }
    return;
  }
  
  const media: { path: string; filename: string; type: 'image' | 'video' }[] = [];
  
  for (const tc of newLogs) {
    if (tc.tool_name !== 'search_files') continue;
    
    const preview = tc.result_preview;
    
    // Try JSON.parse first (most reliable)
    try {
      const data = JSON.parse(preview);
      if (data.results && Array.isArray(data.results)) {
        for (const r of data.results) {
          const ext = (r.extension || '').toLowerCase();
          if (ext && MEDIA_EXTENSIONS.includes(ext) && r.path) {
            media.push({
              path: r.path,
              filename: r.filename || r.path.split('/').pop() || '',
              type: VIDEO_EXTENSIONS.includes(ext) ? 'video' : 'image',
            });
          }
        }
        continue;
      }
    } catch {
      // JSON truncated — fall back to regex
    }
    
    // Regex fallback for truncated JSON
    try {
      const pathMatches = preview.matchAll(/"path"\s*:\s*"([^"]+)"/g);
      const extMatches = preview.matchAll(/"extension"\s*:\s*"([^"]+)"/g);
      const nameMatches = preview.matchAll(/"filename"\s*:\s*"([^"]+)"/g);
      
      const paths = [...pathMatches].map(m => m[1]);
      const exts = [...extMatches].map(m => m[1].toLowerCase());
      const names = [...nameMatches].map(m => m[1]);
      
      for (let i = 0; i < paths.length; i++) {
        if (exts[i] && MEDIA_EXTENSIONS.includes(exts[i])) {
          media.push({
            path: paths[i],
            filename: names[i] || paths[i].split('/').pop() || '',
            type: VIDEO_EXTENSIONS.includes(exts[i]) ? 'video' : 'image',
          });
        }
      }
    } catch {
      // Ignore parse errors
    }
  }
  
  fileMediaPreviews.value = media;
}, { immediate: true, deep: true });

/** Open a local file using the OS default viewer */
const openFileWithOS = async (path: string) => {
  try {
    await invoke('open_local_file', { 
      vaultPath: props.vaultPath || '', 
      path 
    });
  } catch (e) {
    console.error('Failed to open file:', e);
  }
};

const formatTime = (iso: string) => {
  const d = new Date(iso);
  return d.toLocaleTimeString([], { hour: '2-digit', minute: '2-digit' });
};

const copyContent = async () => {
  try {
    await navigator.clipboard.writeText(props.message.content);
    copied.value = true;
    setTimeout(() => { copied.value = false; }, 2000);
  } catch {
    // Ignore clipboard errors
  }
};
</script>

<template>
  <div
    class="message-bubble flex gap-3 w-full"
    :class="message.role === 'user' ? 'flex-row-reverse' : 'flex-row'"
    style="animation: messageIn 0.25s ease-out forwards;"
  >
    <!-- Avatar (assistant only) -->
    <div
      v-if="message.role === 'assistant'"
      class="w-8 h-8 rounded-xl bg-gradient-to-br from-violet-500 to-purple-600 flex items-center justify-center flex-shrink-0 shadow-lg shadow-violet-500/20 mt-0.5"
    >
      <Bot class="w-4 h-4 text-white" />
    </div>

    <!-- Content -->
    <div
      class="group relative max-w-[80%] min-w-0"
      :class="message.role === 'user' ? 'flex flex-col items-end' : ''"
    >
      <!-- Bubble -->
      <div
        class="px-4 py-3 rounded-2xl text-sm leading-relaxed relative overflow-hidden"
        :class="message.role === 'user'
          ? 'bg-gradient-to-r from-violet-500 to-purple-600 text-white rounded-br-md shadow-lg shadow-violet-500/20'
          : 'bg-white dark:bg-[#1e1f25] border border-gray-100 dark:border-gray-800/60 rounded-tl-md shadow-sm'"
      >
        <!-- User message -->
        <template v-if="message.role === 'user'">
          <div class="whitespace-pre-wrap break-words">
            {{ message.content }}
          </div>
          
          <!-- Attached images -->
          <div v-if="message.images?.length" class="flex flex-wrap gap-2 mt-2">
            <img
              v-for="(img, i) in message.images"
              :key="i"
              :src="'data:image/png;base64,' + img"
              class="max-w-52 max-h-52 rounded-lg object-cover cursor-pointer
                     border border-white/20
                     hover:border-white/50 transition-colors"
              @click="openImageFullscreen(img)"
            />
          </div>
        </template>

        <!-- Assistant message: rendered markdown -->
        <div
          v-else
          class="prose prose-sm dark:prose-invert max-w-none
            prose-p:my-1.5 prose-p:leading-relaxed
            prose-pre:bg-gray-900 prose-pre:text-gray-100 prose-pre:rounded-lg prose-pre:my-3
            prose-code:text-violet-600 dark:prose-code:text-violet-400
            prose-headings:font-semibold prose-headings:mt-4 prose-headings:mb-2
            prose-ul:my-2 prose-ol:my-2 prose-li:my-0.5
            prose-a:text-violet-600 dark:prose-a:text-violet-400 prose-a:no-underline hover:prose-a:underline
            prose-blockquote:border-violet-300 dark:prose-blockquote:border-violet-600
            prose-strong:text-gray-900 dark:prose-strong:text-white"
          v-html="renderedContent"
        />

        <!-- Streaming cursor -->
        <span
          v-if="isStreaming && message.role === 'assistant'"
          class="inline-block w-0.5 h-4 bg-violet-500 ml-0.5 align-middle streaming-cursor"
        />

        <!-- Tool Calls Log (collapsible) -->
        <div v-if="message.role === 'assistant' && message.tool_calls_log?.length" class="mt-3 pt-3 border-t border-gray-100 dark:border-gray-800/50">
          <button
            @click="showTools = !showTools"
            class="flex items-center gap-1.5 text-[11px] font-medium text-gray-500 dark:text-gray-400 
                   hover:text-gray-700 dark:hover:text-gray-300 transition-colors cursor-pointer"
          >
            <Wrench class="w-3 h-3" />
            <span>{{ message.tool_calls_log.length }} tool call{{ message.tool_calls_log.length > 1 ? 's' : '' }}</span>
            <ChevronDown v-if="showTools" class="w-3 h-3" />
            <ChevronRight v-else class="w-3 h-3" />
          </button>
          
          <Transition
            enter-active-class="transition-all duration-200 ease-out"
            enter-from-class="max-h-0 opacity-0"
            enter-to-class="max-h-96 opacity-100"
            leave-active-class="transition-all duration-150 ease-in"
            leave-from-class="max-h-96 opacity-100"
            leave-to-class="max-h-0 opacity-0"
          >
            <div v-if="showTools" class="mt-2 space-y-1 overflow-hidden">
              <div
                v-for="(tc, i) in message.tool_calls_log"
                :key="i"
                class="flex items-start gap-2 px-2.5 py-1.5 rounded-md bg-gray-50 dark:bg-gray-800/30 text-[11px]"
              >
                <span class="text-violet-500 font-mono font-medium shrink-0">{{ tc.tool_name }}</span>
                <span class="text-gray-400 font-mono truncate">{{ formatArgs(tc.tool_args) }}</span>
                <span v-if="tc.result_preview" class="text-gray-500 dark:text-gray-400 truncate ml-auto">→ {{ tc.result_preview.slice(0, 80) }}</span>
              </div>
            </div>
          </Transition>
        </div>

        <!-- File Media Previews (when search_files found images/videos) -->
        <div v-if="fileMediaPreviews.length" 
             class="mt-3 pt-3 border-t border-gray-100 dark:border-gray-800/50">
          <div class="flex items-center gap-1.5 mb-2 text-[11px] font-medium text-gray-500 dark:text-gray-400">
            <ImageIcon class="w-3 h-3" />
            <span>{{ fileMediaPreviews.length }} file{{ fileMediaPreviews.length > 1 ? 's' : '' }}</span>
          </div>
          <div class="flex flex-wrap gap-2">
            <div
              v-for="(media, i) in fileMediaPreviews"
              :key="i"
              class="group/file relative cursor-pointer"
              @click="media.type === 'image' ? openFileImageFullscreen(media.path) : openFileWithOS(media.path)"
              @dblclick.stop="openFileWithOS(media.path)"
            >
              <!-- Image preview -->
              <img
                v-if="media.type === 'image'"
                :src="convertFileSrc(media.path)"
                :alt="media.filename"
                class="w-28 h-28 rounded-lg object-cover border border-gray-200 dark:border-gray-700
                       hover:border-violet-400 dark:hover:border-violet-500 transition-all
                       hover:shadow-lg hover:shadow-violet-500/10"
              />
              <!-- Video preview -->
              <div v-else class="w-28 h-28 rounded-lg border border-gray-200 dark:border-gray-700
                                 hover:border-violet-400 dark:hover:border-violet-500 transition-all
                                 hover:shadow-lg hover:shadow-violet-500/10
                                 bg-gray-900 flex items-center justify-center relative overflow-hidden">
                <video
                  :src="convertFileSrc(media.path)"
                  class="w-full h-full object-cover absolute inset-0"
                  muted preload="metadata"
                />
                <div class="absolute inset-0 bg-black/30 flex items-center justify-center">
                  <div class="w-8 h-8 rounded-full bg-white/90 flex items-center justify-center">
                    <svg class="w-4 h-4 text-gray-800 ml-0.5" viewBox="0 0 24 24" fill="currentColor">
                      <path d="M8 5v14l11-7z"/>
                    </svg>
                  </div>
                </div>
              </div>
              <!-- Filename overlay -->
              <div class="absolute bottom-0 left-0 right-0 bg-gradient-to-t from-black/70 to-transparent 
                          rounded-b-lg px-1.5 py-1 opacity-0 group-hover/file:opacity-100 transition-opacity">
                <span class="text-[9px] text-white truncate block">{{ media.filename }}</span>
              </div>
            </div>
          </div>
        </div>

        <!-- Source Citations (only for assistant messages with sources) -->
        <div v-if="message.role === 'assistant' && message.sources?.length" 
             class="flex flex-wrap gap-1.5 mt-3 pt-3 border-t border-gray-100 dark:border-gray-800/50">
          <button
            v-for="source in message.sources"
            :key="source"
            @click="$emit('open-source', source)"
            class="inline-flex items-center gap-1 px-2 py-0.5 text-[11px] font-medium rounded-md 
                   bg-violet-50 dark:bg-violet-900/20 text-violet-600 dark:text-violet-400 
                   hover:bg-violet-100 dark:hover:bg-violet-900/40 transition-colors cursor-pointer"
          >
            <FileText class="w-3 h-3" />
            {{ source }}
          </button>
        </div>
      </div>

      <!-- Hover action bar -->
      <div class="absolute top-2 right-2 opacity-0 group-hover:opacity-100 transition-opacity duration-150
                  flex items-center gap-0.5 bg-white dark:bg-gray-800 rounded-lg shadow-md border border-gray-100 dark:border-gray-700/50 px-1 py-0.5">
        <button
          @click="copyContent"
          class="p-1.5 rounded-md hover:bg-gray-100 dark:hover:bg-gray-700 text-gray-400 hover:text-gray-600 dark:hover:text-gray-300 transition-colors cursor-pointer"
          :title="copied ? $t('syn.copied') : $t('syn.copy')"
        >
          <Check v-if="copied" class="w-3.5 h-3.5 text-green-500" />
          <Clipboard v-else class="w-3.5 h-3.5" />
        </button>
        <button
          v-if="message.role === 'assistant'"
          @click="$emit('regenerate')"
          class="p-1.5 rounded-md hover:bg-gray-100 dark:hover:bg-gray-700 text-gray-400 hover:text-gray-600 dark:hover:text-gray-300 transition-colors cursor-pointer"
          :title="$t('syn.regenerate')"
        >
          <RefreshCw class="w-3.5 h-3.5" />
        </button>
      </div>

      <!-- Metadata row -->
      <div
        class="flex items-center gap-2 mt-1 px-1 opacity-0 group-hover:opacity-100 transition-opacity duration-150"
        :class="message.role === 'user' ? 'flex-row-reverse' : ''"
      >
        <span class="text-[11px] text-gray-400 dark:text-gray-500">
          {{ formatTime(message.timestamp) }}
        </span>
        <span
          v-if="message.role === 'assistant' && (message.tokens || message.duration_ms)"
          class="text-[11px] text-gray-400 dark:text-gray-500"
        >
          <template v-if="message.tokens">{{ message.tokens }} {{ $t('syn.tokens') }}</template>
          <template v-if="message.tokens && message.duration_ms"> · </template>
          <template v-if="message.duration_ms">{{ (message.duration_ms / 1000).toFixed(1) }}s</template>
        </span>
      </div>
    </div>
  </div>

  <!-- Fullscreen image lightbox -->
  <Teleport to="body">
    <Transition
      enter-active-class="transition-opacity duration-200 ease-out"
      enter-from-class="opacity-0"
      enter-to-class="opacity-100"
      leave-active-class="transition-opacity duration-150 ease-in"
      leave-from-class="opacity-100"
      leave-to-class="opacity-0"
    >
      <div
        v-if="fullscreenImage"
        class="fixed inset-0 z-50 bg-black/80 backdrop-blur-sm flex items-center justify-center cursor-pointer"
        @click="fullscreenImage = null"
      >
        <img
          :src="fullscreenSrc"
          class="max-w-[90vw] max-h-[90vh] rounded-xl object-contain shadow-2xl"
          @click.stop
        />
      </div>
    </Transition>
  </Teleport>
</template>

<style scoped>
@keyframes messageIn {
  from {
    opacity: 0;
    transform: translateY(8px);
  }
  to {
    opacity: 1;
    transform: translateY(0);
  }
}

@keyframes blink {
  0%, 100% { opacity: 1; }
  50% { opacity: 0; }
}

.streaming-cursor {
  animation: blink 0.8s ease-in-out infinite;
}

/* Code block styling for assistant messages */
:deep(pre) {
  position: relative;
  margin: 0.75rem 0;
  border-radius: 0.75rem;
  overflow-x: auto;
}

:deep(pre code) {
  display: block;
  padding: 1rem;
  font-size: 0.8rem;
  line-height: 1.6;
  font-family: 'JetBrains Mono', 'Fira Code', 'Menlo', monospace;
}

:deep(table) {
  border-collapse: collapse;
  width: 100%;
  margin: 0.5rem 0;
}

:deep(th), :deep(td) {
  border: 1px solid var(--color-border);
  padding: 0.4rem 0.75rem;
  text-align: left;
  font-size: 0.8rem;
}

:deep(th) {
  background-color: var(--color-surface-hover);
  font-weight: 600;
}

.dark :deep(th) {
  background-color: var(--color-surface-hover-dark);
}

.dark :deep(th), .dark :deep(td) {
  border-color: var(--color-border-dark);
}
</style>
