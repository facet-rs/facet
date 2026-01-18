+++
title = "package.json"
weight = 5
slug = "package-json"
insert_anchor_links = "heading"
+++

An npm package.json in JSON vs Styx.

```compare
/// json
{
  "name": "@myorg/webapp",
  "version": "2.1.0",
  "description": "A modern web application",
  "main": "dist/index.js",
  "types": "dist/index.d.ts",
  "type": "module",
  "scripts": {
    "dev": "vite",
    "build": "tsc && vite build",
    "preview": "vite preview",
    "test": "vitest",
    "test:coverage": "vitest --coverage",
    "lint": "eslint src --ext .ts,.tsx",
    "format": "prettier --write src"
  },
  "dependencies": {
    "react": "^18.2.0",
    "react-dom": "^18.2.0",
    "react-router-dom": "^6.20.0",
    "@tanstack/react-query": "^5.0.0",
    "zod": "^3.22.0"
  },
  "devDependencies": {
    "@types/react": "^18.2.0",
    "@types/react-dom": "^18.2.0",
    "typescript": "^5.3.0",
    "vite": "^5.0.0",
    "vitest": "^1.0.0",
    "eslint": "^8.55.0",
    "prettier": "^3.1.0"
  },
  "peerDependencies": {
    "react": ">=18.0.0"
  },
  "engines": {
    "node": ">=20.0.0"
  },
  "repository": {
    "type": "git",
    "url": "https://github.com/myorg/webapp"
  },
  "keywords": ["react", "typescript", "webapp"],
  "author": "My Org <dev@myorg.com>",
  "license": "MIT"
}
/// styx
name "@myorg/webapp"
version 2.1.0
description "A modern web application"
main dist/index.js
types dist/index.d.ts
type module

scripts {
  dev vite
  build "tsc && vite build"
  preview "vite preview"
  test vitest
  test:coverage "vitest --coverage"
  lint "eslint src --ext .ts,.tsx"
  format "prettier --write src"
}

dependencies {
  react ^18.2.0
  react-dom ^18.2.0
  react-router-dom ^6.20.0
  "@tanstack/react-query" ^5.0.0
  zod ^3.22.0
}

devDependencies {
  "@types/react" ^18.2.0
  "@types/react-dom" ^18.2.0
  typescript ^5.3.0
  vite ^5.0.0
  vitest ^1.0.0
  eslint ^8.55.0
  prettier ^3.1.0
}

peerDependencies react>>=18.0.0
engines node>>=20.0.0

repository type>git url>https://github.com/myorg/webapp
keywords (react typescript webapp)
author "My Org <dev@myorg.com>"
license MIT
```
