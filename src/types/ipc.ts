/**
 * Shared TypeScript interfaces mirroring Rust backend structs.
 * These types ensure type-safe IPC communication between the
 * Vue frontend and the Tauri Rust backend.
 *
 * Keep in sync with: src-tauri/src/models/
 */

// ──────────────────────────────────────────────
// Notes
// ──────────────────────────────────────────────

export interface NodeMetadata {
  id: string;
  rel_path: string;
  node_type: string;
  title: string;
  content: string;
  properties: Record<string, unknown>;
  created_at: string;
  updated_at: string;
  timestamp: number;
}

/**
 * A node as a list needs it. Mirrors `models::node::NodeSummary`.
 *
 * Deliberately has no `content`: a list shows a title, a date, some properties
 * and the opening of the text, and sending the bodies too was the single
 * largest cost of opening one. Fetch the full node with `getNode` when the user
 * opens it.
 */
export interface NodeSummary {
  id: string;
  node_type: string;
  title: string;
  /** The opening of the body — not the body. */
  preview: string;
  properties: Record<string, unknown>;
  created_at: string;
  updated_at: string;
  timestamp: number;
}

/** What a vault scan managed to do. Mirrors `commands::nodes::ScanReport`. */
export interface ScanReport {
  /** Files parsed and written to the index on this run. */
  indexed: number;
  /** Nodes dropped because the file behind them is gone. */
  removed: number;
  /**
   * Files the scan could not fully index. The vault itself is intact — these
   * are simply missing from search and backlinks until the next successful
   * scan. The Rust log names each one.
   */
  failed: number;
}

// ──────────────────────────────────────────────
// Projects
// ──────────────────────────────────────────────

export interface ProjectMetadata {
  id: string;
  title: string;
  status: string;
  start_date: string;
  due_date: string;
  color: string;
  tags: string[];
  content: string;
  path: string;
  created_at: string;
  updated_at: string;
  custom_fields: Record<string, unknown>;
}

// ──────────────────────────────────────────────
// Tasks
// ──────────────────────────────────────────────

export interface ChecklistItem {
  content: string;
  completed: boolean;
}

export interface TaskFrontMatter {
  title: string;
  status: string;
  is_transferred: boolean;
  transferred_to: string;
  track_progress: boolean;
  project_id?: string;
  priority: string;
  start_date: string;
  due_date: string;
  comment: string;
  source_link: string;
  tags: string[];
  checklist: ChecklistItem[];
  completed_at: string;
  [key: string]: unknown; // custom_fields via serde(flatten)
}

export interface TaskMetadata {
  id: string;
  title: string;
  status: string;
  is_transferred: boolean;
  transferred_to: string;
  track_progress: boolean;
  project_id?: string;
  priority: string;
  start_date: string;
  due_date: string;
  comment: string;
  source_link: string;
  tags: string[];
  checklist: ChecklistItem[];
  content: string;
  path: string;
  created_at: string;
  updated_at: string;
  completed_at: string;
  custom_fields: Record<string, unknown>;
}

// ──────────────────────────────────────────────
// Events
// ──────────────────────────────────────────────

export interface EventFrontMatter {
  title: string;
  event_date: string;
  event_time: string;
  location: string;
  tags: string[];
}

export interface EventMetadata {
  id: string;
  title: string;
  event_date: string;
  event_time: string;
  location: string;
  tags: string[];
  content: string;
  path: string;
  created_at: string;
}

// ──────────────────────────────────────────────
// QuickCaps
// ──────────────────────────────────────────────

export interface QuickCapMetadata {
  id: string;
  date: string;
  content: string;
  path: string;
}

// ──────────────────────────────────────────────
// Files
// ──────────────────────────────────────────────

export interface FileItem {
  id: string;
  name: string;
  extension: string;
  size_mb: number;
  source_folder: string;
  date_modified: string;
  path: string;
  tags: string[];
}

export interface FileManagerSettings {
  tracked_sources: string[];
}

// ──────────────────────────────────────────────
// Nexus (unified search)
// ──────────────────────────────────────────────

export interface NexusItem {
  id: string;
  item_type: string;
  title: string;
  preview: string;
  tags: string[];
  date: string;
  path: string;
  content: string;
}

export interface TagStat {
  name: string;
  total_count: number;
  distribution: Record<string, number>;
}

export interface VaultStats {
  total_items: number;
  type_distribution: Record<string, number>;
  tags: TagStat[];
}

// ──────────────────────────────────────────────
// Whiteboards
// ──────────────────────────────────────────────

export interface WhiteboardMetadata {
  id: string;
  title: string;
  tags: string[];
  content: string;
  path: string;
  created_at: string;
  updated_at: string;
}

// ──────────────────────────────────────────────
// Google Drive Sync
// ──────────────────────────────────────────────

/// Mirrors `sync::core::types::SyncResult`. Every field is always present.
///
/// Asset counters are deliberately absent: nothing in the sync engine produces
/// them today, and declaring them made the UI read fields that never arrive.
/** A file kept aside because another device's version took its place. */
export interface SyncConflict {
  /** The contested location, which now holds the other device's version. */
  rel_path: string;
  /** Where ours was moved to. */
  kept_as: string;
}

export interface SyncResult {
  pulled: number;
  pulled_files: string[];
  pushed: number;
  deleted: number;
  errors: string[];
  /** Not failures. The sync worked; these files were preserved under new names. */
  conflicts: SyncConflict[];
  tx_bytes: number;
  rx_bytes: number;
}
