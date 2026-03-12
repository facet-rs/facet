# facet-csv

<!-- cargo-reedme: start -->

<!-- cargo-reedme: info-start

    Do not edit this region by hand
    ===============================

    This region was generated from Rust documentation comments by `cargo-reedme` using this command:

        cargo +nightly reedme --workspace

    for more info: https://github.com/nik-rev/cargo-reedme

cargo-reedme: info-end -->

CSV parser and serializer using facet-format.

**Note:** CSV is a fundamentally different format from JSON/XML/YAML.
While those formats are tree-structured and map naturally to nested types,
CSV is a flat, row-based format where each row represents a single record
and each column represents a field.

This crate provides basic CSV support via the `FormatParser` trait, but
has significant limitations:

- No support for nested structures (CSV is inherently flat)
- No support for arrays/sequences as field values
- No support for enums beyond unit variants (encoded as strings)
- All values are strings and must be parseable to target types

For more sophisticated CSV handling, consider a dedicated CSV library.

<!-- cargo-reedme: end -->
