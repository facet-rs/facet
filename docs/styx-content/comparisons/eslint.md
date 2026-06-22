+++
title = "ESLint"
weight = 8
slug = "eslint"
insert_anchor_links = "heading"
+++

An ESLint flat config in JavaScript vs Styx.

```compare
/// javascript
import eslint from '@eslint/js';
import tseslint from 'typescript-eslint';
import reactPlugin from 'eslint-plugin-react';
import prettierConfig from 'eslint-config-prettier';

export default [
  eslint.configs.recommended,
  ...tseslint.configs.strictTypeChecked,
  {
    files: ['**/*.{ts,tsx}'],
    plugins: {
      react: reactPlugin,
    },
    languageOptions: {
      parserOptions: {
        project: true,
        ecmaFeatures: {
          jsx: true,
        },
      },
    },
    rules: {
      '@typescript-eslint/no-unused-vars': ['error', {
        argsIgnorePattern: '^_',
        varsIgnorePattern: '^_',
      }],
      '@typescript-eslint/consistent-type-imports': 'error',
      '@typescript-eslint/no-floating-promises': 'error',
      'react/jsx-uses-react': 'off',
      'react/react-in-jsx-scope': 'off',
      'react/prop-types': 'off',
    },
  },
  {
    ignores: ['dist/', 'node_modules/', '*.config.js'],
  },
  prettierConfig,
];
/// styx
extends (
  "@eslint/js:recommended"
  typescript-eslint:strictTypeChecked
)

config {
  files "**/*.{ts,tsx}"
  plugins react>eslint-plugin-react

  languageOptions parserOptions>{
    project true
    ecmaFeatures jsx>true
  }

  rules {
    "@typescript-eslint/no-unused-vars" (error {
      argsIgnorePattern "^_"
      varsIgnorePattern "^_"
    })
    "@typescript-eslint/consistent-type-imports" error
    "@typescript-eslint/no-floating-promises" error
    react/jsx-uses-react off
    react/react-in-jsx-scope off
    react/prop-types off
  }
}

ignores (dist/ node_modules/ "*.config.js")

extends eslint-config-prettier
```
