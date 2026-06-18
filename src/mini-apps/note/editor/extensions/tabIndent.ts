import { Extension } from '@tiptap/core';

export const TabIndentExtension = Extension.create({
  name: 'tabIndent',
  addKeyboardShortcuts() {
    return {
      Tab: () => {
        if (this.editor.commands.sinkListItem('listItem')) return true;
        if (this.editor.commands.sinkListItem('taskItem')) return true;
        // Fallback for regular paragraph (insert spaces), BUT NOT IN A TABLE
        if (this.editor.isActive('paragraph') && !this.editor.isActive('table')) {
          return this.editor.commands.insertContent('    ');
        }
        return false;
      },
      'Shift-Tab': () => {
        if (this.editor.commands.liftListItem('listItem')) return true;
        if (this.editor.commands.liftListItem('taskItem')) return true;
        return false;
      },
    };
  },
});
