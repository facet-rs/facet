# Changelog

The format of this changelog is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/).

## [Unreleased]

## [0.4.0] - 2023-05-26

- BREAKING: Providing a custom override for the borrowed form name has changed from `ref` to
  `ref_name`, as `syn` now requires that it be a valid path (and `ref` is a keyword)
- Upgraded `syn` dependency to v2

## [0.3.1] - 2022-11-02

### Fixed

- Normalized braids with custom inner types would cause compiler errors on trying to convert from
  `Cow<str>`.

## [0.3.0] - 2022-11-02

### Added

- `from_infallible!()` utility macro added for providing universal `From<Infallible>` impls
- Added an example using `CompactString` from the `compact_str` crate ([#15])

### Changed

- `String` at the end of a braid name will now be shortened to `Str` in the borrowed form ([#19])
- `Validator::Error` is now expected to implement `From<Infallible>` to allow potentially
  fallible wrapped type conversions

[#15]: https://github.com/neoeinstein/aliri_braid/pull/15
[#19]: https://github.com/neoeinstein/aliri_braid/pull/19

## [0.2.4] - 2022-07-21

### Fixed

- Removed unnecessary lifetime annotations that cause clippy warnings in 1.62 ([#20])

[#20]: https://github.com/neoeinstein/aliri_braid/pull/20

## [0.2.3] - 2022-06-15

### Changed

- Added allow for clippy lint on automatically derived `serde::Deserialize` impls

<!--markdownlint-disable-file MD024 -->
