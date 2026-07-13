# Vix Checker Corpus

Each `.json` file is a facet-json `CorpusEntry` consumed by
`vix/tests/checker_corpus.rs`.

The schema is intentionally small and stable:

- `schema_version`: currently `1`
- `id`: stable corpus id
- `outcome`: `accept` or `reject`
- `description`: review text
- `sources`: source files plus module paths
- `assertions`: absolute checker facts

Assertion spans are half-open byte offsets in the referenced source. Accept
cases use `0..0` when the fact is program-level. Reject cases pin the primary
diagnostic span.

The active test verifies that the corpus is real facet-json and that all source
files exist. The checker execution test is ignored until the Vix-written checker
binary exists.
