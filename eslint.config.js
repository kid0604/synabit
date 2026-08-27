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
      '@typescript-eslint/ban-ts-comment': 'off'
    }
  }
]
