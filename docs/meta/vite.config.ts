import { defineConfig } from "vite";
import { resolve } from "path";

export default defineConfig({
  build: {
    outDir: "dist",
    lib: {
      entry: resolve(__dirname, "src/main.ts"),
      name: "FacetHljs",
      fileName: "facet-hljs",
      formats: ["iife"],
    },
    rollupOptions: {
      output: {
        // Ensure we get a single file with everything inlined
        inlineDynamicImports: true,
        // Don't add hash to filenames for predictable URLs
        entryFileNames: "facet-hljs.iife.js",
        assetFileNames: "[name][extname]",
      },
    },
    // Generate source maps for debugging
    sourcemap: true,
    // Minify for production (esbuild is the default in Vite 7)
    minify: true,
  },
  // Preview server config for serving rustdoc output
  preview: {
    port: 3000,
    open: "/hljs_test_crate/",
  },
});
