import { defineConfig } from "vite";
import { resolve } from "path";
import { svelte } from "@sveltejs/vite-plugin-svelte";
import wasm from "vite-plugin-wasm";
import topLevelAwait from "vite-plugin-top-level-await";

export default defineConfig({
  plugins: [svelte(), wasm(), topLevelAwait()],
  resolve: {
    alias: {
      "@bearcove/styx": resolve(__dirname, "../implementations/styx-js/src/index.ts"),
      "@bearcove/styx-webmd": resolve(__dirname, "../tools/styx-webmd/dist/styx_webmd.js"),
    },
  },
  server: {
    fs: {
      allow: [".."],
    },
  },
  optimizeDeps: {
    exclude: ["@bearcove/styx-webmd"],
  },
  assetsInclude: ["**/*.wasm"],
  build: {
    manifest: true,
    rollupOptions: {
      input: {
        // Only quiz is bundled; Monaco and CodeMirror playgrounds load from esm.sh
        quiz: resolve(__dirname, "src/quiz/main.ts"),
      },
      output: {
        entryFileNames: "[name].js",
        chunkFileNames: "chunks/[name]-[hash].js",
        assetFileNames: "assets/[name][extname]",
      },
      preserveEntrySignatures: "exports-only",
    },
    outDir: "dist",
    emptyOutDir: true,
  },
});
