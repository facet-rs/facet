+++
title = "JavaScript / TypeScript"
weight = 4
slug = "javascript"
insert_anchor_links = "heading"
+++

Native TypeScript implementation with full type definitions.

## Installation

```bash
npm install @aspect/styx
```

## Usage

```typescript
import { parse } from '@aspect/styx';

const doc = parse('name "Alice"\nage 30');
```

## Source

[implementations/styx-js](https://github.com/bearcove/styx/tree/main/implementations/styx-js)
