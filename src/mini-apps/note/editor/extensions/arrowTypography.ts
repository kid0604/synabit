import { Extension, textInputRule } from '@tiptap/core';
import Blockquote from '@tiptap/extension-blockquote';

// --- Arrow Typography Extension ---
export const ArrowExtension = Extension.create({
  name: 'arrows',
  addInputRules() {
    return [
      textInputRule({
        find: /->$/,
        replace: '→',
      }),
      textInputRule({
        find: /<-$/,
        replace: '←',
      }),
      textInputRule({
        find: /←>$/,
        replace: '↔',
      }),
      // Support direct <-> typing without individual triggering just in case
      textInputRule({
        find: /<->$/,
        replace: '↔',
      }),
    ];
  },
});

// --- Custom Blockquote to remove "> " shortcut ---
export const CustomBlockquote = Blockquote.extend({
  addInputRules() {
    return [];
  }
});
