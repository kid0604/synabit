import { Extension } from '@tiptap/core';
import Suggestion from '@tiptap/suggestion';
import { PluginKey } from '@tiptap/pm/state';

export const EmojiSuggestion = Extension.create({
  name: 'emojiSuggestion',

  addOptions() {
    return {
      suggestion: {
        char: ':',
        pluginKey: new PluginKey('emojiSuggestion'),
        // Only activate after at least 2 chars typed (avoid false triggers on plain colons)
        allowSpaces: false,
        command: ({ editor, range, props }: any) => {
          editor.chain().focus().deleteRange(range).insertContent(props.emoji).run();
        },
      },
    };
  },

  addProseMirrorPlugins() {
    return [
      Suggestion({
        editor: this.editor,
        ...this.options.suggestion,
      }),
    ];
  },
});
