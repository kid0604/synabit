<script setup lang="ts">
import { ref, onMounted, onUnmounted, watch, computed } from 'vue';
import { invoke } from '@tauri-apps/api/core';
import { useI18n } from 'vue-i18n';
import { Loader2, Settings, Download, ChevronLeft, Zap } from 'lucide-vue-next';
import { logger } from '../../utils/logger';
import synAvatar from '../../assets/syn-avatar.jpg';

import ChatSidebar, { type ChatContact } from './components/ChatSidebar.vue';
import ChatPanel from './components/ChatPanel.vue';
import ModelSelector from './components/ModelSelector.vue';
import SynSettings from './components/SynSettings.vue';

import { useSynChat } from './composables/useSynChat';
import { useSynModels } from './composables/useSynModels';
import type { SynConversation, SynConversationFull, SynMessage } from './types';

const props = defineProps<{
  vaultPath: string;
}>();

const emit = defineEmits(['open-node']);
const { t } = useI18n();

const handleOpenSource = (source: { id: string; title: string; node_type: string }) => {
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

const handleNotificationAction = (notification: any) => {
  const targetId = notification.content?.metadata?.target_id;
  const targetType = notification.content?.metadata?.target_type || 'note';
  if (targetId) {
    emit('open-node', targetId, targetType);
  }
};

// Composables
const {
  streamingContent,
  isStreaming,
  toolCalls,
  error: chatError,
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
  pullError,
  isPolling,
  checkStatus,
  fetchModels,
  pullModel,
  formatModelSize,
  startPolling,
  stopPolling,
  startHealthCheck,
  stopHealthCheck,
  cleanup: cleanupModels,
} = useSynModels();

// State
const activeChatId = ref<string | null>(null);
const activeMessages = ref<SynMessage[]>([]);
const notifications = ref<any[]>([]);
const showSettings = ref(false);
const loading = ref(true);

const isMobile = ref(window.innerWidth < 768);

// The hardcoded Syn conversation ID for the backend
const SYN_BACKEND_ID = ref<string | null>(null);

const activeModelName = computed(() => {
  if (!selectedModel.value) return '';
  const model = models.value.find(m => m.name === selectedModel.value);
  return model ? model.name : selectedModel.value;
});

// Contacts list
const contacts = computed<ChatContact[]>(() => {
  // Generate preview text from the last message in mixedMessages
  let lastMessagePreview = '';
  let timestamp = '';
  if (mixedMessages.value.length > 0) {
     const last = mixedMessages.value[mixedMessages.value.length - 1];
     timestamp = last.timestamp;
     if (last.role === 'system' && last.notification) {
         lastMessagePreview = last.notification.content?.title || 'System Notification';
     } else {
         lastMessagePreview = last.role === 'user' ? `You: ${last.content}` : last.content;
     }
  }

  // Count unread notifications
  const unreadCount = notifications.value.filter(n => !n.read_receipt).length;

  return [
    {
      id: 'syn-main',
      name: 'Syn',
      avatar: synAvatar,
      type: 'ai',
      lastMessagePreview,
      timestamp,
      unreadCount,
      isOnline: status.value.connected,
    }
  ];
});

// Load a specific conversation from backend
const loadConversation = async (id: string) => {
  try {
    const full = await invoke<SynConversationFull>('syn_get_conversation', { vaultPath: props.vaultPath, conversationId: id });
    activeMessages.value = full.messages;
    if (full.meta.model) {
      selectedModel.value = full.meta.model;
    }
  } catch (e) {
    logger.error('[Syn] Failed to load conversation', e);
    activeMessages.value = [];
  }
};

const fetchNotifications = async () => {
  try {
    const history = await invoke<any[]>('get_chat_history', { vaultPath: props.vaultPath });
    notifications.value = history;
  } catch (e) {
    logger.error('[Messages] Failed to fetch notifications', e);
  }
};

const mixedMessages = computed(() => {
  const notifMsgs: SynMessage[] = notifications.value.map(n => ({
    id: n.id,
    role: 'system',
    content: '',
    timestamp: n.timestamp,
    notification: n,
  }));
  
  return [...activeMessages.value, ...notifMsgs].sort((a, b) => new Date(a.timestamp).getTime() - new Date(b.timestamp).getTime());
});

// Ensure a single conversation exists
const initConversation = async () => {
  try {
    const list = await invoke<SynConversation[]>('syn_list_conversations', { vaultPath: props.vaultPath });
    if (list.length > 0) {
      SYN_BACKEND_ID.value = list[0].id;
      await loadConversation(list[0].id);
    }
  } catch (e) {
    logger.error('[Syn] Failed to load conversations', e);
  }
};

const createConversation = async () => {
  try {
    const conv = await invoke<SynConversation>('syn_create_conversation', {
      vaultPath: props.vaultPath,
      title: t('syn.new_conversation_title'),
    });
    SYN_BACKEND_ID.value = conv.id;
    activeMessages.value = [];
    clearStreaming();
  } catch (e) {
    logger.error('[Syn] Failed to create conversation', e);
  }
};

// Send message
const handleSendMessage = async (text: string, images?: string[]) => {
  if (!SYN_BACKEND_ID.value) {
    await createConversation();
  }
  if (!SYN_BACKEND_ID.value) return;

  const cleanText = text
    .replace(/[\u200B-\u200D\uFEFF]/g, '')
    .split('\n')
    .map(l => l.trim().replace(/\s+/g, ' '))
    .filter(l => l.length > 0)
    .filter((line, i, arr) => i === 0 || line.normalize('NFC').toLowerCase() !== arr[i - 1].normalize('NFC').toLowerCase())
    .join('\n');

  if (!cleanText && !images?.length) return;

  const userMessage: SynMessage = {
    id: crypto.randomUUID(),
    role: 'user',
    content: cleanText,
    timestamp: new Date().toISOString(),
    images: images,
  };
  activeMessages.value.push(userMessage);

  const response = await sendMessage(
    props.vaultPath,
    SYN_BACKEND_ID.value,
    cleanText,
    selectedModel.value || undefined,
    undefined,
    images
  );

  if (response) {
    activeMessages.value.push(response);
    clearStreaming();
  }
};

const handlePullModel = async (name: string) => {
  await pullModel(name, props.vaultPath);
};

const refresh = async () => {
  await checkStatus(props.vaultPath);
  if (status.value.connected) {
    await fetchModels(props.vaultPath);
  }
  await initConversation();
  await fetchNotifications();
};

const handleRegenerate = async (messageId: string) => {
  if (!SYN_BACKEND_ID.value) return;
  const msgIndex = activeMessages.value.findIndex(m => m.id === messageId);
  if (msgIndex <= 0) return;
  const userMsg = activeMessages.value[msgIndex - 1];
  if (userMsg.role !== 'user') return;

  activeMessages.value.splice(msgIndex, 1);
  await handleSendMessage(userMsg.content, userMsg.images);
};

const handleExportConversation = async () => {
  if (!SYN_BACKEND_ID.value) return;
  try {
    const markdown = await invoke<string>('syn_export_conversation', {
      vaultPath: props.vaultPath,
      conversationId: SYN_BACKEND_ID.value,
    });
    await navigator.clipboard.writeText(markdown);
  } catch (e) {
    logger.error('[Syn] Failed to export conversation', e);
  }
};

const handleSettingsSaved = async () => {
  showSettings.value = false;
  await checkStatus(props.vaultPath);
  if (status.value.connected) {
    await fetchModels(props.vaultPath);
    startHealthCheck(props.vaultPath);
  } else {
    startPolling(props.vaultPath);
  }
};

const handleSelectChat = (id: string) => {
  activeChatId.value = id;
};

watch(() => status.value.connected, (connected, wasConnected) => {
  if (connected && !wasConnected) {
    stopPolling();
    startHealthCheck(props.vaultPath);
  } else if (!connected && wasConnected) {
    stopHealthCheck();
    startPolling(props.vaultPath);
  }
});

onMounted(async () => {
  loading.value = true;
  try {
    await checkStatus(props.vaultPath);
    if (status.value.connected) {
      await fetchModels(props.vaultPath);
      startHealthCheck(props.vaultPath);
    } else {
      startPolling(props.vaultPath);
    }
    await initConversation();
    await fetchNotifications();
    
    // Auto select Syn on desktop
    if (!isMobile.value) {
       activeChatId.value = 'syn-main';
    }
  } catch (e) {
    logger.error('[Syn] Failed to initialize', e);
  } finally {
    loading.value = false;
  }

  const handleResize = () => {
      isMobile.value = window.innerWidth < 768;
  };
  window.addEventListener('resize', handleResize);

  const handleKeydown = (e: KeyboardEvent) => {
    if (e.key === 'Escape' && isStreaming.value) {
      stopGeneration();
    }
  };
  window.addEventListener('keydown', handleKeydown);
  
  onUnmounted(() => {
    window.removeEventListener('resize', handleResize);
    window.removeEventListener('keydown', handleKeydown);
    cleanupModels();
  });
});

defineExpose({ refresh, fetchNotifications });
</script>

<template>
  <div class="flex-1 w-full h-full flex bg-gray-50 dark:bg-[#0f1115] text-text dark:text-text-dark relative overflow-hidden">
    
    <!-- Sidebar -->
    <div 
        class="flex-shrink-0 h-full border-r border-border dark:border-border-dark transition-all duration-300 z-20"
        :class="[isMobile ? (activeChatId ? 'hidden' : 'w-full') : 'w-[320px] max-w-[35%]']"
    >
        <ChatSidebar 
            :contacts="contacts"
            :active-id="activeChatId"
            @select="handleSelectChat"
        />
    </div>

    <!-- Main Chat Area -->
    <div 
        class="flex-1 flex flex-col min-w-0 h-full bg-white dark:bg-[#15161a] transition-all duration-300"
        :class="[isMobile ? (activeChatId ? 'w-full block' : 'hidden') : 'block']"
    >
        <!-- Header for Chat -->
        <div class="h-14 border-b border-border dark:border-border-dark flex items-center justify-between px-4 flex-shrink-0 bg-surface dark:bg-surface-dark shadow-sm">
            <!-- Left: Back button (mobile) + Contact info -->
            <div class="flex items-center gap-3">
                <button v-if="isMobile" @click="activeChatId = null" class="p-1.5 -ml-2 rounded-lg hover:bg-gray-100 dark:hover:bg-white/5 text-gray-500 cursor-pointer" aria-label="Active Chat Id = null">
                    <ChevronLeft class="w-5 h-5" />
                </button>
                
                <template v-if="activeChatId === 'syn-main'">
                    <div class="w-8 h-8 rounded-xl overflow-hidden shadow-sm ring-1 ring-violet-500/30 flex-shrink-0 relative">
                        <img :src="synAvatar" alt="Syn" class="w-full h-full object-cover" />
                        <div v-if="status.connected" class="absolute -bottom-0.5 -right-0.5 w-2.5 h-2.5 bg-green-500 rounded-full border-2 border-surface dark:border-surface-dark"></div>
                    </div>
                    <div class="flex flex-col justify-center min-w-0">
                        <span class="text-sm font-semibold tracking-tight text-gray-900 dark:text-white leading-tight">Syn</span>
                        <div class="flex items-center gap-1.5 text-[11px] text-gray-500">
                           <span v-if="status.connected" class="text-green-500 font-medium">Online</span>
                           <span v-else class="text-gray-400">Offline</span>
                           
                           <template v-if="status.connected">
                              <span class="text-gray-300 dark:text-gray-600">·</span>
                              <span class="truncate">{{ activeModelName }}</span>
                           </template>
                        </div>
                    </div>
                </template>
                <template v-else-if="!activeChatId">
                    <span class="font-medium text-gray-400">No chat selected</span>
                </template>
            </div>

            <!-- Right: Actions -->
            <div class="flex items-center gap-1.5" v-if="activeChatId === 'syn-main'">
                
                <ModelSelector
                  v-if="status.connected && models.length > 0"
                  v-model="selectedModel"
                  :models="models"
                  :format-size="formatModelSize"
                  :pulling-model="pullingModel"
                  :pull-progress="pullProgress"
                  @pull-model="handlePullModel"
                />

                <button
                  v-if="mixedMessages.length > 0"
                  @click="handleExportConversation"
                  class="p-1.5 rounded-lg hover:bg-gray-100 dark:hover:bg-white/5 text-gray-500 dark:text-gray-400 transition-colors cursor-pointer"
                  :title="$t('syn.export')"
                >
                  <Download class="w-4 h-4" />
                </button>

                <button
                  @click="showSettings = !showSettings"
                  class="p-1.5 rounded-lg hover:bg-gray-100 dark:hover:bg-white/5 text-gray-500 dark:text-gray-400 transition-colors cursor-pointer"
                  title="Syn AI Settings"
                >
                  <Settings class="w-4 h-4" />
                </button>
            </div>
        </div>

        <!-- Chat Panel -->
        <div class="flex-1 flex min-h-0 overflow-hidden relative">
            
            <div v-if="loading" class="absolute inset-0 flex items-center justify-center bg-white/50 dark:bg-[#15161a]/50 z-10 backdrop-blur-sm">
                <Loader2 class="w-8 h-8 text-violet-500 animate-spin" />
            </div>

            <template v-if="activeChatId === 'syn-main'">
                <ChatPanel
                  :messages="mixedMessages"
                  :streaming-content="streamingContent"
                  :is-streaming="isStreaming"
                  :tool-calls="toolCalls"
                  :vault-path="vaultPath"
                  :connection-lost="!status.connected"
                  :chat-error="chatError"
                  @send="handleSendMessage"
                  @stop="stopGeneration"
                  @open-source="handleOpenSource"
                  @regenerate="handleRegenerate"
                  @notification-action="handleNotificationAction"
                />
            </template>
            <template v-else>
                <div class="flex-1 flex flex-col items-center justify-center text-gray-400 dark:text-gray-500">
                    <div class="w-16 h-16 rounded-3xl bg-gray-100 dark:bg-gray-800 flex items-center justify-center mb-4">
                        <Zap class="w-8 h-8 text-gray-300 dark:text-gray-600" />
                    </div>
                    <p class="text-sm">Select a chat to start messaging</p>
                </div>
            </template>
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
  </div>
</template>
