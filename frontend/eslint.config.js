import js from '@eslint/js';
import ts from 'typescript-eslint';
import svelte from 'eslint-plugin-svelte';
import prettier from 'eslint-config-prettier';
import globals from 'globals';

export default [
  js.configs.recommended,
  ...ts.configs.recommended,
  ...svelte.configs['flat/recommended'],
  prettier,
  {
    languageOptions: { globals: { ...globals.browser, ...globals.node } },
    rules: {
      '@typescript-eslint/no-unused-vars': [
        'error',
        { argsIgnorePattern: '^_', varsIgnorePattern: '^_' }
      ]
    }
  },
  {
    // `**/*.svelte` does not match a module named `foo.svelte.ts` — the name ends
    // in `.ts` — so `.svelte.ts` needs listing explicitly. Plugin v3 claims those
    // files itself (its `flat/base` config, `setup-for-svelte-script`) and gives
    // them bare `svelte-eslint-parser`, which cannot read TypeScript; without the
    // globs below, `import type { ... } from './types'` is a parsing error. v2
    // never claimed the files, which is why the gap only appeared with v3.
    files: ['**/*.svelte', '**/*.svelte.ts', '**/*.svelte.js'],
    languageOptions: { parserOptions: { parser: ts.parser } }
  },
  { ignores: ['build/', '.svelte-kit/', 'dist/', 'node_modules/'] }
];
