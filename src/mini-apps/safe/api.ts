/**
 * Safe's commands, typed.
 *
 * The shapes mirror `src-tauri/src/safe/item.rs` and `session.rs`. What is
 * missing from them is deliberate: an `ItemView` never carries the value of a
 * concealed field, only its label and a rough length. The value comes from
 * `reveal`, one field at a time, when the user presses the eye — or never
 * reaches this side at all, when they press copy.
 */
import { invoke } from '@tauri-apps/api/core';

export type ItemKind = 'login' | 'api_key' | 'ssh_key' | 'card' | 'secure_note' | 'identity' | 'wifi' | 'other';
export type FieldKind =
  | 'text' | 'username' | 'email' | 'url' | 'phone' | 'date' | 'multiline'
  | 'password' | 'concealed' | 'pin' | 'unknown';
export type AiLevel = 'hidden' | 'listed' | 'usable' | 'unknown';

export interface ItemSummary {
  id: string;
  kind: ItemKind;
  title: string;
  subtitle: string;
  hosts: string[];
  tags: string[];
  favorite: boolean;
  trashed: boolean;
  updated_at: number;
  ai_level: AiLevel;
}

export interface FieldView {
  id: string;
  label: string;
  kind: FieldKind;
  concealed: boolean;
  /** Only when not concealed. */
  value: string | null;
  /** Only when concealed: 8, 12, 16, 24 or 32 — never the real length. */
  length_bucket: number | null;
  empty: boolean;
}

export interface UrlRule {
  url: string;
  match: 'domain' | 'host' | 'exact' | 'never' | 'unknown';
}

export interface ItemView {
  id: string;
  kind: ItemKind;
  title: string;
  fields: FieldView[];
  urls: UrlRule[];
  tags: string[];
  favorite: boolean;
  notes: string;
  links: string[];
  ai_level: AiLevel;
  expires_at: number | null;
  created_at: number;
  updated_at: number;
  history_count: number;
  trashed_at: number | null;
}

/** A concealed value the editor never received goes back as `unchanged`. */
export type EditValue = { t: 'unchanged' } | { t: 'set'; v: string };

export interface FieldEdit {
  id: string | null;
  label: string;
  kind: FieldKind;
  value: EditValue;
}

export interface ItemEdit {
  kind: ItemKind;
  title: string;
  fields: FieldEdit[];
  urls: UrlRule[];
  tags: string[];
  favorite: boolean;
  notes: string;
  expires_at: number | null;
}

export type Filter =
  | { by: 'all' }
  | { by: 'favorites' }
  | { by: 'kind'; kind: ItemKind }
  | { by: 'tag'; tag: string }
  | { by: 'trash' };

export interface Overview {
  all: number;
  favorites: number;
  trash: number;
  kinds: [ItemKind, number][];
  tags: [string, number][];
  unreadable: { file: string; reason: string }[];
}

export interface Status {
  exists: boolean;
  unlocked: boolean;
  has_secret_key: boolean;
}

export interface Settings {
  auto_lock_secs: number;
  clipboard_clear_secs: number;
}

export type Recipe =
  | { mode: 'random'; length: number; lower: boolean; upper: boolean; digits: boolean; symbols: boolean; avoid_ambiguous: boolean }
  | { mode: 'passphrase'; words: number; separator: string; capitalise: boolean; digit: boolean }
  | { mode: 'pin'; length: number };

/** Emitted by Rust when the Safe locks itself after being left alone. */
export const LOCKED_EVENT = 'safe://locked';

/**
 * The code of a Safe refusal — `locked`, `wrong_password`, … — or null for
 * anything that is not one. Rust sends `{ code: "SAFE:<code>", message }`.
 */
export function safeCode(error: unknown): string | null {
  const code = (error as { code?: unknown } | null)?.code;
  return typeof code === 'string' && code.startsWith('SAFE:') ? code.slice(5) : null;
}

export function useSafeApi(vaultPath: () => string) {
  const v = () => ({ vaultPath: vaultPath() });
  return {
    status: () => invoke<Status>('safe_status', v()),
    create: (password: string) =>
      invoke<{ secret_key: string; stored_on_device: boolean }>('safe_create', { ...v(), password }),
    unlock: (password: string, secretKey?: string) =>
      invoke<void>('safe_unlock', { ...v(), password, secretKey: secretKey || null }),
    lock: () => invoke<void>('safe_lock'),
    changePassword: (current: string, next: string) => invoke<void>('safe_change_password', { ...v(), current, next }),
    secretKey: (password: string) => invoke<string>('safe_secret_key', { ...v(), password }),
    saveEmergencyKit: (path: string) => invoke<void>('safe_save_emergency_kit', { ...v(), path }),
    overview: () => invoke<Overview>('safe_overview', v()),
    list: (filter: Filter, query: string) => invoke<ItemSummary[]>('safe_list', { ...v(), filter, query }),
    get: (id: string) => invoke<ItemView>('safe_get', { ...v(), id }),
    reveal: (id: string, field: string) => invoke<string>('safe_reveal', { ...v(), id, field }),
    copy: (id: string, field: string) => invoke<{ clear_after_secs: number }>('safe_copy', { ...v(), id, field }),
    createItem: (item: ItemEdit) => invoke<ItemView>('safe_create_item', { ...v(), item }),
    updateItem: (id: string, item: ItemEdit) => invoke<ItemView>('safe_update_item', { ...v(), id, item }),
    setFavorite: (id: string, favorite: boolean) => invoke<void>('safe_set_favorite', { ...v(), id, favorite }),
    setTrashed: (id: string, trashed: boolean) => invoke<void>('safe_set_trashed', { ...v(), id, trashed }),
    purge: (id: string) => invoke<void>('safe_purge', { ...v(), id }),
    generate: (recipe?: Recipe) => invoke<{ value: string; bits: number }>('safe_generate', { recipe: recipe ?? null }),
    estimate: (password: string) => invoke<number>('safe_estimate', { password }),
    getSettings: () => invoke<Settings>('safe_get_settings', v()),
    setSettings: (settings: Settings) => invoke<Settings>('safe_set_settings', { ...v(), settings }),
  };
}

export type SafeApi = ReturnType<typeof useSafeApi>;
