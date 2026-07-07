# rmeta span-noise spike

## Environment and commands

- Repo: facet monorepo worktree at `/Users/amos/.paseo/worktrees/1t3lgrd0/spike-rmeta-hash`.
- Stable toolchain: `rustc 1.96.0 (ac68faa20 2026-05-25)`, `aarch64-apple-darwin`.
- Local nightly used only for flag comparison: `rustc 1.98.0-nightly (423e3d252 2026-05-24)`.
- Stable `rustc -Z help` fails because the pinned 1.96.0 toolchain is stable.
- Main run:

```sh
cargo run --manifest-path spikes/rmeta-hash/Cargo.toml -- measure
```

- Prototype projection examples:

```sh
cargo run --manifest-path spikes/rmeta-hash/Cargo.toml -- project \
  spikes/rmeta-hash/out/rmeta/stable-default/base.rmeta \
  spikes/rmeta-hash/data/stable-default-projection-mask.txt \
  spikes/rmeta-hash/out/rmeta/stable-default/top_comment.rmeta

cargo run --manifest-path spikes/rmeta-hash/Cargo.toml -- project \
  spikes/rmeta-hash/out/rmeta/stable-default/base.rmeta \
  spikes/rmeta-hash/data/stable-default-projection-mask.txt \
  spikes/rmeta-hash/out/rmeta/stable-default/pub_signature.rmeta
```

The durable data is in `spikes/rmeta-hash/data/`:

- `noise-matrix.tsv`: byte-level change matrix, lengths, ranges, and coarse regions.
- `projection-matrix.tsv`: whole-file hash versus projection hash behavior.
- `diff-regions.tsv`: every equal-offset diff range with top-level region labels.
- `*-projection-mask.txt`: baseline keep-ranges used by the prototype hasher.
- `summary.md`: base artifact sizes and visible ASCII inventory.

## Format grounding

The Rust compiler dev guide describes `.rmeta` as rustc-specific metadata created by `--emit=metadata`; it is enough for `cargo check`, docs, and pipelining, but it is not linkable object code: <https://rustc-dev-guide.rust-lang.org/backend/libs-and-metadata.html>.

The relevant public rustdoc for `rustc_metadata::rmeta` says the module has `encoder`, `decoder`, `table`, lazy values/tables, `CrateRoot`, and `SpanTag`: <https://doc.rust-lang.org/beta/nightly-rustc/rustc_metadata/rmeta/index.html>.

The source docs show the only top-level structure this spike parses directly: `METADATA_HEADER` is `rust\0\0\0<METADATA_VERSION>`, followed by the root position, and metadata is a post-order lazy tree with the root position written next to the header: <https://doc.rust-lang.org/beta/nightly-rustc/src/rustc_metadata/rmeta/mod.rs.html>.

`CrateRoot` carries both interface-relevant and noise-bearing material: `crate_deps`, `traits`, `impls`, exported symbols, `tables`, `source_map`, `syntax_contexts`, `expn_data`, `expn_hashes`, and `def_path_hash_map`: <https://doc.rust-lang.org/beta/nightly-rustc/rustc_metadata/rmeta/struct.CrateRoot.html>.

The encoder source shows that spans are encoded with shorthand/backreferences and source-file indexes, and that source files are serialized when spans require them: <https://doc.rust-lang.org/beta/nightly-rustc/src/rustc_metadata/rmeta/encoder.rs.html>.

This spike labels only these top-level regions without linking against rustc internals:

- `metadata_header`: bytes `0..8`.
- `crate_root_pointer`: bytes `8..16`.
- `lazy_metadata_payload`: bytes `16..crate_root_pos`.
- `crate_root_and_table_directory`: bytes `crate_root_pos..len`.

That is enough to show where noise lands, but not enough to name every lazy table entry precisely.

## Noise Matrix

Stable default, base `.rmeta` length `4122`, root position `3744`:

| edit | changed? | len | coarse changed regions |
| --- | --- | ---: | --- |
| top comment | yes | 4144 | prefix only `0..8`; replacement `8..4099`; 194 lazy-payload ranges, 30 crate-root/table ranges |
| mid-file whitespace | yes | 4145 | prefix only `0..8`; replacement `8..4099`; 35 lazy-payload ranges, 31 crate-root/table ranges |
| private non-generic fn body | yes | 4138 | prefix only `0..8`; replacement `8..4099`; 14 lazy-payload ranges, 34 crate-root/table ranges |
| public fn signature | yes | 4140 | prefix only `0..8`; replacement `8..4099`; 15 lazy-payload ranges, 36 crate-root/table ranges |
| generic `#[inline]` body | yes | 4152 | prefix only `0..8`; replacement `8..4099`; 14 lazy-payload ranges, 34 crate-root/table ranges |

`--remap-path-prefix` shrinks the stable base `.rmeta` from `4122` to `3946`, so path strings are real payload. It does not make any edit byte-identical: the same five edits still change bytes starting at the root pointer and spanning lazy payload plus crate-root/table directory.

Nightly flag sweep:

| plan | base len | result |
| --- | ---: | --- |
| nightly default | 4131 | all five edits changed `.rmeta` |
| `-Z incremental-ignore-spans=yes` | 4171 | all five edits changed `.rmeta` |
| `-Z remap-cwd-prefix=/rmeta-spike` | 4027 | all five edits changed `.rmeta` |
| `-Z location-detail=none` | 4163 | all five edits changed `.rmeta` |
| `-Z span-free-formats=yes` | 4155 | all five edits changed `.rmeta` |

So the whole-file hash is polluted by span/source-map/table-offset/SVH noise, and no tested stable or local nightly flag combination produces span-stable `.rmeta`.

## Projection Prototype

The prototype lives in `spikes/rmeta-hash/src/main.rs`. It does not attempt to decode rustc private tables. Instead it uses the fixture matrix as an oracle:

1. Compile the baseline and all variants to `.rmeta`.
2. For each intended-stable edit (`top_comment`, `mid_whitespace`, `private_body`), align the baseline bytes to the variant bytes with LCS.
3. Keep only baseline byte positions that survive all intended-stable alignments. Also drop the root pointer at `8..16`.
4. For a candidate `.rmeta`, align it to the baseline and hash only the kept positions.

This is a baseline-trained semantic projection. It is useful for the spike because it defeats length-changing span/source-map noise without decoding the file, but it is not a general rustc metadata decoder.

Projection results:

| plan | kept bytes | top comment | whitespace | private body | pub signature | inline/generic body |
| --- | ---: | --- | --- | --- | --- | --- |
| stable default | 3872/4122 | stable | stable | stable | changed | changed |
| stable remap | 3696/3946 | stable | stable | stable | changed | changed |
| nightly default | 3878/4131 | stable | stable | stable | changed | changed |
| nightly `incremental-ignore-spans` | 3918/4171 | stable | stable | stable | changed | changed |
| nightly `remap-cwd-prefix` | 3774/4027 | stable | stable | stable | changed | changed |
| nightly `location-detail=none` | 3910/4163 | stable | stable | stable | changed | changed |
| nightly `span-free-formats` | 3902/4155 | stable | stable | stable | changed | changed |

Stable default hashes from `projection-matrix.tsv`:

| variant | whole hash | projection hash |
| --- | --- | --- |
| top comment | `b10298fc8b2b858b` | `d694d902072ead6c` |
| mid whitespace | `5403ddc479d70237` | `d694d902072ead6c` |
| private body | `01e5bd40c4a3ee61` | `d694d902072ead6c` |
| public signature | `01906272cb2516d6` | `ecfae20b534395bb` |
| inline/generic body | `3b1580771efa7d4e` | `62ca24c88d7642de` |

The prototype therefore has zero false positives and zero false negatives on this matrix. Its risk is generality: an unseen semantic change could live outside the kept projection, or an unseen non-semantic noise source could disturb kept bytes.

## Verdict

Naive whole-file `.rmeta` hashing cannot support RDR on unpatched rustc 1.96.0. Even comment insertion and whitespace-only edits change the file, and the change starts at the root pointer and propagates through lazy payload and crate-root/table metadata.

The LCS projection proves that there is stable semantic signal inside `.rmeta`: on this fixture it ignores span/source-map/private-body noise and still catches public signature plus generic/inline body changes.

But vix should not ship a general "relink, do not rebuild" cutoff on unpatched rustc 1.96 using only external byte surgery. The format is rustc-private, version-locked, and does not expose stable section boundaries. A shippable design needs one of:

- a rustc-side semantic metadata hash/projection,
- an exact-version rustc metadata decoder used as an implementation detail, with conservative rebuild fallback,
- or an experimental oracle-gated mode that treats this projection as advisory and never as proof for broad dependency cutoffs.

What breaks the external approach:

- source-map and span encodings move under comment/whitespace edits,
- the root pointer and lazy positions shift when metadata length changes,
- crate hash/SVH-like data changes for body edits even when downstream type interface does not,
- generic/inline bodies must remain semantically visible, so blanket span/MIR stripping is unsafe,
- nightly `-Z` span-related flags tested here do not canonicalize `.rmeta`,
- a different rustc cannot decode 1.96 `.rmeta` (`-Z ls` on local nightly reports a version mismatch).

