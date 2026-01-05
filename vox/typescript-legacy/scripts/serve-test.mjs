#!/usr/bin/env node

/**
 * Simple static file server for browser tests.
 */

import { createServer } from "node:http";
import { readFile } from "node:fs/promises";
import { extname, join } from "node:path";
import { fileURLToPath } from "node:url";

const __dirname = fileURLToPath(new URL(".", import.meta.url));
const projectRoot = join(__dirname, "..");

const MIME_TYPES = {
  ".html": "text/html",
  ".js": "application/javascript",
  ".mjs": "application/javascript",
  ".css": "text/css",
  ".json": "application/json",
  ".map": "application/json",
};

const PORT = process.env.PORT || 3000;

const server = createServer(async (req, res) => {
  let filePath = join(projectRoot, req.url === "/" ? "/tests/browser/test-page.html" : req.url);

  try {
    const content = await readFile(filePath);
    const ext = extname(filePath);
    const contentType = MIME_TYPES[ext] || "application/octet-stream";

    res.writeHead(200, { "Content-Type": contentType });
    res.end(content);
  } catch (error) {
    if (error.code === "ENOENT") {
      res.writeHead(404);
      res.end("Not Found");
    } else {
      res.writeHead(500);
      res.end("Internal Server Error");
    }
  }
});

server.listen(PORT, () => {
  console.log(`Test server running at http://127.0.0.1:${PORT}`);
});
