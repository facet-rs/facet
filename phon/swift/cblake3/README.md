# CBlake3

Vendored portable C BLAKE3 (from the official `BLAKE3-team/BLAKE3` `c/` sources,
as shipped in the `blake3` Rust crate v1.8.5). Only the portable path is built
(`-DBLAKE3_USE_NEON=0`, `-DBLAKE3_NO_SSE2`, `-DBLAKE3_NO_SSE41`,
`-DBLAKE3_NO_AVX2`, `-DBLAKE3_NO_AVX512`); the x86 SIMD sources are omitted.
Used by `PhonSchema` for content-hash schema identity so
Swift computes the same `SchemaId` as the Rust and TypeScript implementations.

BLAKE3 is dual-licensed CC0-1.0 / Apache-2.0-with-LLVM-exception.
