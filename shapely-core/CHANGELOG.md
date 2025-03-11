# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

## [2.0.2](https://github.com/bearcove/shapely/compare/shapely-core-v2.0.1...shapely-core-v2.0.2) - 2025-03-11

### Other

- Remove unreachable wildcard pattern in ScalarContents::get_contents
- non exhaustive
- mh
- Add scalar contents tests and improvements

## [2.0.0](https://github.com/bearcove/shapely/compare/shapely-core-v1.0.0...shapely-core-v2.0.0) - 2025-03-11

### Other

- Stub out shapely-yaml
- Simplify Shape name function signature
- Clean up HashMap's name impl
- Change Shape.name from static str to NameFn
- Note on valid zst pointer values, closes #11
- Back to something working
- Clippy auto fix
- Add `addr` and `shape` methods to Partial and Slot
- Looking good!
- Refactor slot_by_name and move_into
- Refactor Partial struct initialization and field access
- more slot/field cleanups
- Clean up Partial API
- clean up drop impls
- more work on the field API
- Shape up field API
- Stability notes
- introduce slot_by_index / slot_by_name
- link to 'free of syn' campaign
- wip sloterrors
- Ensure no syn dependency (and badge about it), closes #9
