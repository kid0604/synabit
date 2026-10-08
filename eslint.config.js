import pluginVue from 'eslint-plugin-vue'
import vueTsEslintConfig from '@vue/eslint-config-typescript'
import unusedImports from 'eslint-plugin-unused-imports'

export default [
  ...pluginVue.configs['flat/essential'],
  ...vueTsEslintConfig(),
  {
    plugins: {
      'unused-imports': unusedImports,
    },
    rules: {
      'no-console': process.env.NODE_ENV === 'production' ? 'warn' : 'off',
      'no-debugger': process.env.NODE_ENV === 'production' ? 'warn' : 'off',
      '@typescript-eslint/no-unused-vars': 'off',
      '@typescript-eslint/no-explicit-any': 'off',
      '@typescript-eslint/no-empty-object-type': 'off',
      'unused-imports/no-unused-imports': 'error',
      'unused-imports/no-unused-vars': [
        'warn',
        {
          'vars': 'all',
          'varsIgnorePattern': '^_',
          'args': 'after-used',
          'argsIgnorePattern': '^_',
          // `_` already means "deliberately unused" for vars and args here, and
          // the codebase writes `catch (_)` in the same spirit — but without
          // this the convention was honoured everywhere except the one place it
          // was written down most explicitly.
          'caughtErrorsIgnorePattern': '^_',
          // `const { groupId, ...rest } = node.data` is how you drop a key in
          // JavaScript: the binding exists so that `rest` does not contain it,
          // and reading it would defeat the point. Without this the only way to
          // silence the warning is to delete the very line doing the work.
          'ignoreRestSiblings': true
        }
      ],
      'vue/multi-word-component-names': 'off',
      'vue/no-reserved-component-names': 'off',
      'vue/no-mutating-props': 'off',
      'vue/no-parsing-error': 'off',
      'vue/valid-v-on': 'off',
      'vue/valid-v-for': 'off',
      '@typescript-eslint/ban-ts-comment': 'off',

      // The UI/UX review's floor, kept by the linter rather than by memory.
      // Text under 12px and gray-400 text (2.5:1 on white) were the two things
      // older readers could not see; they had crept into 150 files one class
      // at a time.
      'vue/no-restricted-class': ['error', '/^text-\\[(?:[0-9]|1[01])px\\]$/', 'text-gray-400'],
      // `aria-label="Show Settings Modal = false"` was a click handler turned
      // into words by a tool. A screen reader reads it out loud.
      'vue/no-restricted-static-attribute': ['error', {
        key: 'aria-label',
        value: '/=|\\.value\\b|^Handle /',
        message: 'This aria-label reads like code. Describe what the control does, through $t().',
      }],
      // One way to ask before something irreversible: ConfirmModal, or better,
      // an undo (useUndoableAction). The browser's and the OS's dialogs look
      // like neither the app nor each other.
      'no-restricted-globals': ['error',
        { name: 'confirm', message: 'Use ConfirmModal, or useUndoableAction for deletes.' },
        { name: 'alert', message: 'Show the message in the app.' },
      ],
      'no-restricted-properties': ['error',
        { object: 'window', property: 'confirm', message: 'Use ConfirmModal, or useUndoableAction for deletes.' },
        { object: 'window', property: 'alert', message: 'Show the message in the app.' },
      ],
      'no-restricted-syntax': ['error',
        // A Tauri command rejects with `{ code, message }`, and `String()` of
        // that is "[object Object]" — which is what a dozen error lines showed.
        {
          selector: "CallExpression[callee.name='String'][arguments.length=1][arguments.0.type='Identifier'][arguments.0.name=/^(e|err|error)$/]",
          message: 'String(e) of a command error is "[object Object]". Use errorText(e) from src/shared/errorText.ts, or pass e to the logger as it is.',
        },
        // The UTC date, not the user's: before 07:00 in Vietnam it is yesterday.
        {
          selector: "CallExpression[callee.property.name=/^(slice|substring|substr)$/][callee.object.type='CallExpression'][callee.object.callee.property.name='toISOString'][arguments.0.value=0][arguments.1.value=10]",
          message: "toISOString() is UTC, so this is yesterday's date in the early morning east of Greenwich. Use todayIso() / localDateKey() from src/shared/localDay.ts for a calendar day.",
        },
        {
          selector: "MemberExpression[property.value=0][object.type='CallExpression'][object.callee.property.name='split'][object.arguments.0.value='T'][object.callee.object.type='CallExpression'][object.callee.object.callee.property.name='toISOString']",
          message: "toISOString() is UTC, so this is yesterday's date in the early morning east of Greenwich. Use todayIso() / localDateKey() from src/shared/localDay.ts for a calendar day.",
        },
      ],
      'no-restricted-imports': ['error', {
        paths: [{
          name: '@tauri-apps/plugin-dialog',
          importNames: ['ask', 'confirm'],
          message: 'Use ConfirmModal, or useUndoableAction for deletes.',
        }],
      }],
    }
  },
  {
    // Tests mock the dialog plugin wholesale to prove it is no longer called,
    // and build UTC dates on purpose to pin down what the code does with them.
    files: ['**/__tests__/**', '**/*.spec.ts'],
    rules: { 'no-restricted-imports': 'off', 'no-restricted-syntax': 'off' },
  },
  {
    // The two places that turn an error into words are where `String(error)`
    // is the right last resort, after everything better has been tried.
    files: ['src/shared/errorText.ts', 'src/utils/said.ts'],
    rules: { 'no-restricted-syntax': 'off' },
  }
]
