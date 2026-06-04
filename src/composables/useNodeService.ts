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

// ─── Types ──────────────────────────────────────────────────

export type NodeType =
  | 'note' | 'task' | 'project' | 'event' | 'person'
  | 'quickcap' | 'finance_month' | 'finance_config' | 'finance_debts'
  | 'pdf_highlight' | 'pdf_drawing' | 'file';

export interface WriteNodeParams {
  relPath: string;
  nodeType: NodeType;
  title: string;
  properties: Record<string, unknown>;
  content: string;
  existingPath?: string;
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
    const args: Record<string, unknown> = {
      vaultPath: vaultPath.value,
      relPath: params.relPath,
      nodeType: params.nodeType,
      title: params.title,
      properties: params.properties,
      content: params.content,
    };
    if (params.existingPath) {
      args.existingPath = params.existingPath;
    }

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

  /** Delete a node file */
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

  /** Fetch all nodes of a given type */
  async function getNodes(nodeType: string): Promise<any[]> {
    return await invoke<any[]>('get_nodes', { nodeType });
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

  /** Update only the properties of a node (without rewriting content) */
  async function updateNodeProperties(id: string, properties: Record<string, unknown>): Promise<void> {
    await invoke('update_node_properties', { id, properties });
  }

  /** Scan specific node paths for indexing */
  async function scanSpecificNodes(paths: string[]): Promise<void> {
    await invoke('scan_specific_nodes', { vaultPath: vaultPath.value, paths });
  }

  return {
    writeNode,
    createNode,
    deleteNode,
    renameNode,
    getNodes,
    getNode,
    getLinkedNodes,
    updateNodeProperties,
    scanSpecificNodes,
    vaultPath,
  };
}
