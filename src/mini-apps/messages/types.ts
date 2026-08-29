export interface OllamaStatus {
  connected: boolean;
  version: string | null;
  url: string;
  /**
   * Whether this provider can pull and delete models on the user's behalf.
   *
   * True for Ollama, which hosts the weights. False behind an
   * OpenAI-compatible API, where the catalogue is the server's business — the
   * pull field has to be hidden rather than offered and left to fail.
   *
   * Optional because a status object built locally as a placeholder, before
   * the backend has answered, has nothing to say about it.
   */
  supports_model_management?: boolean;
}

export interface ModelInfo {
  name: string;
  model: string;
  size: number;
  digest: string;
  modified_at: string;
  details?: {
    format?: string;
    family?: string;
    parameter_size?: string;
    quantization_level?: string;
  };
}

export interface SynToolCallEvent {
  conversation_id: string;
  tool_name: string;
  tool_args: Record<string, unknown>;
  result_preview: string;
  iteration: number;
}

export interface SynMessage {
  id: string;
  role: 'user' | 'assistant' | 'system';
  content: string;
  model?: string;
  timestamp: string;
  tokens?: number;
  duration_ms?: number;
  sources?: SourceRef[];  // Source references from RAG
  tool_calls_log?: SynToolCallEvent[];
  images?: string[];  // base64 encoded
  notification?: any; // The raw chat notification
}

export interface SourceRef {
  id: string;
  title: string;
  node_type: string;
}

export interface SynConversation {
  id: string;
  title: string;
  model?: string;
  message_count: number;
  created_at: string;
  updated_at: string;
  pinned: boolean;
}

export interface SynConversationFull {
  meta: SynConversation;
  messages: SynMessage[];
}

export interface SynChatRequest {
  conversation_id: string;
  message: string;
  model?: string;
  temperature?: number;
  images?: string[];  // base64 encoded
}

export interface SynStreamToken {
  conversation_id: string;
  message_id: string;
  token: string;
  done: boolean;
}

export type { SynSettings } from './composables/useSynSettings';

