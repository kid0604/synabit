import { describe, it, expect } from 'vitest';

import { buildFocus, readSelection, describeFocus, MAX_SELECTION_READ } from '../focus';

/**
 * What Syn is told about the screen.
 *
 * These rules decide whether "viết lại đoạn này cho gọn" is a sentence Syn can
 * act on, so each one is here as a case rather than as a comment. The reading
 * itself is split from the DOM globals precisely so it can be checked without
 * a browser and without mounting anything.
 */
describe('reading what is on screen', () => {
  it('takes the document selection', () => {
    expect(readSelection(null, 'giá per-seat')).toBe('giá per-seat');
  });

  /**
   * A selection inside `<input>` or `<textarea>` does not appear in
   * `window.getSelection()`. Without this, highlighting half a task title and
   * asking about it would be answered as though nothing were highlighted.
   */
  it('takes a selection inside a form field, which the document cannot see', () => {
    const field = document.createElement('textarea');
    field.value = 'chuẩn bị demo cho thứ Sáu';
    // Found rather than counted. The first version of this test hand-counted
    // the offsets, got them wrong by three, and read as the code being broken.
    // Vietnamese is exactly where counting UTF-16 offsets by eye goes wrong.
    const wanted = 'demo cho';
    field.selectionStart = field.value.indexOf(wanted);
    field.selectionEnd = field.selectionStart + wanted.length;

    expect(readSelection(field, null)).toBe(wanted);
  });

  it('prefers the field over the document when the caret is in one', () => {
    const field = document.createElement('input');
    field.value = 'trong ô nhập';
    field.selectionStart = 0;
    field.selectionEnd = 5;

    expect(readSelection(field, 'ngoài trang')).toBe('trong');
  });

  /** A caret is not a selection. Clicking into a field selects nothing. */
  it('treats a collapsed caret as nothing selected', () => {
    const field = document.createElement('input');
    field.value = 'abc';
    field.selectionStart = 2;
    field.selectionEnd = 2;

    expect(readSelection(field, null)).toBeUndefined();
  });

  /**
   * A double-click on a blank line selects whitespace. Sending it would have
   * the prompt claim something is highlighted when nothing is.
   */
  it('treats whitespace as nothing selected', () => {
    expect(readSelection(null, '   \n\t ')).toBeUndefined();
    expect(readSelection(null, '')).toBeUndefined();
    expect(readSelection(null, null)).toBeUndefined();
  });

  /** Select-all on a long note must not put a megabyte across the IPC call. */
  it('cuts a selection that is longer than anything worth sending', () => {
    const huge = 'x'.repeat(MAX_SELECTION_READ + 5_000);
    expect(readSelection(null, huge)).toHaveLength(MAX_SELECTION_READ);
  });
});

describe('what gets sent with a question', () => {
  it('carries the app, the open node and the selection', () => {
    expect(buildFocus({ app: 'note', node: 'Notes/pricing.md' }, 'per-seat')).toEqual({
      app: 'note',
      node: 'Notes/pricing.md',
      selection: 'per-seat',
    });
  });

  /**
   * Absent keys rather than empty ones. The Rust side skips serialising
   * `None`, and a `""` arriving where it expects an absent field would render
   * a heading over nothing.
   */
  it('leaves out what there is nothing to say about', () => {
    expect(buildFocus({ app: 'task' }, undefined)).toEqual({ app: 'task' });
  });

  /**
   * Being on a screen at all is worth saying — somebody on the Tasks board
   * with nothing selected has still told Syn something.
   */
  it('sends the app on its own', () => {
    expect(buildFocus({ app: 'task' }, undefined)).toBeDefined();
  });

  /**
   * The thread is chosen by a person rather than read off the screen, and it
   * travels in the same envelope. A question asked from a screen with nothing
   * on it, inside a piece of work, still has something to say.
   */
  it('carries the thread, on its own if that is all there is', () => {
    expect(buildFocus({ app: 'note', thread: 'SynThreads/Pricing.md' }, undefined)).toEqual({
      app: 'note',
      thread: 'SynThreads/Pricing.md',
    });
    expect(buildFocus({ app: '', thread: 'SynThreads/Pricing.md' }, undefined)).toEqual({
      app: '',
      thread: 'SynThreads/Pricing.md',
    });
  });

  /** Nothing at all sends nothing, so the prompt renders no section. */
  it('sends nothing when there is nothing', () => {
    expect(buildFocus({ app: '' }, undefined)).toBeUndefined();
    expect(buildFocus({ app: '   ' }, undefined)).toBeUndefined();
  });
});

describe('what the bar shows about it', () => {
  /**
   * A path names the file and a title names the thing, and in a vault whose
   * notes are named by uuid they share nothing. The bar showed
   * `Notes/4e0bc181-e384-40d2-ab08-268c98ef…` in the first screenshot of this
   * working — which told the person reading it precisely nothing.
   */
  it('shows the title to a person, while the path still travels to Syn', () => {
    const focus = buildFocus(
      { app: 'note', node: 'Notes/4e0bc181.md', nodeTitle: 'Lỗi kênh truyền 2025-05-26' },
      'a selection',
    );

    expect(focus?.node, 'the model gets something it can address').toBe('Notes/4e0bc181.md');
    expect(describeFocus(focus).node, 'the person gets a name').toBe('Lỗi kênh truyền 2025-05-26');
  });

  /** A title with no path is a name nothing can act on. */
  it('does not send a title with nothing behind it', () => {
    expect(buildFocus({ app: 'note', nodeTitle: 'Orphan' }, undefined)).toEqual({ app: 'note' });
  });

  it('falls back to the path when nothing knows the title', () => {
    const focus = buildFocus({ app: 'file', node: 'Files/report.pdf' }, undefined);
    expect(describeFocus(focus).node).toBe('Files/report.pdf');
  });

  /**
   * The bar says what it picked up. The difference between "Syn ignored me"
   * and "Syn never had the paragraph" is invisible unless the screen says
   * which one happened.
   */
  it('reports the node and how much text was picked up', () => {
    expect(describeFocus(buildFocus({ app: 'note', node: 'Notes/a.md' }, 'bốn chữ ở đây'))).toEqual({
      node: 'Notes/a.md',
      chars: 13,
    });
  });

  it('reports nothing when nothing was picked up', () => {
    expect(describeFocus(buildFocus({ app: 'task' }, undefined))).toEqual({});
    expect(describeFocus(undefined)).toEqual({});
  });
});
