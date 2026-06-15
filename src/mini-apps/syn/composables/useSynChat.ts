import { ref, onUnmounted } from 'vue';
import { invoke } from '@tauri-apps/api/core';
import { listen, type UnlistenFn } from '@tauri-apps/api/event';
import { logger } from '../../../utils/logger';
import type { SynStreamToken, SynMessage, SynToolCallEvent } from '../types';

export function useSynChat() {
  const streamingContent = ref('');
  const streamingMessageId = ref<string | null>(null);
  const isStreaming = ref(false);
  const toolCalls = ref<SynToolCallEvent[]>([]);
  const activeConversationId = ref<string | null>(null);
  const error = ref<string | null>(null);

  let unlisten: UnlistenFn | null = null;
  let unlistenTools: UnlistenFn | null = null;

  const setupListener = async (conversationId: string) => {
    // Clean up previous listeners if any
    if (unlisten) {
      unlisten();
      unlisten = null;
    }
    if (unlistenTools) {
      unlistenTools();
      unlistenTools = null;
    }

    try {
      activeConversationId.value = conversationId;
      unlisten = await listen<SynStreamToken>('syn-stream-token', (event) => {
        const token = event.payload;

        // Only process tokens for the active conversation
        if (token.conversation_id !== conversationId) return;

        if (token.done) {
          // Stream completed
          isStreaming.value = false;
          streamingMessageId.value = null;
          // Don't clear streamingContent here — the parent will handle it
          // after it picks up the final assembled message
        } else {
          streamingMessageId.value = token.message_id;
          streamingContent.value += token.token;
        }
      });

      unlistenTools = await listen<SynToolCallEvent>('syn-tool-call', (event) => {
        if (event.payload.conversation_id !== conversationId) return;
        toolCalls.value.push(event.payload);
      });
    } catch (e) {
      logger.error('[Syn] Failed to setup stream listener', e);
    }
  };

  const sendMessage = async (
    vaultPath: string,
    conversationId: string,
    message: string,
    model?: string,
    temperature?: number,
    images?: string[]
  ): Promise<SynMessage | null> => {
    error.value = null;
    isStreaming.value = true;
    streamingContent.value = '';
    toolCalls.value = [];
    streamingMessageId.value = null;

    // Setup listener before sending
    await setupListener(conversationId);

    try {
      const response = await invoke<SynMessage>('syn_send_message', {
        vaultPath,
        request: {
          conversation_id: conversationId,
          message,
          model: model || undefined,
          temperature: temperature || undefined,
          images: images?.length ? images : undefined,
        },
      });
      return response;
    } catch (e: any) {
      logger.error('[Syn] Failed to send message', e);
      error.value = e?.message || String(e);
      isStreaming.value = false;
      streamingContent.value = '';
      streamingMessageId.value = null;
      return null;
    }
  };

  const stopGeneration = async () => {
    try {
      await invoke('syn_stop_generation', {
        conversationId: activeConversationId.value || undefined,
      });
    } catch (e) {
      logger.error('[Syn] Failed to stop generation', e);
    } finally {
      isStreaming.value = false;
      streamingContent.value = '';
      streamingMessageId.value = null;
      activeConversationId.value = null;
      toolCalls.value = [];
    }
  };

  const clearStreaming = () => {
    streamingContent.value = '';
    streamingMessageId.value = null;
    activeConversationId.value = null;
    isStreaming.value = false;
    toolCalls.value = [];
  };

  onUnmounted(() => {
    if (unlisten) {
      unlisten();
      unlisten = null;
    }
    if (unlistenTools) {
      unlistenTools();
      unlistenTools = null;
    }
  });

  return {
    streamingContent,
    streamingMessageId,
    isStreaming,
    error,
    toolCalls,
    sendMessage,
    stopGeneration,
    clearStreaming,
  };
}
