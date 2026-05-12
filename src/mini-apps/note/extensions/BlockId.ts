import { Extension } from '@tiptap/core';
import { Plugin, PluginKey } from '@tiptap/pm/state';
import { Decoration, DecorationSet } from '@tiptap/pm/view';

export const BlockIdExtension = Extension.create({
  name: 'blockId',

  addProseMirrorPlugins() {
    return [
      new Plugin({
        key: new PluginKey('blockId'),
        props: {
          decorations(state) {
            const decorations: Decoration[] = [];
            const idRegex = /(?:\s+)\^([a-zA-Z0-9\-]+)\s*$/;

            state.doc.descendants((node, pos) => {
              if (node.isText) {
                const text = node.text || '';
                const match = idRegex.exec(text);
                if (match) {
                  const startPos = pos + match.index;
                  const endPos = pos + text.length;
                  decorations.push(
                    Decoration.inline(startPos, endPos, {
                      class: 'block-id-marker'
                    })
                  );
                }
              }
            });

            return DecorationSet.create(state.doc, decorations);
          }
        },
        appendTransaction(transactions, oldState, newState) {
          if (!transactions.some(tr => tr.docChanged)) {
            return null;
          }

          let tr = newState.tr;
          let modified = false;

          const selection = newState.selection;
          const activeFrom = selection.from;
          const activeTo = selection.to;

          // Regex to check if text ends with a block ID ` ^id`
          const idRegex = /(?:\s+)\^([a-zA-Z0-9\-]+)\s*$/;

          const insertions: { pos: number; text: string }[] = [];

          newState.doc.descendants((node, pos) => {
            if (node.type.name === 'paragraph' || node.type.name === 'heading') {
              // Only add ID if the node is NOT currently being edited
              // Meaning the cursor is not inside this node
              if (activeFrom >= pos && activeTo <= pos + node.nodeSize) {
                return;
              }

              const text = node.textContent;
              if (text.trim().length > 0 && !idRegex.test(text)) {
                // Generate a random 6-character ID
                const id = Math.random().toString(36).substring(2, 8);
                // Collect insertion position (end of the node's text)
                insertions.push({ pos: pos + node.nodeSize - 1, text: ` ^${id}` });
              }
            }
          });

          if (insertions.length > 0) {
            // Apply insertions in reverse order to prevent position shifting
            for (let i = insertions.length - 1; i >= 0; i--) {
              tr.insertText(insertions[i].text, insertions[i].pos);
            }
            modified = true;
          }

          return modified ? tr : null;
        }
      })
    ];
  }
});
