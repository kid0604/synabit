import HardBreak from '@tiptap/extension-hard-break';

/**
 * A line break (Shift+Enter), saved where markdown has no line break of its
 * own as `<br>`.
 *
 * A heading is one line in markdown. A break inside one was written as `\` and
 * a new line, and what followed came back as a paragraph under a shorter
 * heading — found by the note property test. tiptap-markdown already writes
 * `<br>` in a table cell for the same reason; a heading is the other place.
 * Elsewhere, and with nothing after it, as tiptap-markdown does.
 */
export const LineBreak = HardBreak.extend({
  addStorage() {
    return {
      markdown: {
        serialize(state: any, node: any, parent: any, index: number) {
          for (let i = index + 1; i < parent.childCount; i++) {
            if (parent.child(i).type !== node.type) {
              state.write(state.inTable || parent.type.name === 'heading' ? '<br>' : '\\\n');
              return;
            }
          }
        },
        parse: {
          // markdown-it reads `\` breaks; `<br>` is read as HTML.
        },
      },
    };
  },
});
