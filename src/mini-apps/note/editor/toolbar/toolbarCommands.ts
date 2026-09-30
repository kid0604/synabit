/**
 * What the formatting toolbar's buttons do, apart from how they look.
 *
 * Kept out of the component so the mapping — which button runs which command,
 * which one reads as pressed, which shortcut it advertises — can be tested
 * without mounting an editor, and so it cannot drift from Tiptap's own
 * keyboard shortcuts: the ones listed here are the ones the extensions bind
 * (StarterKit, TaskList, Blockquote), plus `Mod-k` for a link, which the
 * editor binds itself.
 */

/** The slice of a Tiptap editor the toolbar uses. */
export interface ToolbarEditor {
  isActive: (name: string, attrs?: Record<string, unknown>) => boolean;
  chain: () => any;
}

export type ToolbarActionId =
  | 'bold'
  | 'italic'
  | 'bulletList'
  | 'orderedList'
  | 'taskList'
  | 'blockquote'
  | 'link';

export interface ToolbarAction {
  id: ToolbarActionId;
  /** i18n key for the button's name. */
  labelKey: string;
  /** In Tiptap's notation: `Mod-Shift-8`. */
  shortcut: string;
  /** The mark or node whose presence makes the button read as pressed. */
  activeWhen: string;
  /**
   * Run it. `link` is not here — it opens the link dialog, which belongs to
   * the editor component, and the toolbar asks for it by event instead.
   */
  run?: (editor: ToolbarEditor) => void;
}

export const TOOLBAR_ACTIONS: ToolbarAction[] = [
  {
    id: 'bold', labelKey: 'note.editor.bold', shortcut: 'Mod-b', activeWhen: 'bold',
    run: (e) => e.chain().focus().toggleBold().run(),
  },
  {
    id: 'italic', labelKey: 'note.editor.italic', shortcut: 'Mod-i', activeWhen: 'italic',
    run: (e) => e.chain().focus().toggleItalic().run(),
  },
  {
    id: 'bulletList', labelKey: 'note.slash.bullet_list.title', shortcut: 'Mod-Shift-8', activeWhen: 'bulletList',
    run: (e) => e.chain().focus().toggleBulletList().run(),
  },
  {
    id: 'orderedList', labelKey: 'note.slash.numbered_list.title', shortcut: 'Mod-Shift-7', activeWhen: 'orderedList',
    run: (e) => e.chain().focus().toggleOrderedList().run(),
  },
  {
    id: 'taskList', labelKey: 'note.slash.task_list.title', shortcut: 'Mod-Shift-9', activeWhen: 'taskList',
    run: (e) => e.chain().focus().toggleTaskList().run(),
  },
  {
    id: 'blockquote', labelKey: 'note.slash.blockquote.title', shortcut: 'Mod-Shift-b', activeWhen: 'blockquote',
    run: (e) => e.chain().focus().toggleBlockquote().run(),
  },
  { id: 'link', labelKey: 'note.toolbar.link', shortcut: 'Mod-k', activeWhen: 'link' },
];

/** Normal text, then the three heading levels the slash menu offers. */
export type BlockLevel = 0 | 1 | 2 | 3;

export const BLOCK_LEVELS: { level: BlockLevel; labelKey: string; shortcut: string }[] = [
  { level: 0, labelKey: 'note.toolbar.normal_text', shortcut: 'Mod-Alt-0' },
  { level: 1, labelKey: 'note.slash.heading1.title', shortcut: 'Mod-Alt-1' },
  { level: 2, labelKey: 'note.slash.heading2.title', shortcut: 'Mod-Alt-2' },
  { level: 3, labelKey: 'note.slash.heading3.title', shortcut: 'Mod-Alt-3' },
];

/**
 * Which level the caret's block is at. Anything that is neither a paragraph
 * nor a heading 1–3 (a code block, a heading 4 pasted in from elsewhere) reads
 * as `null`, so the menu marks nothing as current rather than something false.
 */
export function currentBlockLevel(editor: ToolbarEditor): BlockLevel | null {
  for (const level of [1, 2, 3] as const) {
    if (editor.isActive('heading', { level })) return level;
  }
  return editor.isActive('paragraph') ? 0 : null;
}

export function setBlockLevel(editor: ToolbarEditor, level: BlockLevel): void {
  if (level === 0) editor.chain().focus().setParagraph().run();
  else editor.chain().focus().setHeading({ level }).run();
}

/**
 * A Tiptap shortcut written the way the platform writes it.
 *
 * `Mod-Shift-8` is `⌘⇧8` on a Mac — glyphs, no separators, as every Mac menu
 * shows them — and `Ctrl+Shift+8` everywhere else.
 */
export function formatShortcut(shortcut: string, isMac: boolean): string {
  const parts = shortcut.split('-');
  const key = parts.pop() ?? '';
  const mods = parts.map((m) => {
    switch (m) {
      case 'Mod': return isMac ? '⌘' : 'Ctrl';
      case 'Shift': return isMac ? '⇧' : 'Shift';
      case 'Alt': return isMac ? '⌥' : 'Alt';
      case 'Ctrl': return isMac ? '⌃' : 'Ctrl';
      default: return m;
    }
  });
  const shownKey = key.length === 1 ? key.toUpperCase() : key;
  return isMac ? [...mods, shownKey].join('') : [...mods, shownKey].join('+');
}

/** "Bold (⌘B)" — the button's name with its shortcut, for `title` and `aria-label`. */
export function labelWithShortcut(label: string, shortcut: string | undefined, isMac: boolean): string {
  return shortcut ? `${label} (${formatShortcut(shortcut, isMac)})` : label;
}

/**
 * Where the roving focus goes next in a toolbar of `count` buttons.
 *
 * Left and right wrap, as the ARIA toolbar pattern suggests; Home and End go
 * to the ends. `null` for any other key, which the toolbar leaves alone.
 */
export function nextToolbarIndex(current: number, key: string, count: number): number | null {
  if (count <= 0) return null;
  switch (key) {
    case 'ArrowRight': return (current + 1) % count;
    case 'ArrowLeft': return (current - 1 + count) % count;
    case 'Home': return 0;
    case 'End': return count - 1;
    default: return null;
  }
}
