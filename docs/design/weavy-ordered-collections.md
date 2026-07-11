# Verified persistent ordered collections

This document is the admission checkpoint for Vix `Map<K, V>` and `Set<T>`.
It deliberately replaces neither Vix's store nor Weavy's value-memory tables.

## Representation

An ordered collection handle names an immutable node in the task's molten
arena.  A node is either an empty leaf or a fixed-fanout B-tree page.  Pages
hold complete, declared key/value (or element) regions and child handles.
Every update allocates only the copied search spine and any split pages; all
unchanged children are shared.  A page with a unique molten owner may be
filled in place before it becomes reachable, but a reachable page is never
mutated.

The canonical value is the in-order sequence of complete entries under the
declared structural key comparator.  This makes identity and iteration
independent of insertion order.  `Set<T>` is the same page format without a
value region.

## Contract

`ProgramContract` will declare one ordered-collection schema with:

- the collection kind (`Map` or `Set`), key and optional value schemas;
- exact inline shapes for a complete key, value, and map row;
- a closed structural comparator witness for the key/element shape;
- fixed B-tree fanout and the result-status ABI.

The verifier admits collection operations only when handle schemas, operand
regions, result regions, and comparator witnesses agree exactly.  A comparator
is program-local verified bytecode/shape metadata, never a host callback or
raw-handle comparison.

## Operations and status

The substrate vocabulary is `OrderedEmpty`, `OrderedProbe`, `OrderedInsert`,
`OrderedUnion`, and `OrderedIter`.  Probe exposes a key plus child/result
handle; it never exposes a map value unless an explicit get/iteration-value
operation requests it.  Insert distinguishes `Insert`, `Replace`, and
`Duplicate`; union reports the first overlapping key in in-order traversal.
Statuses are converted by Vix lowering into `MissingKey` and `DuplicateKey`
at the originating source trace site.  Machine malformed-handle/schema/status
conditions remain typed machine faults.

## Costs

Search, get, has, insert, replace, and a disjoint-union merge are
`O(log n)` page work plus copied spine/split pages.  Persistent descendants
share every unchanged page with their source.  No operation may materialize a
whole-map dense array or copy all rows for a single insert.  `has` is key-only:
it does not read, project, or require residency of a stored value.

## Proof obligations

Weavy tests must establish insertion-order-independent in-order iteration,
identity, replacement/duplicate distinction, deterministic union conflict,
persistence, multiword structural keys/values, key-only has, interpreter/JIT
agreement, and an allocation/copy scaling oracle.  Vix lowering can only
claim the language rules after those substrate obligations and the production
plain/chaos/disabled-JIT certificates are green.
