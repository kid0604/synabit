/**
 * What each kind of item looks like, and the fields a new one starts with.
 *
 * Only a starting point: every field can be renamed, removed, or joined by
 * others. The kind decides the icon and the template, nothing else — which is
 * why Rust reads an unknown kind as `other` and loses nothing but the picture.
 */
import type { Component } from 'vue';
import { CreditCard, FileLock, Globe, IdCard, KeyRound, Package, Terminal, Wifi } from 'lucide-vue-next';
import type { FieldKind, ItemKind } from './api';

export interface KindInfo {
  kind: ItemKind;
  icon: Component;
  /** Label keys under `safe.field` for the fields a new item starts with. */
  fields: { label: string; kind: FieldKind }[];
  /** Whether a new item of this kind asks for a website. */
  url: boolean;
}

export const KINDS: KindInfo[] = [
  { kind: 'login', icon: Globe, url: true, fields: [{ label: 'username', kind: 'username' }, { label: 'password', kind: 'password' }] },
  { kind: 'api_key', icon: KeyRound, url: true, fields: [{ label: 'token', kind: 'concealed' }] },
  { kind: 'ssh_key', icon: Terminal, url: false, fields: [{ label: 'private_key', kind: 'concealed' }, { label: 'public_key', kind: 'multiline' }, { label: 'passphrase', kind: 'password' }] },
  { kind: 'card', icon: CreditCard, url: false, fields: [{ label: 'cardholder', kind: 'text' }, { label: 'number', kind: 'concealed' }, { label: 'expiry', kind: 'text' }, { label: 'cvv', kind: 'pin' }, { label: 'pin', kind: 'pin' }] },
  { kind: 'secure_note', icon: FileLock, url: false, fields: [] },
  { kind: 'identity', icon: IdCard, url: false, fields: [{ label: 'full_name', kind: 'text' }, { label: 'email', kind: 'email' }, { label: 'phone', kind: 'phone' }, { label: 'id_number', kind: 'concealed' }] },
  { kind: 'wifi', icon: Wifi, url: false, fields: [{ label: 'network', kind: 'text' }, { label: 'password', kind: 'password' }] },
  { kind: 'other', icon: Package, url: false, fields: [{ label: 'value', kind: 'concealed' }] },
];

export function kindInfo(kind: ItemKind): KindInfo {
  return KINDS.find((k) => k.kind === kind) ?? KINDS[KINDS.length - 1];
}

/** Field kinds whose value is hidden until asked for. Mirrors `FieldKind::is_concealed`. */
export const CONCEALED: FieldKind[] = ['password', 'concealed', 'pin', 'unknown'];

export const FIELD_KINDS: FieldKind[] = ['text', 'username', 'email', 'url', 'phone', 'date', 'multiline', 'password', 'concealed', 'pin'];
