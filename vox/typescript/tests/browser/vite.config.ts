import { defineConfig } from 'vite';
import path from 'node:path';

export default defineConfig({
  resolve: {
    alias: {
      '@bearcove/vox-core': path.resolve(__dirname, '../../packages/vox-core/src'),
      '@bearcove/vox-ws': path.resolve(__dirname, '../../packages/vox-ws/src'),
      '@bearcove/vox-generated': path.resolve(__dirname, '../../generated'),
    },
  },
  build: {
    target: 'esnext',
  },
  server: {
    port: 3000,
  },
});
