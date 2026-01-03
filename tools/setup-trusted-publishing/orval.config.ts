import { defineConfig } from "orval";

export default defineConfig({
  cratesio: {
    input: "/home/amos/crates-io-openapi.json",
    output: {
      target: "./generated/cratesio.ts",
      client: "fetch",
      mode: "single",
    },
  },
});
