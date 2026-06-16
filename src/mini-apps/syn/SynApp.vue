<script setup lang="ts">
import { ref, onMounted, onUnmounted, watch, computed } from 'vue';
import { invoke } from '@tauri-apps/api/core';
import { confirm } from '@tauri-apps/plugin-dialog';
import { useI18n } from 'vue-i18n';
import { PanelLeftClose, PanelLeft, Loader2, Settings, Download } from 'lucide-vue-next';
import { logger } from '../../utils/logger';
import NavButtons from '../../shared/components/NavButtons.vue';
import synAvatar from '../../assets/syn-avatar.jpg';

import ConversationList from './components/ConversationList.vue';
import ChatPanel from './components/ChatPanel.vue';
import WelcomeScreen from './components/WelcomeScreen.vue';
import ModelSelector from './components/ModelSelector.vue';
import SynSettings from './components/SynSettings.vue';
import ConfirmModal from '../../shared/components/ConfirmModal.vue';

import { useSynChat } from './composables/useSynChat';
import { useSynModels } from './composables/useSynModels';
import type { SynConversation, SynConversationFull, SynMessage } from './types';

const props = defineProps<{
  vaultPath: string;
}>();

const emit = defineEmits(['open-node']);

const handleOpenSource = (source: { id: string; title: string; node_type: string }) => {
  // Map source_type to the type expected by handleEditFromNexus
  const typeMap: Record<string, string> = {
    note: 'note',
    task: 'task',
    event: 'calendar',
    quickcap: 'quickcap',
    person: 'person',
    finance_month: 'finance_month',
    whiteboard: 'whiteboard',
    feed_source: 'feed_source',
    project: 'project',
  };
  const mappedType = typeMap[source.node_type] || 'note';
  emit('open-node', source.id, mappedType);
};

const { t } = useI18n();

// Composables
const {
  streamingContent,
  isStreaming,
  toolCalls,
  sendMessage,
  stopGeneration,
  clearStreaming,
} = useSynChat();

const {
  models,
  status,
  selectedModel,
  pullingModel,
  pullProgress,
  checkStatus,
  fetchModels,
  pullModel,
  formatModelSize,
} = useSynModels();

// State
const conversations = ref<SynConversation[]>([]);
const activeConversationId = ref<string | null>(null);
const activeMessages = ref<SynMessage[]>([]);
const showSidebar = ref(true);
const showSettings = ref(false);
const loading = ref(true);

// Computed
const isReady = computed(() => status.value.connected && models.value.length > 0);

const statusColor = computed(() => {
  if (!status.value.connected) return 'bg-red-500';
  if (isStreaming.value) return 'bg-amber-500 animate-pulse';
  return 'bg-green-500';
});

const statusText = computed(() => {
  if (!status.value.connected) return t('syn.status_disconnected');
  if (isStreaming.value) return t('syn.status_generating');
  return t('syn.status_connected');
});

const activeModelName = computed(() => {
  if (!selectedModel.value) return '';
  const model = models.value.find(m => m.name === selectedModel.value);
  if (!model) return selectedModel.value;
  return model.name;
});

// Load conversations
const loadConversations = async () => {
  try {
    const list = await invoke<SynConversation[]>('syn_list_conversations', { vaultPath: props.vaultPath });
    conversations.value = list;
  } catch (e) {
    logger.error('[Syn] Failed to load conversations', e);
  }
};

// Load a specific conversation
const loadConversation = async (id: string) => {
  try {
    const full = await invoke<SynConversationFull>('syn_get_conversation', { vaultPath: props.vaultPath, conversationId: id });
    activeMessages.value = full.messages;

    // Use the conversation's model if set
    if (full.meta.model) {
      selectedModel.value = full.meta.model;
    }
  } catch (e) {
    logger.error('[Syn] Failed to load conversation', e);
    activeMessages.value = [];
  }
};

// Select conversation
const selectConversation = async (id: string) => {
  if (isStreaming.value) return; // Don't switch during streaming
  activeConversationId.value = id;
  clearStreaming();
  await loadConversation(id);
};

// Create new conversation
const createConversation = async () => {
  try {
    const conv = await invoke<SynConversation>('syn_create_conversation', {
      vaultPath: props.vaultPath,
      title: t('syn.new_conversation_title'),
    });
    conversations.value.unshift(conv);
    activeConversationId.value = conv.id;
    activeMessages.value = [];
    clearStreaming();
  } catch (e) {
    logger.error('[Syn] Failed to create conversation', e);
  }
};

// Delete conversation
const showDeleteConfirm = ref(false);
const conversationToDelete = ref<string | null>(null);

const deleteConversation = (id: string) => {
  conversationToDelete.value = id;
  showDeleteConfirm.value = true;
};

const executeDeleteConversation = async () => {
  if (!conversationToDelete.value) return;
  const id = conversationToDelete.value;
  try {
    await invoke('syn_delete_conversation', { vaultPath: props.vaultPath, conversationId: id });
    conversations.value = conversations.value.filter(c => c.id !== id);
    if (activeConversationId.value === id) {
      activeConversationId.value = null;
      activeMessages.value = [];
    }
  } catch (e) {
    logger.error('[Syn] Failed to delete conversation', e);
  } finally {
    showDeleteConfirm.value = false;
    conversationToDelete.value = null;
  }
};

const cancelDeleteConversation = () => {
  showDeleteConfirm.value = false;
  conversationToDelete.value = null;
};

// Rename conversation
const renameConversation = async (id: string, title: string) => {
  try {
    await invoke('syn_rename_conversation', { vaultPath: props.vaultPath, conversationId: id, title });
    const conv = conversations.value.find(c => c.id === id);
    if (conv) conv.title = title;
  } catch (e) {
    logger.error('[Syn] Failed to rename conversation', e);
  }
};

// Send message
const handleSendMessage = async (text: string, images?: string[]) => {
  // Create conversation if none active
  if (!activeConversationId.value) {
    await createConversation();
  }

  if (!activeConversationId.value) return;

  // Double-check: deduplicate consecutive identical lines (IME artifact safety net)
  const cleanText = text
    .replace(/[\u200B-\u200D\uFEFF]/g, '')
    .split('\n')
    .map(l => l.trim().replace(/\s+/g, ' '))
    .filter(l => l.length > 0)
    .filter((line, i, arr) => i === 0 || line.normalize('NFC').toLowerCase() !== arr[i - 1].normalize('NFC').toLowerCase())
    .join('\n');

  if (!cleanText && !images?.length) return;

  // Add user message immediately
  const userMessage: SynMessage = {
    id: crypto.randomUUID(),
    role: 'user',
    content: cleanText,
    timestamp: new Date().toISOString(),
    images: images,
  };
  activeMessages.value.push(userMessage);

  // Send to backend
  const response = await sendMessage(
    props.vaultPath,
    activeConversationId.value,
    cleanText,
    selectedModel.value || undefined,
    undefined,
    images
  );

  if (response) {
    // Add assistant response
    activeMessages.value.push(response);
    clearStreaming();

    // Update conversation in sidebar
    const conv = conversations.value.find(c => c.id === activeConversationId.value);
    if (conv) {
      conv.message_count += 2;
      conv.updated_at = new Date().toISOString();
      // Auto-title from first message
      if (conv.message_count <= 2) {
        conv.title = cleanText.slice(0, 50) + (cleanText.length > 50 ? '...' : '');
      }
    }
  }
};

// Quick action from welcome screen
const handleQuickAction = async (prompt: string) => {
  await createConversation();
  await handleSendMessage(prompt);
};

// Handle model pull from welcome screen
const handlePullModel = async (name: string) => {
  await pullModel(name);
};

// Refresh method exposed to parent
const refresh = async () => {
  await checkStatus(props.vaultPath);
  if (status.value.connected) {
    await fetchModels();
  }
  await loadConversations();
};

// Handle regenerate
const handleRegenerate = async (messageId: string) => {
  if (!activeConversationId.value) return;
  const msgIndex = activeMessages.value.findIndex(m => m.id === messageId);
  if (msgIndex <= 0) return;
  const userMsg = activeMessages.value[msgIndex - 1];
  if (userMsg.role !== 'user') return;

  // Remove the assistant message from local state
  activeMessages.value.splice(msgIndex, 1);

  // Re-send the user's message, preserving original images
  await handleSendMessage(userMsg.content, userMsg.images);
};

// Handle pin
const handlePinConversation = async (conversationId: string, pinned: boolean) => {
  try {
    await invoke('syn_pin_conversation', { vaultPath: props.vaultPath, conversationId, pinned });
    await loadConversations();
  } catch (e) {
    logger.error('[Syn] Failed to pin conversation', e);
  }
};

// Handle export
const handleExportConversation = async () => {
  if (!activeConversationId.value) return;
  try {
    const markdown = await invoke<string>('syn_export_conversation', {
      vaultPath: props.vaultPath,
      conversationId: activeConversationId.value,
    });
    await navigator.clipboard.writeText(markdown);
  } catch (e) {
    logger.error('[Syn] Failed to export conversation', e);
  }
};

// Handle settings saved
const handleSettingsSaved = async () => {
  showSettings.value = false;
  // Reload status in case Ollama URL changed
  await checkStatus(props.vaultPath);
  if (status.value.connected) {
    await fetchModels();
  }
};

// Handle new conversation helper
const handleNewConversation = () => {
  createConversation();
};

// Watch for model changes
watch(selectedModel, (newModel) => {
  if (newModel) {
    logger.info(`[Syn] Selected model: ${newModel}`);
  }
});

// Lifecycle
onMounted(async () => {
  loading.value = true;
  try {
    await checkStatus(props.vaultPath);
    if (status.value.connected) {
      await fetchModels();
    }
    await loadConversations();
  } catch (e) {
    logger.error('[Syn] Failed to initialize', e);
  } finally {
    loading.value = false;
  }

  // Keyboard shortcuts
  const handleKeydown = (e: KeyboardEvent) => {
    if ((e.metaKey || e.ctrlKey) && e.key === 'n') {
      e.preventDefault();
      handleNewConversation();
    }
    if ((e.metaKey || e.ctrlKey) && e.shiftKey && e.key === 's') {
      e.preventDefault();
      showSidebar.value = !showSidebar.value;
    }
    if (e.key === 'Escape' && isStreaming.value) {
      stopGeneration();
    }
  };
  window.addEventListener('keydown', handleKeydown);
  onUnmounted(() => window.removeEventListener('keydown', handleKeydown));
});

defineExpose({ refresh });
</script>

<template>
  <div class="flex-1 w-full h-full flex flex-col bg-gray-50 dark:bg-[#0f1115] text-text dark:text-text-dark relative overflow-hidden">
    <!-- Header -->
    <div
      class="h-14 border-b border-border dark:border-border-dark flex items-center justify-between px-4 flex-shrink-0 bg-surface dark:bg-surface-dark z-10 shadow-sm"
      data-tauri-drag-region
    >
      <!-- Left: Nav + Title -->
      <div class="flex items-center gap-3">
        <NavButtons />

        <!-- Sidebar toggle -->
        <button
          @click="showSidebar = !showSidebar"
          class="p-1.5 rounded-lg hover:bg-gray-100 dark:hover:bg-white/5 text-gray-500 dark:text-gray-400 transition-colors cursor-pointer"
          :title="showSidebar ? $t('syn.hide_sidebar') : $t('syn.show_sidebar')"
        >
          <PanelLeftClose v-if="showSidebar" class="w-4 h-4" />
          <PanelLeft v-else class="w-4 h-4" />
        </button>

        <div class="flex items-center gap-2">
          <div class="w-7 h-7 rounded-lg overflow-hidden shadow-sm ring-1 ring-violet-500/30">
            <img :src="synAvatar" alt="Syn" class="w-full h-full object-cover" />
          </div>
          <span class="text-lg font-semibold tracking-tight">Syn</span>
        </div>
      </div>

      <!-- Right: Model selector + actions -->
      <div class="flex items-center gap-2">
        <!-- Export button -->
        <button
          v-if="activeConversationId"
          @click="handleExportConversation"
          class="p-1.5 rounded-lg hover:bg-gray-100 dark:hover:bg-white/5 text-gray-500 dark:text-gray-400 transition-colors cursor-pointer"
          :title="$t('syn.export')"
        >
          <Download class="w-4 h-4" />
        </button>

        <!-- Settings button -->
        <button
          @click="showSettings = !showSettings"
          class="p-1.5 rounded-lg hover:bg-gray-100 dark:hover:bg-white/5 text-gray-500 dark:text-gray-400 transition-colors cursor-pointer"
          :title="$t('syn.settings')"
        >
          <Settings class="w-4 h-4" />
        </button>

        <ModelSelector
          v-if="isReady"
          v-model="selectedModel"
          :models="models"
          :format-size="formatModelSize"
        />
      </div>
    </div>

    <!-- Loading state -->
    <div v-if="loading" class="flex-1 flex items-center justify-center">
      <div class="flex flex-col items-center gap-3">
        <Loader2 class="w-8 h-8 text-violet-500 animate-spin" />
        <span class="text-sm text-gray-400 dark:text-gray-500">{{ $t('syn.loading') }}</span>
      </div>
    </div>

    <!-- Main content -->
    <div v-else class="flex-1 flex min-h-0 overflow-hidden">
      <!-- Sidebar -->
      <Transition
        enter-active-class="transition-all duration-200 ease-out"
        enter-from-class="-translate-x-full opacity-0"
        enter-to-class="translate-x-0 opacity-100"
        leave-active-class="transition-all duration-150 ease-in"
        leave-from-class="translate-x-0 opacity-100"
        leave-to-class="-translate-x-full opacity-0"
      >
        <div
          v-if="showSidebar"
          class="w-[260px] flex-shrink-0 border-r border-border dark:border-border-dark overflow-hidden"
        >
          <ConversationList
            :conversations="conversations"
            :active-id="activeConversationId"
            @select="selectConversation"
            @create="createConversation"
            @delete="deleteConversation"
            @rename="renameConversation"
            @pin="handlePinConversation"
          />
        </div>
      </Transition>

      <!-- Main area -->
      <div class="flex-1 flex flex-col min-w-0">
        <!-- Chat Panel or Welcome Screen -->
        <ChatPanel
          v-if="activeConversationId && isReady"
          :messages="activeMessages"
          :streaming-content="streamingContent"
          :is-streaming="isStreaming"
          :tool-calls="toolCalls"
          :vault-path="vaultPath"
          @send="handleSendMessage"
          @stop="stopGeneration"
          @open-source="handleOpenSource"
          @regenerate="handleRegenerate"
        />
        <WelcomeScreen
          v-else
          :status="status"
          :models="models"
          :pulling-model="pullingModel"
          :pull-progress="pullProgress"
          @new-chat="createConversation"
          @quick-action="handleQuickAction"
          @pull-model="handlePullModel"
        />
      </div>
    </div>

    <!-- Status bar -->
    <div class="h-7 border-t border-border dark:border-border-dark flex items-center px-4 bg-surface dark:bg-surface-dark flex-shrink-0 select-none">
      <div class="flex items-center gap-2 text-[11px] text-gray-500 dark:text-gray-400">
        <div class="flex items-center gap-1.5">
          <div class="w-1.5 h-1.5 rounded-full" :class="statusColor" />
          <span>{{ statusText }}</span>
        </div>
        <span v-if="status.connected && activeModelName" class="text-gray-400 dark:text-gray-500">
          · {{ activeModelName }}
        </span>
        <span v-if="status.version" class="text-gray-400 dark:text-gray-500">
          · Ollama {{ status.version }}
        </span>
      </div>
    </div>

    <!-- Settings Panel -->
    <SynSettings
      v-if="showSettings"
      :vault-path="props.vaultPath"
      :models="models"
      @close="showSettings = false"
      @saved="handleSettingsSaved"
    />

    <!-- Delete Confirmation Modal -->
    <ConfirmModal
      :show="showDeleteConfirm"
      :title="$t('syn.delete_conversation_title')"
      :message="$t('syn.delete_confirm')"
      :confirm-text="$t('syn.delete')"
      :cancel-text="$t('syn.cancel')"
      :is-destructive="true"
      @confirm="executeDeleteConversation"
      @cancel="cancelDeleteConversation"
    />
  </div>
</template>
