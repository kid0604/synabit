import { describe, it, expect, afterEach, vi } from 'vitest';
import { Editor } from '@tiptap/core';
import StarterKit from '@tiptap/starter-kit';
import { replaceFromOutside } from '../replaceFromOutside';

let editor: Editor;

const make = (html: string) => {
  editor = new Editor({ element: document.createElement('div'), extensions: [StarterKit], content: html });
  return editor;
};

/** Type at the end of the first paragraph, the way a keystroke lands. */
const typeInFirstParagraph = (text: string) => {
  const end = editor.state.doc.firstChild!.nodeSize - 1;
  editor.commands.insertContentAt(end, text);
};

afterEach(() => editor.destroy());

describe('replaceFromOutside', () => {
  // The whole point. A change synced in from another device is not something
  // this person did, so Cmd+Z must undo their typing and leave it alone.
  it('undoes what was typed and keeps what arrived', () => {
    make('<p>mine</p><p>theirs</p>');
    typeInFirstParagraph(' typed');

    replaceFromOutside(editor, '<p>mine typed</p><p>theirs, edited on the phone</p>');
    editor.commands.undo();

    expect(editor.getHTML()).toBe('<p>mine</p><p>theirs, edited on the phone</p>');
  });

  // A restore or a file rewritten on disk replaces everything. Undo must not
  // quietly put the old text back for the autosave to write over it.
  it('cannot be undone even when it replaces everything', () => {
    make('<p>the note as it was</p>');

    replaceFromOutside(editor, '<p>the version put back</p>');
    editor.commands.undo();

    expect(editor.getHTML()).toBe('<p>the version put back</p>');
  });

  it('leaves the cursor where it was when the change is elsewhere', () => {
    make('<p>mine</p><p>theirs</p>');
    editor.commands.setTextSelection(3);

    replaceFromOutside(editor, '<p>mine</p><p>theirs, and more of it</p>');

    expect(editor.state.selection.from).toBe(3);
  });

  it('does nothing at all when nothing changed', () => {
    make('<p>same</p>');
    const onUpdate = vi.fn();
    editor.on('update', onUpdate);

    replaceFromOutside(editor, '<p>same</p>');

    expect(onUpdate).not.toHaveBeenCalled();
  });

  // Where a run repeats, the scans from each end overlap. The result still has
  // to be exactly the new document.
  it('lands on exactly the new document when letters repeat', () => {
    for (const [from, to] of [['aab', 'ab'], ['ab', 'aab'], ['abba', 'aba'], ['x', 'xxxx']]) {
      make(`<p>${from}</p>`);
      replaceFromOutside(editor, `<p>${to}</p>`);
      expect(editor.getHTML()).toBe(`<p>${to}</p>`);
      editor.destroy();
    }
    make('<p></p>');
  });
});
