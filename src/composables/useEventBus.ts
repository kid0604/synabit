/**
 * useEventBus — Typed cross-app event bus for Synabit mini-apps
 *
 * Two layers:
 * 1. Tauri events (from Rust backend) — bridged automatically via initEventBus()
 * 2. Frontend events (cross-app) — emitted/subscribed purely in the frontend
 *
 * Usage:
 *   const bus = useEventBus();
 *   bus.on('vault:file-modified', ({ paths }) => { ... });
 *   bus.emit('task:completed', { id, title });
 */

import { onUnmounted, getCurrentInstance } from 'vue';
import { listen, type UnlistenFn } from '@tauri-apps/api/event';

import { logger } from '../utils/logger';

// ─── Event Type Definitions ──────────────────────────────────

/** Tauri backend events (bridged automatically by initEventBus) */
export interface TauriEventMap {
  'vault:file-modified': { paths: string[] };
  'vault:file-created-deleted': { paths: string[] };
  'vault:sync-completed': { result?: any };
  'vault:changed': void;
  'chat:new-message': void;
  'note:updated-external': { id: string; content: string };
  'e2ee:setup-required': void;
  'feed:refresh-completed': { sourceId?: string };
}

/** Frontend-only cross-app events */
export interface AppEventMap {
  // Generic node lifecycle
  'node:created': { nodeType: string; id: string; title: string };
  'node:updated': { nodeType: string; id: string; title: string; changes?: string[] };
  'node:deleted': { nodeType: string; id: string };

  // Task-specific
  'task:status-changed': { id: string; oldStatus: string; newStatus: string; title: string };
  'task:completed': { id: string; title: string; projectId?: string };

  // Calendar
  'event:created': { id: string; title: string; startAt: string };

  // Finance
  'transaction:created': { id: string; type: string; amount: number; category: string };

  // Feeds
  'feed:refreshed': { sourceId?: string };
  'feed:article-read': { articleId: string };

  // Navigation request (cross-app)
  'navigate:to-item': { app: string; itemId: string };
}

/** Combined event map */
export type EventMap = TauriEventMap & AppEventMap;

export type EventName = keyof EventMap;

// Handler type — void payloads get no argument
type Handler<T> = T extends void ? () => void : (payload: T) => void;

// ─── Singleton State ─────────────────────────────────────────

type AnyHandler = (...args: any[]) => void;
const subscribers = new Map<string, Set<AnyHandler>>();
const tauriUnlistenFns: UnlistenFn[] = [];
let initialized = false;

// ─── Internal Helpers ────────────────────────────────────────

function getOrCreateSet(event: string): Set<AnyHandler> {
  let set = subscribers.get(event);
  if (!set) {
    set = new Set();
    subscribers.set(event, set);
  }
  return set;
}

function dispatch(event: string, payload?: any): void {
  const set = subscribers.get(event);
  if (!set || set.size === 0) return;
  for (const handler of set) {
    try {
      handler(payload);
    } catch (err) {
      logger.error(`[EventBus] Error in handler for "${event}":`, err);
    }
  }
}

// ─── Tauri Event Name Mapping ────────────────────────────────

/**
 * Maps Tauri event names → bus event names.
 * Only events listed here will be automatically bridged.
 */
const TAURI_BRIDGE_MAP: Record<string, EventName> = {
  'vault-file-modified': 'vault:file-modified',
  'vault-file-created-deleted': 'vault:file-created-deleted',
  'vault-sync-completed': 'vault:sync-completed',
  'vault-changed': 'vault:changed',
  'new-chat-message': 'chat:new-message',
  'note-updated': 'note:updated-external',
  'e2ee-setup-required': 'e2ee:setup-required',
  'feed-refresh-completed': 'feed:refresh-completed',
};

// ─── Public API ──────────────────────────────────────────────

/**
 * Initialize the Event Bus — bridges Tauri events to the bus.
 * Call ONCE from App.vue onMounted, before mini-apps mount.
 */
export async function initEventBus(): Promise<void> {
  if (initialized) {
    logger.warn('[EventBus] Already initialized, skipping.');
    return;
  }

  for (const [tauriName, busName] of Object.entries(TAURI_BRIDGE_MAP)) {
    try {
      const unlisten = await listen(tauriName, (event) => {
        const payload = event.payload;
        dispatch(busName, payload);
      });
      tauriUnlistenFns.push(unlisten);
    } catch (err) {
      logger.error(`[EventBus] Failed to bridge Tauri event "${tauriName}":`, err);
    }
  }

  initialized = true;
  logger.info(`[EventBus] Initialized with ${tauriUnlistenFns.length} Tauri bridges.`);
}

/**
 * Destroy the Event Bus — unlistens all Tauri bridges and clears subscribers.
 * Call from App.vue onUnmounted.
 */
export function destroyEventBus(): void {
  for (const unlisten of tauriUnlistenFns) {
    unlisten();
  }
  tauriUnlistenFns.length = 0;
  subscribers.clear();
  initialized = false;
  logger.info('[EventBus] Destroyed.');
}

/**
 * useEventBus() — composable for subscribing and emitting events.
 *
 * When called inside a Vue component, subscriptions are auto-cleaned
 * on component unmount. When called outside (services, etc.),
 * use off() for manual cleanup.
 */
export function useEventBus() {
  const instance = getCurrentInstance();
  const localCleanups: Array<() => void> = [];

  // Auto-cleanup when the Vue component that called this unmounts
  if (instance) {
    onUnmounted(() => {
      for (const cleanup of localCleanups) {
        cleanup();
      }
      localCleanups.length = 0;
    });
  }

  /**
   * Subscribe to an event. Auto-cleaned on component unmount.
   */
  function on<K extends EventName>(event: K, handler: Handler<EventMap[K]>): void {
    const set = getOrCreateSet(event);
    set.add(handler as AnyHandler);

    const cleanup = () => {
      set.delete(handler as AnyHandler);
    };

    if (instance) {
      localCleanups.push(cleanup);
    }
  }

  /**
   * Manually unsubscribe a handler.
   */
  function off<K extends EventName>(event: K, handler: Handler<EventMap[K]>): void {
    const set = subscribers.get(event);
    if (set) {
      set.delete(handler as AnyHandler);
    }
  }

  /**
   * Subscribe to an event, but only fire once.
   */
  function once<K extends EventName>(event: K, handler: Handler<EventMap[K]>): void {
    const wrapper = ((payload: any) => {
      off(event, wrapper as any);
      (handler as AnyHandler)(payload);
    }) as Handler<EventMap[K]>;
    on(event, wrapper);
  }

  /**
   * Emit a frontend event (or a bridged Tauri event name).
   */
  function emit<K extends EventName>(
    event: K,
    ...args: EventMap[K] extends void ? [] : [payload: EventMap[K]]
  ): void {
    dispatch(event, args[0]);
  }

  return { on, off, once, emit };
}
