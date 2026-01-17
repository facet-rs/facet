import { defineConfig } from 'vite';
import { resolve } from 'path';

export default defineConfig({
  build: {
    lib: {
      entry: resolve(__dirname, 'src/main.ts'),
      name: 'StyxMonaco',
      fileName: 'monaco',
      formats: ['es'],
    },
    outDir: '../static/monaco',
    emptyOutDir: true,
  },
});
