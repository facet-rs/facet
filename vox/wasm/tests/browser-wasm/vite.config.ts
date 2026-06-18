import { defineConfig } from 'vite';
import wasm from 'vite-plugin-wasm';

export default defineConfig({
  plugins: [wasm()],
  build: {
    target: 'esnext',
  },
  server: {
    port: 3001,
  },
});
