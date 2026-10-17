import { describe, it, expect, afterEach, vi } from 'vitest';
import { Editor } from '@tiptap/core';
import type { Node as PMNode } from '@tiptap/pm/model';
import StarterKit from '@tiptap/starter-kit';
import TaskList from '@tiptap/extension-task-list';
import TaskItem from '@tiptap/extension-task-item';
import { Markdown } from 'tiptap-markdown';
import { Eisenhower, EisenhowerCell, EisenhowerTask, EisenhowerConvert, type EisenhowerOptions } from '../../extensions/EisenhowerExtension';
import { taskHref, taskIdFromHref } from '../../extensions/eisenhowerMarkdown';
import { DetailsExtension } from '../../extensions/DetailsExtension';
import { lostOnReopen } from '../roundTrip';

let editors: Editor[] = [];
afterEach(() => {
  editors.forEach((e) => e.destroy());
  editors = [];
});

function open(content: string | object, options: EisenhowerOptions = {}): Editor {
  const element = document.createElement('div');
  document.body.appendChild(element);
  const e = new Editor({
    element,
    extensions: [
      StarterKit.configure({ link: { protocols: ['synabit'] } }),
      TaskList,
      TaskItem,
      Markdown.configure({ html: true }),
      Eisenhower.configure({ onDeleted: options.onDeleted }),
      EisenhowerCell,
      EisenhowerTask.configure(options),
      EisenhowerConvert.configure(options),
      DetailsExtension,
    ],
    content,
  });
  editors.push(e);
  return e;
}

const markdownOf = (e: Editor) => (e.storage as any).markdown.getMarkdown() as string;

function matrixOf(e: Editor): PMNode {
  let found: PMNode | null = null;
  e.state.doc.descendants((n) => {
    if (n.type.name === 'eisenhower') found = n;
    return !found;
  });
  if (!found) throw new Error('no matrix');
  return found;
}

/** Each quadrant's lines, a task as `task:<id>`. */
function quadrants(e: Editor): Record<string, string[]> {
  const out: Record<string, string[]> = {};
  matrixOf(e).forEach((cell) => {
    const lines: string[] = [];
    cell.descendants((n) => {
      if (n.type.name !== 'paragraph') return true;
      let task = '';
      n.forEach((c) => { if (c.type.name === 'eisenhowerTask') task = `task:${c.attrs.id}`; });
      const line = task || n.textContent;
      if (line) lines.push(line);
      return false;
    });
    out[cell.attrs.quadrant] = lines;
  });
  return out;
}

const NOTE = `# Tuần này

<!-- eisenhower -->

#### Làm ngay

- Gọi lại cho ngân hàng
- [Nộp thuế quý 3](synabit://task/Tasks/a1b2.md)

#### Lên kế hoạch

- Viết lại phần sync

#### Ủy thác

#### Loại bỏ

- Xem [bài này](https://example.com) sau

<!-- /eisenhower -->

Sau ma trận.`;

describe('an Eisenhower matrix in a note', () => {
  it('reads four quadrants, a task link as the task itself', () => {
    const e = open(NOTE);
    expect(quadrants(e)).toEqual({
      do_first: ['Gọi lại cho ngân hàng', 'task:Tasks/a1b2.md'],
      schedule: ['Viết lại phần sync'],
      delegate: [],
      eliminate: ['Xem bài này sau'],
    });
    // The paragraph after it is still after it, not inside the last quadrant.
    expect(e.state.doc.lastChild?.textContent).toBe('Sau ma trận.');
  });

  it('keeps a link that is only part of a line as a link', () => {
    const e = open(NOTE);
    let href = '';
    e.state.doc.descendants((n) => {
      n.marks.forEach((m) => { if (m.type.name === 'link') href = m.attrs.href; });
      return true;
    });
    expect(href).toBe('https://example.com');
  });

  it('comes back the same after saving and opening again', () => {
    const first = open(NOTE);
    const saved = markdownOf(first);
    expect(lostOnReopen(first, first.state.doc, saved)).toEqual({ words: 0, links: 0, firstBlock: null });

    const second = open(saved);
    expect(quadrants(second)).toEqual(quadrants(first));
    expect(markdownOf(second)).toBe(saved);
    expect(second.state.doc.lastChild?.textContent).toBe('Sau ma trận.');
  });

  it('writes the task as a link any Markdown reader can follow', () => {
    const saved = markdownOf(open(NOTE));
    expect(saved).toContain('[Nộp thuế quý 3](synabit://task/Tasks/a1b2.md)');
    expect(saved).toContain('<!-- eisenhower -->');
    expect(saved).toContain('<!-- /eisenhower -->');
  });

  it('knows a quadrant by its place, not by the words of its heading', () => {
    const e = open(`<!-- eisenhower -->

#### Do first

- a

#### Schedule

- b

<!-- /eisenhower -->`);
    expect(quadrants(e)).toEqual({ do_first: ['a'], schedule: ['b'], delegate: [], eliminate: [] });
  });

  it('puts lines written before the first heading in the first quadrant', () => {
    const e = open(`<!-- eisenhower -->

- trước

#### Lên kế hoạch

- b

<!-- /eisenhower -->`);
    expect(quadrants(e).do_first).toEqual(['trước']);
    expect(quadrants(e).schedule).toEqual(['b']);
  });

  it('round-trips a task whose path has spaces and brackets', () => {
    const id = 'Tasks/Việc (cũ) số 1.md';
    expect(taskIdFromHref(taskHref(id))).toBe(id);
    const e = open({
      type: 'doc',
      content: [{
        type: 'eisenhower',
        content: ['do_first', 'schedule', 'delegate', 'eliminate'].map((quadrant, i) => ({
          type: 'eisenhowerCell',
          attrs: { quadrant },
          content: [{ type: 'paragraph', content: i === 0 ? [{ type: 'eisenhowerTask', attrs: { id, title: 'Việc [cũ] *gấp*' } }] : [] }],
        })),
      }],
    });
    const reopened = open(markdownOf(e));
    let attrs: Record<string, string> | null = null;
    reopened.state.doc.descendants((n) => {
      if (n.type.name === 'eisenhowerTask') attrs = n.attrs as Record<string, string>;
      return true;
    });
    expect(attrs).toEqual({ id, title: 'Việc [cũ] *gấp*' });
  });

  it('comes back whole from inside a toggle, a list and a quote', () => {
    const matrix = (prefix: string) => ({
      type: 'eisenhower',
      content: ['do_first', 'schedule', 'delegate', 'eliminate'].map((quadrant) => ({
        type: 'eisenhowerCell',
        attrs: { quadrant },
        content: [{ type: 'paragraph', content: [{ type: 'text', text: `${prefix}${quadrant}` }] }],
      })),
    });
    const after = { type: 'paragraph', content: [{ type: 'text', text: 'sau' }] };
    const docs = [
      { type: 'doc', content: [{ type: 'details', attrs: { summary: 'S' }, content: [matrix('d-')] }, after] },
      { type: 'doc', content: [{ type: 'bulletList', content: [{ type: 'listItem', content: [{ type: 'paragraph', content: [{ type: 'text', text: 'mục' }] }, matrix('l-')] }] }, after] },
      { type: 'doc', content: [{ type: 'blockquote', content: [matrix('q-')] }, after] },
    ];
    // Whether a list is tight, and whether a toggle is open, are not this block's.
    const shape = (e: Editor) => JSON.stringify(e.getJSON(), (k, v) => (k === 'tight' || k === 'open' ? undefined : v));
    for (const doc of docs) {
      const e = open(doc);
      expect(shape(open(markdownOf(e)))).toBe(shape(e));
    }
  });

  it('leaves a lone opening comment as a comment', () => {
    const e = open('<!-- eisenhower -->\n\nChữ thường.');
    let matrix = false;
    e.state.doc.descendants((n) => { if (n.type.name === 'eisenhower') matrix = true; return true; });
    expect(matrix).toBe(false);
    expect(e.state.doc.textContent).toContain('Chữ thường.');
  });

  it('is inserted empty, the caret in the first quadrant', () => {
    const e = open('<p></p>');
    e.commands.insertEisenhower();
    expect(quadrants(e)).toEqual({ do_first: [], schedule: [], delegate: [], eliminate: [] });
    const $from = e.state.selection.$from;
    let quadrant = '';
    for (let d = $from.depth; d > 0; d--) {
      if ($from.node(d).type.name === 'eisenhowerCell') quadrant = $from.node(d).attrs.quadrant;
    }
    expect(quadrant).toBe('do_first');
    // An empty quadrant is saved as just its heading.
    expect(markdownOf(e)).not.toMatch(/^-\s*$/m);
  });

  it('turns a line into a task in its quadrant', async () => {
    const createTask = vi.fn(async () => 'Tasks/new.md');
    const e = open(NOTE, { createTask });
    const buttons = e.view.dom.querySelectorAll<HTMLButtonElement>('.eis-to-task');
    // Every line with words, less the one that is a task already.
    expect(buttons.length).toBe(3);
    const sync = [...buttons].find((b) => b.closest('[data-quadrant]')?.getAttribute('data-quadrant') === 'schedule')!;
    sync.click();
    await vi.waitFor(() => expect(quadrants(e).schedule).toEqual(['task:Tasks/new.md']));
    expect(createTask).toHaveBeenCalledWith('Viết lại phần sync', 'schedule');
    expect(markdownOf(e)).toContain('[Viết lại phần sync](synabit://task/Tasks/new.md)');
  });

  it('leaves the line as it was when the task could not be made', async () => {
    const onConvertFailed = vi.fn();
    const e = open(NOTE, { createTask: async () => { throw new Error('disk full'); }, onConvertFailed });
    e.view.dom.querySelector<HTMLButtonElement>('.eis-to-task')!.click();
    await vi.waitFor(() => expect(onConvertFailed).toHaveBeenCalled());
    expect(quadrants(e).do_first).toEqual(['Gọi lại cho ngân hàng', 'task:Tasks/a1b2.md']);
  });

  it('offers no button where there is no vault to make a task in', () => {
    const e = open(NOTE);
    expect(e.view.dom.querySelector('.eis-to-task')).toBeNull();
  });

  it('is removed whole by the button over it, and comes back on undo', () => {
    const onDeleted = vi.fn();
    const e = open(NOTE, { onDeleted });
    const button = e.view.dom.querySelector<HTMLButtonElement>('.eis-delete');
    expect(button).not.toBeNull();
    button!.click();
    let matrix = false;
    e.state.doc.descendants((n) => { if (n.type.name === 'eisenhower') matrix = true; return !matrix; });
    expect(matrix).toBe(false);
    expect(onDeleted).toHaveBeenCalledOnce();
    // The lines around it are left as they were.
    expect(e.state.doc.textContent).toBe('Tuần nàySau ma trận.');

    e.commands.undo();
    expect(quadrants(e).do_first).toEqual(['Gọi lại cho ngân hàng', 'task:Tasks/a1b2.md']);
  });

  it('goes with one Backspace while there is nothing in it', () => {
    const e = open('<p>Trước</p>');
    e.commands.setTextSelection(e.state.doc.content.size - 1);
    e.commands.enter();
    e.commands.insertEisenhower();
    expect(e.commands.keyboardShortcut('Backspace')).toBe(true);
    let matrix = false;
    e.state.doc.descendants((n) => { if (n.type.name === 'eisenhower') matrix = true; return !matrix; });
    expect(matrix).toBe(false);
  });

  it('stays on Backspace once something is written in it', () => {
    const e = open('<p></p>');
    e.commands.insertEisenhower();
    e.commands.insertContent('việc');
    e.commands.setTextSelection(e.state.selection.from - 'việc'.length);
    e.commands.keyboardShortcut('Backspace');
    expect(quadrants(e).do_first).toEqual(['việc']);
  });
});
