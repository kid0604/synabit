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
  /** What the health check found; see `safe::health`. */
  health: HealthFlag[];
}

export type HealthFlag = 'breached' | 'reused' | 'weak' | 'expired' | 'expiring' | 'old';

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
  /** How its one-time codes are made — never the secret behind them. */
  totp: { algorithm: 'sha1' | 'sha256' | 'sha512'; digits: number; period: number } | null;
  ai_level: AiLevel;
  handle: string | null;
  ai_destinations: string[];
  health: HealthFlag[];
  expires_at: number | null;
  created_at: number;
  updated_at: number;
  history_count: number;
  trashed_at: number | null;
}

/** A concealed value the editor never received goes back as `unchanged`. */
export type EditValue = { t: 'unchanged' } | { t: 'set'; v: string };

/** The same for one-time codes: `set` carries an otpauth:// link or a base32 secret. */
export type TotpEdit = { t: 'unchanged' } | { t: 'remove' } | { t: 'set'; v: string };

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
  totp: TotpEdit;
  expires_at: number | null;
}

export type Filter =
  | { by: 'all' }
  | { by: 'favorites' }
  | { by: 'kind'; kind: ItemKind }
  | { by: 'tag'; tag: string }
  | { by: 'trash' }
  | { by: 'health'; flag?: HealthFlag | null };

export interface Overview {
  all: number;
  favorites: number;
  trash: number;
  kinds: [ItemKind, number][];
  tags: [string, number][];
  unreadable: { file: string; reason: string }[];
  unhealthy: number;
  health: [HealthFlag, number][];
  breach_checked_at: number | null;
  /** Items older on disk than this device has seen them, by title. */
  rolled_back: string[];
  keyset_rolled_back: boolean;
  /** The master password was changed on another device since this one last opened the Safe. */
  keyset_changed_elsewhere: boolean;
}

export interface Status {
  exists: boolean;
  unlocked: boolean;
  has_secret_key: boolean;
}

export interface Settings {
  auto_lock_secs: number;
  clipboard_clear_secs: number;
  breach_check: boolean;
  ssh_agent: boolean;
  ssh_confirm: boolean;
  cli: boolean;
}

export interface CliStatus {
  supported: boolean;
  running: boolean;
  socket: string | null;
}

export interface SshStatus {
  supported: boolean;
  running: boolean;
  socket: string | null;
  keys: { title: string; fingerprint: string | null; public: string | null; problem: string | null }[];
}

export type Recipe =
  | { mode: 'random'; length: number; lower: boolean; upper: boolean; digits: boolean; symbols: boolean; avoid_ambiguous: boolean }
  | { mode: 'passphrase'; words: number; separator: string; capitalise: boolean; digit: boolean }
  | { mode: 'pin'; length: number };

/** A secret in this device's keychain, beside the Safe. Never its value. */
export interface DeviceSecret {
  slot: string;
  kind: 'sync_key' | 'app_lock_pin' | 'provider' | 'telegram' | 'connector';
  name: string;
  forgettable: boolean;
}

/** What creating the Safe, or changing its Secret Key, gives back once. */
export interface Created {
  secret_key: string;
  stored_on_device: boolean;
}

/** Emitted by Rust whenever the Safe locks: by itself, by a click, on quitting. */
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
    create: (password: string) => invoke<Created>('safe_create', { ...v(), password }),
    /** `previous`: the user never changed the password; open the keyset another device replaced. */
    unlock: (password: string, secretKey?: string, previous = false) =>
      invoke<void>('safe_unlock', { ...v(), password, secretKey: secretKey || null, previous }),
    lock: () => invoke<void>('safe_lock'),
    changePassword: (current: string, next: string, secretKey?: string) =>
      invoke<void>('safe_change_password', { ...v(), current, next, secretKey: secretKey || null }),
    secretKey: (password: string) => invoke<string>('safe_secret_key', { ...v(), password }),
    /** The master password is needed except right after the words were shown. */
    saveEmergencyKit: (path: string, password?: string) =>
      invoke<void>('safe_save_emergency_kit', { ...v(), path, password: password || null }),
    rotateKey: (password: string, secretKey?: string) =>
      invoke<number>('safe_rotate_key', { ...v(), password, secretKey: secretKey || null }),
    changeSecretKey: (password: string) => invoke<Created>('safe_change_secret_key', { ...v(), password }),
    refresh: () => invoke<void>('safe_refresh', v()),
    overview: () => invoke<Overview>('safe_overview', v()),
    list: (filter: Filter, query: string) => invoke<ItemSummary[]>('safe_list', { ...v(), filter, query }),
    get: (id: string) => invoke<ItemView>('safe_get', { ...v(), id }),
    reveal: (id: string, field: string) => invoke<string>('safe_reveal', { ...v(), id, field }),
    copy: (id: string, field: string) => invoke<{ clear_after_secs: number }>('safe_copy', { ...v(), id, field }),
    totp: (id: string) => invoke<{ code: string; remaining: number; period: number }>('safe_totp', { ...v(), id }),
    copyTotp: (id: string) => invoke<{ clear_after_secs: number }>('safe_copy_totp', { ...v(), id }),
    createItem: (item: ItemEdit) => invoke<ItemView>('safe_create_item', { ...v(), item }),
    updateItem: (id: string, item: ItemEdit) => invoke<ItemView>('safe_update_item', { ...v(), id, item }),
    setFavorite: (id: string, favorite: boolean) => invoke<void>('safe_set_favorite', { ...v(), id, favorite }),
    setTrashed: (id: string, trashed: boolean) => invoke<void>('safe_set_trashed', { ...v(), id, trashed }),
    purge: (id: string) => invoke<void>('safe_purge', { ...v(), id }),
    generate: (recipe?: Recipe) => invoke<{ value: string; bits: number }>('safe_generate', { recipe: recipe ?? null }),
    estimate: (password: string) => invoke<{ score: number; bits: number }>('safe_estimate', { password }),
    checkBreaches: () => invoke<{ checked: number; breached: number; failed: number }>('safe_check_breaches', v()),
    importFile: (path: string, password?: string) =>
      invoke<{ format: string; imported: number; warnings: string[]; source_was_plaintext: boolean }>('safe_import', { ...v(), path, password: password || null }),
    exportSealed: (path: string, exportPassword: string) => invoke<number>('safe_export', { ...v(), path, exportPassword }),
    exportPlain: (path: string, password: string, secretKey?: string) =>
      invoke<number>('safe_export_plain', { ...v(), path, password, secretKey: secretKey || null }),
    setAi: (id: string, level: AiLevel, handle: string | null, destinations: string[]) =>
      invoke<ItemView>('safe_set_ai', { ...v(), id, level, handle, destinations }),
    destinations: () => invoke<{ key: string; label: string }[]>('safe_destinations', v()),
    requestSubmit: (requestId: string, title: string, handle: string, value: string, destinations: string[]) =>
      invoke<string>('safe_request_submit', { ...v(), requestId, title, handle, value, destinations }),
    deviceSecrets: () => invoke<DeviceSecret[]>('safe_device_secrets', v()),
    forgetDeviceSecret: (slot: string) => invoke<void>('safe_forget_device_secret', { ...v(), slot }),
    sshStatus: () => invoke<SshStatus>('safe_ssh_status', v()),
    cliStatus: () => invoke<CliStatus>('safe_cli_status'),
    getSettings: () => invoke<Settings>('safe_get_settings', v()),
    setSettings: (settings: Settings) => invoke<Settings>('safe_set_settings', { ...v(), settings }),
  };
}

export type SafeApi = ReturnType<typeof useSafeApi>;
