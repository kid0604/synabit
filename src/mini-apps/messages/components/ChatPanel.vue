<script setup lang="ts">
import { ref, computed, watch, nextTick, onMounted, onUnmounted } from 'vue';
import { useThrottleFn } from '@vueuse/core';
import { useI18n } from 'vue-i18n';
import { Send, Square, Sparkles, ImagePlus, WifiOff, AlertCircle } from 'lucide-vue-next';
import { open } from '@tauri-apps/plugin-dialog';
import { readFile } from '@tauri-apps/plugin-fs';
import type { SynMessage, SynToolCallEvent, SourceRef } from '../types';
import MessageBubble from './MessageBubble.vue';
import StreamingIndicator from './StreamingIndicator.vue';
import NotificationCard from './NotificationCard.vue';

const MAX_IMAGES = 4;

const props = defineProps<{
  messages: SynMessage[];
  streamingContent: string;
  isStreaming: boolean;
  toolCalls?: SynToolCallEvent[];
  vaultPath?: string;
  connectionLost?: boolean;
  chatError?: string | null;
}>();

const emit = defineEmits<{
  send: [message: string, images?: string[]];
  stop: [];
  'open-source': [source: SourceRef];
  'regenerate': [messageId: string];
  'notification-action': [notification: any];
}>();

const inputText = ref('');
const messagesContainer = ref<HTMLElement | null>(null);
const textareaRef = ref<HTMLTextAreaElement | null>(null);
const isComposing = ref(false);
const pendingImages = ref<string[]>([]);

const { t } = useI18n();

const isDragging = ref(false);

const handleDragOver = (e: DragEvent) => {
  e.preventDefault();
  if (e.dataTransfer?.types.includes('Files')) {
    isDragging.value = true;
  }
};

const handleDragLeave = (e: DragEvent) => {
  const rect = (e.currentTarget as HTMLElement).getBoundingClientRect();
  if (
    e.clientX <= rect.left || e.clientX >= rect.right ||
    e.clientY <= rect.top || e.clientY >= rect.bottom
  ) {
    isDragging.value = false;
  }
};

const handleDrop = async (e: DragEvent) => {
  e.preventDefault();
  isDragging.value = false;
  const files = e.dataTransfer?.files;
  if (!files) return;
  for (const file of Array.from(files)) {
    if (!file.type.startsWith('image/')) continue;
    if (pendingImages.value.length >= MAX_IMAGES) break;
    const reader = new FileReader();
    reader.onload = (ev) => {
      const dataUrl = ev.target?.result as string;
      const base64 = dataUrl.split(',')[1];
      if (base64) pendingImages.value.push(base64);
    };
    reader.readAsDataURL(file);
  }
};

// Auto-scroll to bottom when new messages arrive or streaming updates
const scrollToBottom = (behavior: ScrollBehavior = 'smooth') => {
  if (messagesContainer.value) {
    messagesContainer.value.scrollTo({
      top: messagesContainer.value.scrollHeight,
      behavior,
    });
  }
};

watch(
  () => props.messages.length,
  async () => {
    await nextTick();
    scrollToBottom();
  }
);

const throttledScroll = useThrottleFn(() => scrollToBottom(), 100);

watch(
  () => props.streamingContent,
  async () => {
    await nextTick();
    throttledScroll();
  }
);

onMounted(() => {
  scrollToBottom('instant');
  window.addEventListener('keydown', handleGlobalKeydown);
});

onUnmounted(() => {
  window.removeEventListener('keydown', handleGlobalKeydown);
});

const handleGlobalKeydown = (e: KeyboardEvent) => {
  if (e.key === 'Escape' && props.isStreaming) {
    emit('stop');
  }
};

// Create a virtual streaming message for display
const streamingMessage = computed<SynMessage | null>(() => {
  if (!props.isStreaming || !props.streamingContent) return null;
  return {
    id: '__streaming__',
    role: 'assistant',
    content: props.streamingContent,
    timestamp: new Date().toISOString(),
  };
});

// Auto-grow textarea
const adjustTextareaHeight = () => {
  if (textareaRef.value) {
    textareaRef.value.style.height = 'auto';
    textareaRef.value.style.height = Math.min(textareaRef.value.scrollHeight, 200) + 'px';
  }
};

const handleInput = () => {
  adjustTextareaHeight();
};

const handleKeydown = (e: KeyboardEvent) => {
  if (e.key === 'Enter' && !e.shiftKey) {
    if (e.isComposing || isComposing.value) {
      return; // Let IME handle the Enter key to commit text without duplicating
    }
    e.preventDefault();
    handleSend();
  }
};

// --- Image helpers ---

const uint8ArrayToBase64 = (bytes: Uint8Array): string => {
  let binary = '';
  for (let i = 0; i < bytes.length; i++) {
    binary += String.fromCharCode(bytes[i]);
  }
  return btoa(binary);
};

const handleAttachImage = async () => {
  if (pendingImages.value.length >= MAX_IMAGES) return;

  try {
    const selected = await open({
      multiple: true,
      filters: [{ name: 'Image', extensions: ['png', 'jpg', 'jpeg', 'gif', 'webp'] }],
    });
    if (!selected) return;

    const paths = Array.isArray(selected) ? selected : [selected];
    for (const filePath of paths) {
      if (pendingImages.value.length >= MAX_IMAGES) break;
      const bytes = await readFile(filePath);
      const base64 = uint8ArrayToBase64(bytes);
      pendingImages.value.push(base64);
    }
  } catch (e) {
    console.error('Failed to attach image:', e);
  }
};

const handlePaste = (event: ClipboardEvent) => {
  const items = event.clipboardData?.items;
  if (!items) return;

  for (const item of items) {
    if (item.type.startsWith('image/')) {
      event.preventDefault();
      if (pendingImages.value.length >= MAX_IMAGES) return;

      const file = item.getAsFile();
      if (!file) continue;

      const reader = new FileReader();
      reader.onload = (e) => {
        const dataUrl = e.target?.result as string;
        // Strip "data:image/...;base64," prefix — store raw base64
        const base64 = dataUrl.split(',')[1];
        if (base64 && pendingImages.value.length < MAX_IMAGES) {
          pendingImages.value.push(base64);
        }
      };
      reader.readAsDataURL(file);
    }
  }
};

const removeImage = (index: number) => {
  pendingImages.value.splice(index, 1);
};

// Whether the send button should be active
const canSend = computed(() => {
  return (inputText.value.trim() || pendingImages.value.length > 0) && !props.isStreaming;
});

const handleSend = () => {
  // Aggressive IME artifact cleanup:
  // 1. Normalize all line endings and remove zero-width chars
  // 2. Split into lines, trim and normalize spaces
  // 3. Remove empty lines
  // 4. Deduplicate consecutive identical lines (Vietnamese IME bug)
  const raw = inputText.value.replace(/\r\n?/g, '\n').replace(/[\u200B-\u200D\uFEFF]/g, '');
  const lines = raw.split('\n').map(l => l.trim().replace(/\s+/g, ' ')).filter(l => l.length > 0);
  const deduped = lines.filter((line, i, arr) => i === 0 || line.normalize('NFC').toLowerCase() !== arr[i - 1].normalize('NFC').toLowerCase());
  const text = deduped.join('\n');
  
  if (!text && pendingImages.value.length === 0) return;
  if (props.isStreaming) return;

  emit('send', text, pendingImages.value.length ? [...pendingImages.value] : undefined);
  inputText.value = '';
  pendingImages.value = [];
  if (textareaRef.value) {
    textareaRef.value.style.height = 'auto';
  }
};

const handleStop = () => {
  emit('stop');
};
</script>

<template>
  <div class="flex-1 flex flex-col min-h-0 relative" @dragover="handleDragOver" @dragleave="handleDragLeave" @drop="handleDrop">
    <!-- Drop zone overlay -->
    <Transition
      enter-active-class="transition-opacity duration-200"
      enter-from-class="opacity-0"
      enter-to-class="opacity-100"
      leave-active-class="transition-opacity duration-150"
      leave-from-class="opacity-100"
      leave-to-class="opacity-0"
    >
      <div
        v-if="isDragging"
        class="absolute inset-0 z-50 flex items-center justify-center bg-violet-500/10 border-2 border-dashed border-violet-400 rounded-xl backdrop-blur-sm"
      >
        <div class="text-center">
          <span class="text-4xl">📸</span>
          <p class="text-sm font-medium text-violet-400 mt-2">{{ t('syn.drop_images') }}</p>
        </div>
      </div>
    </Transition>
    <!-- Connection lost banner -->
    <Transition
      enter-active-class="transition-all duration-300 ease-out"
      enter-from-class="-translate-y-full opacity-0"
      enter-to-class="translate-y-0 opacity-100"
      leave-active-class="transition-all duration-200 ease-in"
      leave-from-class="translate-y-0 opacity-100"
      leave-to-class="-translate-y-full opacity-0"
    >
      <div
        v-if="connectionLost"
        class="flex items-center gap-2 px-4 py-2.5 bg-amber-500/10 border-b border-amber-500/20 text-amber-600 dark:text-amber-400 text-sm flex-shrink-0"
      >
        <WifiOff class="w-4 h-4 flex-shrink-0" />
        <span>{{ $t('syn.connection_lost') }}</span>
      </div>
    </Transition>

    <!-- Messages area -->
    <div
      ref="messagesContainer"
      class="flex-1 overflow-y-auto px-4 md:px-8 py-6"
    >
      <div class="max-w-3xl mx-auto flex flex-col gap-5">
        <!-- Empty state -->
        <div
          v-if="messages.length === 0 && !isStreaming"
          class="flex flex-col items-center justify-center py-20 text-gray-400 dark:text-gray-500"
        >
          <div class="w-14 h-14 rounded-2xl bg-gradient-to-br from-violet-500/10 to-purple-500/10 dark:from-violet-500/20 dark:to-purple-500/20 flex items-center justify-center mb-4">
            <Sparkles class="w-7 h-7 text-violet-500/60" />
          </div>
          <p class="text-sm">{{ $t('syn.start_conversation') }}</p>
        </div>

        <!-- Message list -->
        <template v-for="msg in messages" :key="msg.id">
          <NotificationCard 
            v-if="msg.notification" 
            :notification="msg.notification"
            @action="$emit('notification-action', $event)"
          />
          <MessageBubble
            v-else
            :message="msg"
            :vault-path="vaultPath"
            @open-source="$emit('open-source', $event)"
            @regenerate="$emit('regenerate', msg.id)"
          />
        </template>

        <!-- Streaming message -->
        <MessageBubble
          v-if="streamingMessage"
          :message="streamingMessage"
          :is-streaming="true"
          @open-source="$emit('open-source', $event)"
          @regenerate="() => {}"
        />

        <!-- Thinking indicator (shown when streaming but no content yet) -->
        <div
          v-if="isStreaming && !streamingContent"
          class="flex items-start gap-3"
          style="animation: messageIn 0.2s ease-out forwards;"
        >
          <div class="w-8 h-8 rounded-xl bg-gradient-to-br from-violet-500 to-purple-600 flex items-center justify-center flex-shrink-0 shadow-lg shadow-violet-500/20">
            <Sparkles class="w-4 h-4 text-white animate-pulse" />
          </div>
          <StreamingIndicator :tool-calls="toolCalls" />
        </div>

        <!-- Error message (shown when send fails) -->
        <div
          v-if="chatError && !isStreaming"
          class="flex items-start gap-3 px-4 py-3 bg-red-500/5 border border-red-500/20 rounded-xl text-sm"
          style="animation: messageIn 0.2s ease-out forwards;"
        >
          <AlertCircle class="w-4 h-4 text-red-400 flex-shrink-0 mt-0.5" />
          <div>
            <p class="text-red-400 font-medium">{{ $t('syn.send_error_title') }}</p>
            <p class="text-red-400/70 text-xs mt-0.5">{{ $t('syn.send_error_desc') }}</p>
          </div>
        </div>
      </div>
    </div>

    <!-- Input area -->
    <div class="flex-shrink-0 px-4 md:px-8 pb-4">
      <div class="max-w-3xl mx-auto">
        <div class="relative flex flex-col bg-white dark:bg-[#1e1f25] border border-gray-200 dark:border-gray-700/60 rounded-2xl shadow-lg shadow-black/5 dark:shadow-black/20 transition-all focus-within:border-violet-400 dark:focus-within:border-violet-500/50 focus-within:shadow-violet-500/10">

          <!-- Pending images preview -->
          <div v-if="pendingImages.length" class="flex gap-2 px-3 pt-3 pb-1 overflow-x-auto">
            <div
              v-for="(img, i) in pendingImages"
              :key="i"
              class="relative shrink-0 group/img"
            >
              <img
                :src="'data:image/png;base64,' + img"
                class="w-16 h-16 rounded-lg object-cover border border-gray-200 dark:border-gray-700 shadow-sm"
              />
              <button
                @click="removeImage(i)"
                class="absolute -top-1.5 -right-1.5 w-5 h-5 bg-red-500 hover:bg-red-600
                       text-white rounded-full flex items-center justify-center text-xs
                       opacity-0 group-hover/img:opacity-100 transition-opacity cursor-pointer shadow-sm"
                :title="$t('syn.remove_image')"
              >
                ✕
              </button>
            </div>

            <!-- Max images hint -->
            <div
              v-if="pendingImages.length >= MAX_IMAGES"
              class="flex items-center text-[11px] text-gray-400 dark:text-gray-500 pl-1"
            >
              {{ $t('syn.max_images') }}
            </div>
          </div>

          <!-- Textarea + buttons row -->
          <div class="flex items-end gap-2">
            <textarea
              ref="textareaRef"
              v-model="inputText"
              @input="handleInput"
              @keydown="handleKeydown"
              @paste="handlePaste"
              @compositionstart="isComposing = true"
              @compositionend="isComposing = false"
              :placeholder="$t('syn.input_placeholder')"
              :disabled="false"
              rows="1"
              class="flex-1 resize-none bg-transparent px-4 py-3.5 text-sm text-text dark:text-text-dark placeholder-gray-400 dark:placeholder-gray-500 outline-none max-h-[200px] leading-relaxed"
            />

            <!-- Attach + Send / Stop buttons -->
            <div class="flex items-center gap-1 pr-2 pb-2">
              <!-- Attach image button -->
              <button
                @click="handleAttachImage"
                :disabled="pendingImages.length >= MAX_IMAGES"
                class="p-2 rounded-xl transition-all cursor-pointer"
                :class="pendingImages.length >= MAX_IMAGES
                  ? 'text-gray-300 dark:text-gray-600 cursor-not-allowed'
                  : 'text-gray-400 dark:text-gray-500 hover:text-violet-500 dark:hover:text-violet-400 hover:bg-violet-50 dark:hover:bg-violet-500/10'"
                :title="$t('syn.attach_image')"
              >
                <ImagePlus class="w-5 h-5" />
              </button>

              <!-- Send / Stop -->
              <button
                v-if="isStreaming"
                @click="handleStop"
                class="p-2.5 rounded-xl bg-red-500 hover:bg-red-600 text-white transition-all cursor-pointer shadow-sm"
                :title="$t('syn.stop')"
              >
                <Square class="w-4 h-4" />
              </button>
              <button
                v-else
                @click="handleSend"
                :disabled="!canSend"
                class="p-2.5 rounded-xl transition-all cursor-pointer shadow-sm"
                :class="canSend
                  ? 'bg-gradient-to-r from-violet-500 to-purple-600 hover:from-violet-600 hover:to-purple-700 text-white shadow-violet-500/20'
                  : 'bg-gray-100 dark:bg-gray-800 text-gray-400 dark:text-gray-500 cursor-not-allowed'"
                :title="$t('syn.send')"
              >
                <Send class="w-4 h-4" />
              </button>
            </div>
          </div>
        </div>

        <!-- Bottom hint -->
        <p class="text-center text-[11px] text-gray-400 dark:text-gray-600 mt-2">
          {{ $t('syn.input_hint') }}
        </p>
      </div>
    </div>
  </div>
</template>

<style scoped>
@keyframes messageIn {
  from { opacity: 0; transform: translateY(8px); }
  to { opacity: 1; transform: translateY(0); }
}

textarea::-webkit-scrollbar {
  width: 4px;
}
textarea::-webkit-scrollbar-thumb {
  background: var(--color-border);
  border-radius: 2px;
}
.dark textarea::-webkit-scrollbar-thumb {
  background: var(--color-border-dark);
}
</style>
