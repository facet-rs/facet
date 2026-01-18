import { defineConfig } from "vite";
import { resolve } from "path";

export default defineConfig({
  resolve: {
    alias: {
      "@bearcove/codemirror-lang-styx": resolve(
        __dirname,
        "../editors/codemirror-styx/src/index.ts",
      ),
    },
  },
  build: {
    manifest: true,
    rollupOptions: {
      input: {
        monaco: resolve(__dirname, "src/monaco/main.ts"),
        codemirror: resolve(__dirname, "src/codemirror/main.ts"),
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
