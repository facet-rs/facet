import { defineConfig } from 'vite';
import path from 'node:path';

export default defineConfig({
  resolve: {
    alias: {
      '@bearcove/roam-core': path.resolve(__dirname, '../../packages/roam-core/src'),
      '@bearcove/roam-ws': path.resolve(__dirname, '../../packages/roam-ws/src'),
      '@bearcove/roam-generated': path.resolve(__dirname, '../../generated'),
    },
  },
  build: {
    target: 'esnext',
  },
  server: {
    port: 3000,
  },
});
