/**
 * useNodeService — Shared Node CRUD service for Synabit mini-apps
 *
 * Wraps all Tauri IPC calls for node operations (read, write, delete, rename)
 * and auto-emits Event Bus events on mutations.
 *
 * Benefits:
 * - No need to pass `vaultPath` — reads from Pinia store
 * - Auto-emits `node:created`, `node:updated`, `node:deleted` events
 * - Standardized error handling
 * - Single source of truth for all node IPC
 *
 * Usage:
 *   const ns = useNodeService();
 *   await ns.writeNode({ relPath, nodeType: 'note', title, properties, content });
 *   const notes = await ns.getNodes('note');
 */

import { invoke } from '@tauri-apps/api/core';
import { useAppStore } from '../stores/useAppStore';
import { useEventBus } from './useEventBus';

import { storeToRefs } from 'pinia';
import type { NodeSummary, EventSummary, EventsInRange } from '../types/ipc';
import { localTimeZone } from '../mini-apps/calendar/timezone';

// ─── Types ──────────────────────────────────────────────────

export type NodeType =
  | 'note' | 'task' | 'project' | 'event' | 'person' | 'interaction'
  | 'quickcap' | 'finance_month' | 'finance_config' | 'finance_debts'
  | 'pdf_highlight' | 'pdf_drawing' | 'file' | 'filter';

export interface WriteNodeParams {
  relPath: string;
  nodeType: NodeType;
  title: string;
  /**
   * The frontmatter keys this write is changing — a patch, not the whole file.
   *
   * Keys named here are set. Keys not named are left exactly as they are on
   * disk, which is what lets a screen that knows about four fields save without
   * deleting the `aliases` someone typed into the file by hand.
   *
   * To remove a key, name it with a value of `null`. Leaving it out means "I
   * have nothing to say about this one", which is not the same thing, and the
   * backend can only tell the two apart if you say which you meant.
   */
  properties: Record<string, unknown>;
  /**
   * The new body, or omitted to leave the body already on disk alone.
   *
   * Omit it for a property-only write — ticking a task off, dragging a card
   * between columns. Those callers hold whatever body was loaded with the
   * list, which after a sync or an edit in another window is no longer what is
   * on disk; sending it back reverts the file to the version the list happened
   * to have. Omitting says "I have nothing to say about the body", and the
   * backend keeps what is there. Same distinction as `properties`, one level up.
   */
  content?: string;
  /** Skip event bus emission (e.g., internal migrations, batch ops) */
  silent?: boolean;
  /** Override event type. Auto-defaults to 'updated'. */
  eventType?: 'created' | 'updated';
}

export interface CreateNodeParams {
  directory: string;
  nodeType: NodeType;
  silent?: boolean;
}

export interface DeleteNodeParams {
  relPath: string;
  /** Skip event bus emission */
  silent?: boolean;
}

export interface RenameNodeParams {
  oldRelPath: string;
  newName: string;
}

// ─── Service ────────────────────────────────────────────────

export function useNodeService() {
  const appStore = useAppStore();
  const { vaultPath } = storeToRefs(appStore);
  const bus = useEventBus();

  // ─── Write (Create/Update) ──────────────────────────────

  /** Write (create or update) a node file */
  async function writeNode(params: WriteNodeParams): Promise<void> {
    // A caller reading the path off a field the backend does not send hands us
    // `undefined`, and the IPC layer reports only "missing required key
    // relPath" — nothing about which node, or which caller. Say it here, where
    // the title is still in hand.
    if (!params.relPath) {
      throw new Error(
        `writeNode called without a relPath (title: "${params.title}", type: ${params.nodeType}). ` +
        `A node's path is its \`id\` — the backend sends no \`rel_path\` field.`
      );
    }

    const args: Record<string, unknown> = {
      vaultPath: vaultPath.value,
      relPath: params.relPath,
      nodeType: params.nodeType,
      title: params.title,
      properties: params.properties,
      // `undefined` reaches the backend as `None`, which is the "keep the body"
      // case. Passing an empty string would empty the file instead.
      content: params.content,
    };
    await invoke('write_node_file', args);

    if (!params.silent) {
      const eventType = params.eventType || 'updated';
      bus.emit(eventType === 'created' ? 'node:created' : 'node:updated', {
        nodeType: params.nodeType,
        id: params.relPath,
        title: params.title,
      });
    }
  }

  // ─── Create ─────────────────────────────────────────────

  /** Create a new empty node file. Returns the new relPath. */
  async function createNode(params: CreateNodeParams): Promise<string> {
    const newPath = await invoke<string>('create_node_file', {
      vaultPath: vaultPath.value,
      directory: params.directory,
      nodeType: params.nodeType,
    });

    if (!params.silent) {
      bus.emit('node:created', {
        nodeType: params.nodeType,
        id: newPath,
        title: 'Untitled',
      });
    }

    return newPath;
  }

  // ─── Delete ─────────────────────────────────────────────

  /**
   * Move a node into the vault's `.trash/`, and report where it landed.
   *
   * This is what a user-facing delete should call. `deleteNode` unlinks the
   * file, which is the right primitive and the wrong default: a note is
   * usually the only copy of something, and the gesture that loses it is a
   * mis-aimed click on a small icon in a context menu.
   *
   * The move is a rename within one filesystem, so it is atomic — the note is
   * never in neither place — and the vault scanner already skips dot
   * directories, so the note leaves the index by itself.
   */
  async function trashNode(params: DeleteNodeParams): Promise<string> {
    const trashedTo = await invoke<string>('trash_node_file', {
      vaultPath: vaultPath.value,
      relPath: params.relPath,
    });

    if (!params.silent) {
      bus.emit('node:deleted', {
        nodeType: '',
        id: params.relPath,
      });
    }

    return trashedTo;
  }

  /**
   * Unlink a node file outright.
   *
   * Prefer `trashNode` for anything the user asked for. This exists for the
   * cases where the file is genuinely disposable — something the app itself
   * wrote and is now cleaning up — where filling the trash with it would only
   * be a slow disk leak.
   */
  async function deleteNode(params: DeleteNodeParams): Promise<void> {
    await invoke('delete_node_file', {
      vaultPath: vaultPath.value,
      relPath: params.relPath,
    });

    if (!params.silent) {
      bus.emit('node:deleted', {
        nodeType: '',
        id: params.relPath,
      });
    }
  }

  // ─── Rename ─────────────────────────────────────────────

  /** Rename a node file. Returns the new relPath. */
  async function renameNode(params: RenameNodeParams): Promise<string> {
    return await invoke<string>('rename_node_file', {
      vaultPath: vaultPath.value,
      oldRelPath: params.oldRelPath,
      newName: params.newName,
    });
  }

  // ─── Read ───────────────────────────────────────────────

  /** Fetch all nodes of a given type, bodies included. */
  async function getNodes(nodeType: string): Promise<any[]> {
    return await invoke<any[]>('get_nodes', { nodeType });
  }

  /**
   * Fetch all nodes of a given type without their bodies — what a list needs.
   *
   * Prefer this for anything that renders a list. `getNodes` sends every body
   * as well, which for a vault of ordinary notes is the bulk of the payload and
   * none of what a list displays. Call `getNode` for the one the user opens.
   */
  async function getNodeSummaries(nodeType: string): Promise<NodeSummary[]> {
    return await invoke<NodeSummary[]>('get_node_summaries', { nodeType });
  }

  /**
   * The events landing on each day between `from` and `to`, already expanded.
   *
   * Recurrence lives in Rust — `src-tauri/src/calendar/recurrence.rs` — and
   * this is how the calendar asks it. Do not re-derive which days a series
   * falls on here; that split is what let the grid and the reminder loop
   * disagree.
   */
  async function getEventsInRange(from: string, to: string): Promise<EventsInRange> {
    // The reader's zone by name, which only the front end knows. Rust expands
    // each series in its own zone and converts the result into this one.
    const viewerTz = localTimeZone();
    return await invoke<EventsInRange>('get_events_in_range', { from, to, viewerTz });
  }

  /**
   * Read a wall clock in one zone off a clock in another.
   *
   * The time grid works in the reader's zone, so a drag that lands an event
   * living in another one has to be turned back before it is stored. The
   * conversion has two genuinely awkward hours a year, so there is one
   * implementation of it and it is in Rust.
   */
  async function convertEventTime(stamps: string[], fromTz: string, toTz: string): Promise<string[]> {
    if (!fromTz || !toTz || fromTz === toTz) return stamps;
    return await invoke<string[]>('convert_event_time', { stamps, fromTz, toTz });
  }

  /** The tasks due on the days between `from` and `to`. */
  async function getTasksInRange(from: string, to: string): Promise<NodeSummary[]> {
    return await invoke<NodeSummary[]>('get_tasks_in_range', { from, to });
  }

  /** A recurring event and every node split off from it, wherever they fall. */
  async function getEventSeries(rootId: string): Promise<EventSummary[]> {
    return await invoke<EventSummary[]>('get_event_series', { rootId });
  }

  /** Fetch a single node by ID */
  async function getNode(id: string): Promise<any | null> {
    return await invoke<any>('get_node', { id });
  }

  /** Fetch nodes linked to a given target (backlinks) */
  async function getLinkedNodes(targetTitle: string, targetId: string): Promise<any[]> {
    return await invoke<any[]>('get_linked_nodes', { targetTitle, targetId });
  }

  // ─── Specialized ────────────────────────────────────────

  /**
   * Update a `file` node's properties.
   *
   * Only for nodes the database manages on its own — `file` nodes, which point
   * at files outside the vault and have no document of their own. Anything
   * backed by a file in the vault must go through `writeNode`, or the change
   * never reaches disk and never syncs; the backend rejects those outright
   * rather than accepting a write it cannot honour.
   */
  async function updateFileNodeProperties(id: string, properties: Record<string, unknown>): Promise<void> {
    await invoke('update_file_node_properties', { id, properties });
  }

  /** Scan specific node paths for indexing */
  async function scanSpecificNodes(paths: string[]): Promise<void> {
    await invoke('scan_specific_nodes', { vaultPath: vaultPath.value, paths });
  }

  return {
    writeNode,
    createNode,
    trashNode,
    deleteNode,
    renameNode,
    getNodes,
    getNodeSummaries,
    getEventsInRange,
    getTasksInRange,
    getEventSeries,
    convertEventTime,
    getNode,
    getLinkedNodes,
    updateFileNodeProperties,
    scanSpecificNodes,
    vaultPath,
  };
}
