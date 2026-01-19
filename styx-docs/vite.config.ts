import { defineConfig } from "vite";
import { resolve } from "path";
import { svelte } from "@sveltejs/vite-plugin-svelte";

export default defineConfig({
  plugins: [svelte()],
  resolve: {
    alias: {
      "@bearcove/codemirror-lang-styx": resolve(
        __dirname,
        "../editors/codemirror-styx/src/index.ts",
      ),
      "@bearcove/styx": resolve(__dirname, "../implementations/styx-js/src/index.ts"),
    },
  },
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
