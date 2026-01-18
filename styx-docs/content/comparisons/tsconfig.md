+++
title = "tsconfig.json"
weight = 6
slug = "tsconfig"
insert_anchor_links = "heading"
+++

A TypeScript configuration in JSON vs Styx.

```compare
/// json
{
  "compilerOptions": {
    "target": "ES2022",
    "lib": ["ES2022", "DOM", "DOM.Iterable"],
    "module": "ESNext",
    "moduleResolution": "bundler",
    "strict": true,
    "noUnusedLocals": true,
    "noUnusedParameters": true,
    "noFallthroughCasesInSwitch": true,
    "exactOptionalPropertyTypes": true,
    "noUncheckedIndexedAccess": true,
    "skipLibCheck": true,
    "declaration": true,
    "declarationMap": true,
    "sourceMap": true,
    "outDir": "./dist",
    "rootDir": "./src",
    "baseUrl": ".",
    "paths": {
      "@/*": ["./src/*"],
      "@components/*": ["./src/components/*"],
      "@utils/*": ["./src/utils/*"]
    },
    "jsx": "react-jsx",
    "esModuleInterop": true,
    "resolveJsonModule": true,
    "isolatedModules": true
  },
  "include": ["src/**/*"],
  "exclude": ["node_modules", "dist", "**/*.test.ts"]
}
/// styx
compilerOptions {
  // Output
  target ES2022
  module ESNext
  lib (ES2022 DOM DOM.Iterable)
  outDir ./dist
  rootDir ./src

  // Module resolution
  moduleResolution bundler
  baseUrl .
  paths {
    "@/*" (./src/*)
    "@components/*" (./src/components/*)
    "@utils/*" (./src/utils/*)
  }

  // Strict checks
  strict true
  noUnusedLocals true
  noUnusedParameters true
  noFallthroughCasesInSwitch true
  exactOptionalPropertyTypes true
  noUncheckedIndexedAccess true

  // Emit
  declaration true
  declarationMap true
  sourceMap true
  skipLibCheck true

  // Interop
  jsx react-jsx
  esModuleInterop true
  resolveJsonModule true
  isolatedModules true
}

include (src/**/*)
exclude (node_modules dist **/*.test.ts)
```
