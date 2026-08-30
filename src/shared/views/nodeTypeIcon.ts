import {
  FileText, CheckSquare, Calendar, Users, Zap, Palette, FolderOpen,
  Wallet, Rss, Filter, Box,
} from 'lucide-vue-next';
import type { Component } from 'vue';

/**
 * An icon for a node type, with a shape for the ones nobody has heard of.
 *
 * The fallback is the point. A vault may carry a type this app has never seen —
 * somebody else's tool wrote it, or the user invented one — and a view that
 * refused to draw a row without a known icon would be a list in the code
 * deciding which of the user's things are real.
 *
 * `Box` for the unknown ones: a container, unopinionated about what is in it,
 * and visibly different from the known types so the distinction reads without
 * needing an explanation.
 */
const ICONS: Readonly<Record<string, Component>> = {
  note: FileText,
  task: CheckSquare,
  project: CheckSquare,
  event: Calendar,
  person: Users,
  interaction: Users,
  quickcap: Zap,
  whiteboard: Palette,
  file: FolderOpen,
  finance_month: Wallet,
  feed_source: Rss,
  filter: Filter,
};

export function iconForNodeType(nodeType: string): Component {
  return ICONS[nodeType] ?? Box;
}

/** Whether this app ships a screen that understands the type. */
export function isKnownNodeType(nodeType: string): boolean {
  return nodeType in ICONS;
}
