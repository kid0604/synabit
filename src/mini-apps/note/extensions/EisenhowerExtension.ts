import { Node, Extension, mergeAttributes } from '@tiptap/core';
import { Plugin, PluginKey, TextSelection } from '@tiptap/pm/state';
import { Decoration, DecorationSet, type EditorView } from '@tiptap/pm/view';
import type { Node as PMNode } from '@tiptap/pm/model';
import { VueNodeViewRenderer } from '@tiptap/vue-3';
import { i18n } from '../../../i18n';
import { QUADRANTS, isQuadrant } from '../../task/quadrant';
import { CLOSE_COMMENT, OPEN_COMMENT, eisenhowerPlugin, escapeLinkText, taskHref } from './eisenhowerMarkdown';
import { quadrantAt, type EisenhowerOptions } from './eisenhowerShared';
import EisenhowerTaskNodeView from '../nodes/EisenhowerTaskNodeView.vue';
import './eisenhower.css';

export type { EisenhowerOptions, MatrixTask } from './eisenhowerShared';

declare module '@tiptap/core' {
  interface Commands<ReturnType> {
    eisenhower: {
      /** An empty matrix here, the caret in its first quadrant. */
      insertEisenhower: () => ReturnType;
    };
  }
}

const t = i18n.global.t;
const labelKey = (quadrant: string) => `task.matrix_${quadrant}`;

/** Nothing written in it: no words, no tasks. Saved as just its heading. */
function isEmptyCell(cell: PMNode): boolean {
  if (cell.textContent.trim() !== '') return false;
  let task = false;
  cell.descendants((n) => {
    if (n.type.name === 'eisenhowerTask') task = true;
    return !task;
  });
  return !task;
}

const emptyCell = (quadrant: string) => ({
  type: 'eisenhowerCell',
  attrs: { quadrant },
  content: [{ type: 'bulletList', content: [{ type: 'listItem', content: [{ type: 'paragraph' }] }] }],
});

/** Lucide's `trash-2`, drawn here because this view is not a Vue component. */
const TRASH_ICON = '<svg xmlns="http://www.w3.org/2000/svg" width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true"><path d="M3 6h18"/><path d="M19 6v14c0 1-1 2-2 2H7c-1 0-2-1-2-2V6"/><path d="M8 6V4c0-1 1-2 2-2h4c1 0 2 1 2 2v2"/><line x1="10" x2="10" y1="11" y2="17"/><line x1="14" x2="14" y1="11" y2="17"/></svg>';

/** Nothing in any quadrant. */
function isEmptyMatrix(matrix: PMNode): boolean {
  let empty = true;
  matrix.forEach((cell) => { if (!isEmptyCell(cell)) empty = false; });
  return empty;
}

export const Eisenhower = Node.create<Pick<EisenhowerOptions, 'onDeleted'>>({
  name: 'eisenhower',
  group: 'block',
  content: 'eisenhowerCell{4}',
  defining: true,
  isolating: true,
  // Ahead of the list's own Backspace, which would otherwise lift the empty
  // item out of its list rather than let an empty matrix go.
  priority: 1000,

  addOptions() {
    return {};
  },

  addKeyboardShortcuts() {
    return {
      // A matrix with nothing in it, the caret at the start of a line in it:
      // Backspace takes the whole matrix, as it would an empty line. One put
      // in by mistake should not need finding a button to undo.
      Backspace: ({ editor }) => {
        const { selection } = editor.state;
        if (!selection.empty || selection.$from.parentOffset !== 0) return false;
        const $from = selection.$from;
        for (let d = $from.depth; d > 0; d--) {
          const n = $from.node(d);
          if (n.type.name !== this.name) continue;
          if (!isEmptyMatrix(n)) return false;
          const from = $from.before(d);
          return editor.chain().deleteRange({ from, to: from + n.nodeSize }).run();
        }
        return false;
      },
    };
  },

  /**
   * The grid, and over it a header with a button that removes the whole matrix —
   * the one way to do it that anybody would find. Selecting it with
   * Backspace from the line below still works, and was all there was.
   */
  addNodeView() {
    const onDeleted = this.options.onDeleted;
    return ({ editor, getPos }) => {
      const dom = document.createElement('div');
      dom.className = 'eis-matrix';
      dom.setAttribute('data-type', 'eisenhower');

      const bar = document.createElement('div');
      bar.className = 'eis-bar';
      bar.contentEditable = 'false';
      const remove = document.createElement('button');
      remove.type = 'button';
      remove.className = 'eis-delete';
      remove.innerHTML = TRASH_ICON;
      remove.title = t('note.eisenhower.delete');
      remove.setAttribute('aria-label', t('note.eisenhower.delete'));
      remove.addEventListener('mousedown', (e) => e.preventDefault());
      remove.addEventListener('click', (e) => {
        e.preventDefault();
        const pos = typeof getPos === 'function' ? getPos() : undefined;
        if (typeof pos !== 'number') return;
        const node = editor.state.doc.nodeAt(pos);
        if (!node || node.type.name !== 'eisenhower') return;
        editor.chain().focus().deleteRange({ from: pos, to: pos + node.nodeSize }).run();
        onDeleted?.();
      });
      const title = document.createElement('span');
      title.className = 'eis-bar-title';
      title.textContent = t('note.slash.eisenhower.title');
      bar.append(title, remove);

      const grid = document.createElement('div');
      grid.className = 'eis-grid';
      dom.append(bar, grid);

      return {
        dom,
        contentDOM: grid,
        stopEvent: (event) => bar.contains(event.target as globalThis.Node),
        ignoreMutation: (mutation) => mutation.type !== 'selection' && bar.contains(mutation.target),
      };
    };
  },

  parseHTML() {
    return [{
      tag: 'div[data-type="eisenhower"]',
      contentElement: (el: HTMLElement) => (el.querySelector(':scope > .eis-grid') || el) as HTMLElement,
    }];
  },

  renderHTML({ HTMLAttributes }) {
    // A wrapper around the grid, because a container query measures the
    // element it is declared on and lays out its children.
    return ['div', mergeAttributes(HTMLAttributes, { 'data-type': 'eisenhower', class: 'eis-matrix' }), ['div', { class: 'eis-grid' }, 0]];
  },

  addCommands() {
    return {
      insertEisenhower:
        () =>
        ({ chain }) =>
          chain()
            .insertContent({ type: this.name, content: QUADRANTS.map(emptyCell) })
            .command(({ tr }) => {
              // Into the first quadrant, ready to type, rather than after the
              // block where `insertContent` leaves the caret.
              const end = tr.selection.from;
              let at = -1;
              tr.doc.nodesBetween(0, Math.min(end, tr.doc.content.size), (n, pos) => {
                if (n.type.name === this.name && pos < end) at = pos;
              });
              if (at >= 0) tr.setSelection(TextSelection.near(tr.doc.resolve(at + 1)));
              return true;
            })
            .run(),
    };
  },

  addStorage() {
    return {
      markdown: {
        serialize(state: any, node: PMNode) {
          state.write(OPEN_COMMENT);
          state.closeBlock(node);
          node.forEach((cell) => {
            state.write(`#### ${t(labelKey(cell.attrs.quadrant))}`);
            state.closeBlock(cell);
            if (!isEmptyCell(cell)) state.renderContent(cell);
          });
          state.write(CLOSE_COMMENT);
          state.closeBlock(node);
        },
        parse: {
          setup(markdownit: any) {
            eisenhowerPlugin(markdownit);
          },
        },
      },
    };
  },
});

export const EisenhowerCell = Node.create({
  name: 'eisenhowerCell',
  // Text and lists. A heading would read back as the start of the next
  // quadrant, so there is none.
  content: '(paragraph | list)+',
  isolating: true,

  addAttributes() {
    return {
      quadrant: {
        default: 'do_first',
        parseHTML: (el: HTMLElement) => {
          const q = el.getAttribute('data-quadrant');
          return isQuadrant(q) ? q : 'do_first';
        },
        renderHTML: (attrs: Record<string, any>) => ({ 'data-quadrant': attrs.quadrant }),
      },
    };
  },

  parseHTML() {
    return [{
      tag: 'div[data-type="eisenhower-cell"]',
      contentElement: (el: HTMLElement) => (el.querySelector(':scope > .eis-cell-body') || el) as HTMLElement,
    }];
  },

  renderHTML({ HTMLAttributes, node }) {
    const q = node.attrs.quadrant;
    return [
      'div',
      mergeAttributes(HTMLAttributes, { 'data-type': 'eisenhower-cell', class: 'eis-cell' }),
      ['div', { class: 'eis-cell-label', contenteditable: 'false' },
        ['span', { class: 'eis-cell-title' }, t(labelKey(q))],
        ['span', { class: 'eis-cell-desc' }, t(`${labelKey(q)}_desc`)],
      ],
      ['div', { class: 'eis-cell-body' }, 0],
    ];
  },

  addStorage() {
    return {
      markdown: {
        // Written by `Eisenhower`, which owns the headings; this is for a
        // cell serialized on its own, as a copy does.
        serialize(state: any, node: PMNode) {
          state.renderContent(node);
        },
      },
    };
  },
});

/**
 * A task in a quadrant, drawn from the task as it is now. In the file it is
 * a link — `[title](synabit://task/<path>)` — which is also how backlinks see
 * it; the title written there is only for whoever reads the file elsewhere.
 */
export const EisenhowerTask = Node.create<EisenhowerOptions>({
  name: 'eisenhowerTask',
  group: 'inline',
  inline: true,
  atom: true,
  selectable: true,

  addOptions() {
    return {};
  },

  addAttributes() {
    return {
      id: {
        default: '',
        parseHTML: (el: HTMLElement) => el.getAttribute('data-id') ?? '',
        renderHTML: (attrs: Record<string, any>) => ({ 'data-id': attrs.id }),
      },
      title: {
        default: '',
        parseHTML: (el: HTMLElement) => el.getAttribute('data-title') ?? el.textContent ?? '',
        renderHTML: (attrs: Record<string, any>) => ({ 'data-title': attrs.title }),
      },
    };
  },

  parseHTML() {
    return [{ tag: 'span[data-type="eisenhower-task"]' }];
  },

  renderHTML({ HTMLAttributes, node }) {
    return ['span', mergeAttributes(HTMLAttributes, { 'data-type': 'eisenhower-task' }), node.attrs.title];
  },

  renderText({ node }) {
    return node.attrs.title;
  },

  addNodeView() {
    return VueNodeViewRenderer(EisenhowerTaskNodeView as any, {
      stopEvent: ({ event }) => !!(event.target as Element | null)?.closest?.('[data-eis-interactive]'),
    });
  },

  addStorage() {
    return {
      markdown: {
        serialize(state: any, node: PMNode) {
          state.write(`[${escapeLinkText(node.attrs.title || node.attrs.id)}](${taskHref(node.attrs.id)})`);
        },
      },
    };
  },
});

const convertKey = new PluginKey('eisenhowerConvert');

/** Whether a line in a quadrant can become a task: it has words, and is not one already. */
function convertible(paragraph: PMNode): boolean {
  if (paragraph.textContent.trim() === '') return false;
  let task = false;
  paragraph.forEach((n) => { if (n.type.name === 'eisenhowerTask') task = true; });
  return !task;
}

/**
 * Turn the line ending at `end` into a task. Found again by its words once
 * the task exists, since the note may have been edited while it was made.
 */
async function convert(view: EditorView, end: number, options: EisenhowerOptions): Promise<void> {
  const { createTask } = options;
  if (!createTask) return;
  const $end = view.state.doc.resolve(end);
  const paragraph = $end.parent;
  const title = paragraph.textContent.trim();
  const quadrant = quadrantAt(view.state.doc, end);
  if (!title || !quadrant) return;

  let id: string;
  try {
    id = await createTask(title, quadrant);
  } catch (e) {
    options.onConvertFailed?.(e);
    return;
  }
  if (view.isDestroyed) return;

  const doc = view.state.doc;
  let target: { from: number; to: number } | null = null;
  doc.descendants((n, pos) => {
    if (target) return false;
    if (n.type.name === 'paragraph' && n.textContent.trim() === title && convertible(n) && quadrantAt(doc, pos + 1)) {
      target = { from: pos + 1, to: pos + 1 + n.content.size };
      return false;
    }
    return true;
  });
  if (!target) return;
  const { from, to } = target;
  view.dispatch(view.state.tr.replaceWith(from, to, view.state.schema.nodes.eisenhowerTask.create({ id, title })));
}

/**
 * A "→ task" button at the end of every line in a quadrant that could become
 * one. Shown on hover, or always where there is no hover to show it by.
 */
export const EisenhowerConvert = Extension.create<EisenhowerOptions>({
  name: 'eisenhowerConvert',

  addOptions() {
    return {};
  },

  addProseMirrorPlugins() {
    const options = this.options;
    if (!options.createTask) return [];
    const build = (doc: PMNode): DecorationSet => {
      const decorations: Decoration[] = [];
      doc.descendants((node, pos) => {
        if (node.type.name !== 'eisenhowerCell') return !node.isTextblock;
        node.descendants((child, offset) => {
          if (child.type.name !== 'paragraph') return true;
          if (convertible(child)) {
            const end = pos + 1 + offset + 1 + child.content.size;
            decorations.push(Decoration.widget(end, (view) => convertButton(view, end, options), {
              side: 1,
              ignoreSelection: true,
              key: `eis-${end}-${child.textContent}`,
            }));
          }
          return false;
        });
        return false;
      });
      return DecorationSet.create(doc, decorations);
    };
    return [
      new Plugin({
        key: convertKey,
        state: {
          init: (_config, state) => build(state.doc),
          apply: (tr, old) => (tr.docChanged ? build(tr.doc) : old),
        },
        props: {
          decorations(state) {
            return convertKey.getState(state);
          },
        },
      }),
    ];
  },
});

function convertButton(view: EditorView, end: number, options: EisenhowerOptions): HTMLElement {
  const button = document.createElement('button');
  button.type = 'button';
  button.className = 'eis-to-task';
  button.textContent = '→';
  button.title = t('note.eisenhower.to_task');
  button.setAttribute('aria-label', t('note.eisenhower.to_task'));
  button.contentEditable = 'false';
  // Keep the caret where it was: a click on the button is not a click into
  // the text.
  button.addEventListener('mousedown', (e) => e.preventDefault());
  button.addEventListener('click', (e) => {
    e.preventDefault();
    if (button.disabled) return;
    button.disabled = true;
    void convert(view, end, options).finally(() => { button.disabled = false; });
  });
  return button;
}
