import { defineConfig } from 'vite';
import { resolve } from 'path';

export default defineConfig({
  resolve: {
    alias: {
      '@bearcove/codemirror-lang-styx': resolve(__dirname, '../../editors/codemirror-styx/src/index.ts'),
    },
  },
  build: {
    lib: {
      entry: resolve(__dirname, 'src/main.ts'),
      name: 'StyxPlayground',
      fileName: 'playground',
      formats: ['es'],
    },
    outDir: '../static/playground',
    emptyOutDir: true,
  },
});
