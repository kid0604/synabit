import { describe, it, expect, vi } from 'vitest';
import {
  TOOLBAR_ACTIONS,
  BLOCK_LEVELS,
  currentBlockLevel,
  setBlockLevel,
  formatShortcut,
  labelWithShortcut,
  nextToolbarIndex,
  type ToolbarEditor,
} from '../toolbar/toolbarCommands';

vi.mock('@tauri-apps/api/core', () => ({ invoke: vi.fn(), convertFileSrc: (s: string) => s }));
vi.mock('@tauri-apps/plugin-dialog', () => ({ open: vi.fn() }));
vi.mock('@tauri-apps/plugin-fs', () => ({ readFile: vi.fn() }));

/** An editor that records the chain it was asked to run. */
function fakeEditor(active: Record<string, boolean | ((attrs?: any) => boolean)> = {}) {
  const calls: string[][] = [];
  const editor: ToolbarEditor = {
    isActive: (name, attrs) => {
      const v = active[name];
      return typeof v === 'function' ? v(attrs) : !!v;
    },
    chain: () => {
      const steps: string[] = [];
      const proxy: any = new Proxy({}, {
        get: (_t, prop: string) => (...args: unknown[]) => {
          if (prop === 'run') { calls.push(steps); return true; }
          steps.push(args.length ? `${prop}(${JSON.stringify(args[0])})` : prop);
          return proxy;
        },
      });
      return proxy;
    },
  };
  return { editor, calls };
}

describe('toolbar actions', () => {
  it('run the command their name says, after focusing the editor', () => {
    const expected: Record<string, string> = {
      bold: 'toggleBold',
      italic: 'toggleItalic',
      bulletList: 'toggleBulletList',
      orderedList: 'toggleOrderedList',
      taskList: 'toggleTaskList',
      blockquote: 'toggleBlockquote',
    };
    for (const action of TOOLBAR_ACTIONS.filter((a) => a.run)) {
      const { editor, calls } = fakeEditor();
      action.run!(editor);
      expect(calls).toEqual([['focus', expected[action.id]]]);
    }
  });

  it('leaves the link to the editor, which owns the dialog', () => {
    const link = TOOLBAR_ACTIONS.find((a) => a.id === 'link')!;
    expect(link.run).toBeUndefined();
    expect(link.shortcut).toBe('Mod-k');
  });

  it('advertise the shortcuts Tiptap actually binds', () => {
    const byId = Object.fromEntries(TOOLBAR_ACTIONS.map((a) => [a.id, a.shortcut]));
    expect(byId).toMatchObject({
      bold: 'Mod-b', italic: 'Mod-i',
      bulletList: 'Mod-Shift-8', orderedList: 'Mod-Shift-7', taskList: 'Mod-Shift-9',
      blockquote: 'Mod-Shift-b',
    });
    expect(BLOCK_LEVELS.map((b) => b.shortcut)).toEqual(['Mod-Alt-0', 'Mod-Alt-1', 'Mod-Alt-2', 'Mod-Alt-3']);
  });
});

describe('block level', () => {
  it('reads the heading level at the caret', () => {
    expect(currentBlockLevel(fakeEditor({ heading: (a) => a?.level === 2 }).editor)).toBe(2);
    expect(currentBlockLevel(fakeEditor({ paragraph: true }).editor)).toBe(0);
    expect(currentBlockLevel(fakeEditor({ codeBlock: true }).editor)).toBeNull();
  });

  it('sets a paragraph for level 0 and a heading otherwise', () => {
    const a = fakeEditor();
    setBlockLevel(a.editor, 0);
    expect(a.calls).toEqual([['focus', 'setParagraph']]);
    const b = fakeEditor();
    setBlockLevel(b.editor, 3);
    expect(b.calls).toEqual([['focus', 'setHeading({"level":3})']]);
  });
});

describe('formatShortcut', () => {
  it('writes shortcuts the Mac way on a Mac', () => {
    expect(formatShortcut('Mod-b', true)).toBe('⌘B');
    expect(formatShortcut('Mod-Shift-8', true)).toBe('⌘⇧8');
    expect(formatShortcut('Mod-Alt-1', true)).toBe('⌘⌥1');
  });

  it('writes them with Ctrl and plus signs elsewhere', () => {
    expect(formatShortcut('Mod-b', false)).toBe('Ctrl+B');
    expect(formatShortcut('Mod-Shift-8', false)).toBe('Ctrl+Shift+8');
    expect(formatShortcut('Mod-Alt-0', false)).toBe('Ctrl+Alt+0');
  });

  it('puts the shortcut after the name', () => {
    expect(labelWithShortcut('Bold', 'Mod-b', false)).toBe('Bold (Ctrl+B)');
    expect(labelWithShortcut('Insert', undefined, true)).toBe('Insert');
  });
});

describe('nextToolbarIndex', () => {
  it('wraps with the arrow keys and jumps with Home and End', () => {
    expect(nextToolbarIndex(0, 'ArrowRight', 3)).toBe(1);
    expect(nextToolbarIndex(2, 'ArrowRight', 3)).toBe(0);
    expect(nextToolbarIndex(0, 'ArrowLeft', 3)).toBe(2);
    expect(nextToolbarIndex(1, 'Home', 3)).toBe(0);
    expect(nextToolbarIndex(1, 'End', 3)).toBe(2);
  });

  it('ignores other keys and empty toolbars', () => {
    expect(nextToolbarIndex(0, 'Enter', 3)).toBeNull();
    expect(nextToolbarIndex(0, 'ArrowRight', 0)).toBeNull();
  });
});

describe('slash items in simple mode', () => {
  const deps = () => ({
    vaultPath: '/v',
    videoModal: { value: { show: false, url: '' } },
    audioModal: { value: { show: false, url: '' } },
    locationModal: { value: {} },
    routeModal: { value: {} },
    emojiPicker: { value: {} },
    whiteboardPickerModal: { value: {} },
    embedPickerModal: { value: false },
    pdfModal: { value: { show: false } },
  });

  it('leaves out the power tools, and only in simple mode', async () => {
    const { createSlashCommandItems, visibleSlashItems } = await import('../config/slashCommandItems');
    const all = createSlashCommandItems(deps());
    const simple = visibleSlashItems(all, true).map((i) => i.title);
    for (const hidden of ['Code Block', 'Query', 'Markmap', 'Equation', 'Embed', 'Whiteboard']) {
      expect(simple).not.toContain(hidden);
    }
    for (const kept of ['Text', 'Heading 1', 'Bullet List', 'Task List', 'Image', 'Table']) {
      expect(simple).toContain(kept);
    }
    expect(visibleSlashItems(all, false)).toHaveLength(all.length);
  });

  it('offers Template only to an editor that asked for it', async () => {
    const { createSlashCommandItems } = await import('../config/slashCommandItems');
    expect(createSlashCommandItems(deps()).some((i) => i.title === 'Template')).toBe(false);

    const onTemplate = vi.fn();
    const item = createSlashCommandItems({ ...deps(), onTemplate }).find((i) => i.title === 'Template')!;
    const { editor, calls } = fakeEditor();
    item.command({ editor, range: { from: 1, to: 3 } });
    expect(calls).toEqual([['focus', 'deleteRange({"from":1,"to":3})']]);
    expect(onTemplate).toHaveBeenCalledOnce();
  });
});
