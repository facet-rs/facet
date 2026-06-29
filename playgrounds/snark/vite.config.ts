import { defineConfig } from "vite";

export default defineConfig({
  server: {
    fs: {
      allow: ["../.."],
    },
  },
  build: {
    target: "esnext",
  },
});
