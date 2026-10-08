import { ref } from 'vue';
import { createLowlight } from 'lowlight';
import type { Editor } from '@tiptap/core';

/**
 * One highlighter for every editor, its grammars fetched after the editor is up.
 *
 * highlight.js's common set is around 160 KB, and the editor is loaded by four
 * screens whether or not the note in front of them holds a line of code. The
 * highlighter starts with only the three languages below; the rest arrive with
 * `loadCodeGrammars`, and `rehighlight` repaints what was already on screen.
 * Until then a code block is plain monospace text — what it is anyway while
 * typing — and nothing about the document changes.
 */
export const lowlight = createLowlight();

/**
 * `mermaid`, `markmap` and `query` are ours, not highlight.js's — a diagram or
 * a saved query, rendered below the block rather than coloured inside it.
 *
 * Registering them as plain text is what keeps typing in them fast. The
 * lowlight plugin falls back to `highlightAuto` for any language it does not
 * know, which runs the block through every grammar it has; and it re-runs that
 * for *every* code block in the note on every keystroke made inside one. On a
 * note of five mermaid diagrams that measured 150ms per character, against
 * 5ms once the language is known — a note you could watch yourself type.
 *
 * Naming them here also puts them in the block's language dropdown, which
 * until now could not display the language the block was actually set to.
 */
for (const name of ['mermaid', 'markmap', 'query']) {
  // Written out rather than reusing highlight.js's own `plaintext`, which
  // carries the `text` and `txt` aliases with it and would hand them to
  // whichever of these three registered last.
  lowlight.register(name, () => ({ name, contains: [], disableAutodetect: true }));
}

/** Moves when the grammars land, so a language dropdown can list them. */
export const grammarsLoaded = ref(false);

let loading: Promise<void> | null = null;

export function loadCodeGrammars(): Promise<void> {
  loading ??= import('./codeGrammars')
    .then(({ common }) => {
      lowlight.register(common);
      grammarsLoaded.value = true;
    })
    .catch((err) => {
      loading = null;
      throw err;
    });
  return loading;
}

/**
 * Recompute the code decorations of an editor that was drawn before the
 * grammars arrived.
 *
 * The lowlight plugin only re-highlights on a transaction that changes a code
 * block, and editing the document to get one would mark the note changed and
 * put a step in its undo history. Taking the plugin out and putting the same
 * one back re-runs its `init`, which highlights the whole document, without a
 * transaction at all.
 */
export function rehighlight(editor: Editor): void {
  if (editor.isDestroyed) return;
  const plugins = editor.state.plugins;
  const index = plugins.findIndex(p => (p as unknown as { key: string }).key.startsWith('lowlight$'));
  if (index < 0) return;
  const plugin = plugins[index];
  editor.unregisterPlugin('lowlight');
  editor.registerPlugin(plugin, (p, rest) => [...rest.slice(0, index), p, ...rest.slice(index)]);
}
