# facet-dessert

<!-- cargo-reedme: start -->

<!-- cargo-reedme: info-start

    Do not edit this region by hand
    ===============================

    This region was generated from Rust documentation comments by `cargo-reedme` using this command:

        cargo +nightly reedme --workspace

    for more info: https://github.com/nik-rev/cargo-reedme

cargo-reedme: info-end -->

Sweet helpers for facet deserialization.

This crate provides common setter functions for handling string, bytes, and scalar values
when deserializing into facet types. It’s used by both `facet-format` and `facet-dom`.

By extracting these functions into a non-generic crate, we reduce monomorphization bloat
in format deserializers. See <https://github.com/bearcove/facet/issues/1924> for details.

<!-- cargo-reedme: end -->
