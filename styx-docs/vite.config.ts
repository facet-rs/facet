import { defineConfig } from "vite";
import { resolve } from "path";
import { svelte } from "@sveltejs/vite-plugin-svelte";
import wasm from "vite-plugin-wasm";
import topLevelAwait from "vite-plugin-top-level-await";

export default defineConfig({
  plugins: [svelte(), wasm(), topLevelAwait()],
  resolve: {
    alias: {
      "@bearcove/codemirror-lang-styx": resolve(
        __dirname,
        "../editors/codemirror-styx/src/index.ts",
      ),
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
        monaco: resolve(__dirname, "src/monaco/main.ts"),
        codemirror: resolve(__dirname, "src/codemirror/main.ts"),
        quiz: resolve(__dirname, "src/quiz/main.ts"),
      },
      output: {
        entryFileNames: "[name].js",
        chunkFileNames: "chunks/[name]-[hash].js",
        assetFileNames: "assets/[name][extname]",
      },
      // Preserve all exports from entry points - they're imported dynamically by HTML templates
      preserveEntrySignatures: "exports-only",
    },
    outDir: "dist",
    emptyOutDir: true,
  },
});
