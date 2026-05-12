import { Extension } from '@tiptap/core';
import { Plugin, PluginKey } from '@tiptap/pm/state';
import { Decoration, DecorationSet } from '@tiptap/pm/view';

/**
 * BlockIdHider — ProseMirror decoration plugin that hides ^block-id markers
 * (e.g., " ^a1b2c3") from the editor's visual rendering while keeping them
 * in the underlying document data for Markdown serialization.
 */
export const BlockIdHider = Extension.create({
  name: 'blockIdHider',

  addProseMirrorPlugins() {
    return [
      new Plugin({
        key: new PluginKey('blockIdHider'),
        props: {
          decorations(state) {
            const decorations: Decoration[] = [];

            state.doc.descendants((node, pos) => {
              if (node.isText && node.text) {
                // Match " ^xxxxxx" at end of text (6-char alphanumeric)
                const match = node.text.match(/ \^[a-z0-9]{6}$/);
                if (match && match.index !== undefined) {
                  decorations.push(
                    Decoration.inline(
                      pos + match.index,
                      pos + match.index + match[0].length,
                      { style: 'display:none' }
                    )
                  );
                }
              }
            });

            return DecorationSet.create(state.doc, decorations);
          },
        },
      }),
    ];
  },
});
