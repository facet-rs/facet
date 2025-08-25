# Changelog
All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

## [0.28.1](https://github.com/facet-rs/facet/compare/facet-reflect-v0.28.0...facet-reflect-v0.28.1) - 2025-08-25

### Other

- remove ko-fi
- Update sponsors.afdesign

## [0.28.0](https://github.com/facet-rs/facet/compare/facet-reflect-v0.27.15...facet-reflect-v0.28.0) - 2025-07-03

### Other

- Rename SmartPointer to Pointer
- Allow serializing references
- Manual impls for reference types
- Fix clippy warnings and remove unused code

## [0.27.15](https://github.com/facet-rs/facet/compare/facet-reflect-v0.27.14...facet-reflect-v0.27.15) - 2025-06-26

### Other

- Add support for `Arc<[U]>` in facet-core and facet-reflect
- Not sure where that came from...
- Inline obvious candidates (small functions)
- Evaluate the ConstTypeId once in ScalarType::try_from_shape()
- Inline everything
- Reduce size_of Frame 144 bytes -> 64 bytes
- Preallocate frames in Partial::alloc_shape()
- Inline selected functions in the critical path
- impl Facet for Arc<str>, Rc<str>, and Box<str>
- Fix minor copy-paste error
- Clippy suggestions
- Fix HashSet serializing to null due to Set being an unhandled type
- Remove tests
- Remove most composite impls for btreeset
- Remove Vec's clone_into composite impl
- Remove ScalarDef
- Remove PartialEq implementations
- Apply modern clippy fixes (mostly format strings)

## [0.27.13](https://github.com/facet-rs/facet/compare/facet-reflect-v0.27.12...facet-reflect-v0.27.13) - 2025-06-15

### Other

- Disable some tests under miri :(
- Add tests for Peek'ing DSTs
- inline all the things
- inline more things
- inline(always) maybe?
- Introduce GenericPtr
- Introduced sized/unsized ValueVTable variants
- s/Wip/Partial

## [0.27.10](https://github.com/facet-rs/facet/compare/facet-reflect-v0.27.9...facet-reflect-v0.27.10) - 2025-06-03

### Other

- Add discord logo + link
- Fix JSON serialization of &[u8] slices

## [0.27.9](https://github.com/facet-rs/facet/compare/facet-reflect-v0.27.8...facet-reflect-v0.27.9) - 2025-06-02

### Other

- Add tests for issue #691: tuple unsoundness with too few fields
- Migrate push_ methods to begin_ convention in facet-reflect
- Fix

## [0.27.8](https://github.com/facet-rs/facet/compare/facet-reflect-v0.27.7...facet-reflect-v0.27.8) - 2025-05-31

### Other

- Simplify code for set_numeric_value
- Fix doc tests
- Add serialization for box
- Resolve warnings etc.
- facet-json is not _currently_ nostd, actually, because of std::io::Write
- Fix facet-args tests
- YAML transparent types fixes
- More facet-yaml test fixes
- facet-json tests pass
- Fix tests
- Tuple handling
- Fix Bytes deserialization through implicit conversion
- Fix NonZero validation using conversion frames
- More facet-json tests
- wow everything typechecks
- facet-msgpack almost passing
- select_nth_variant
- wip toml
- facet-deserialize fixes
- Fix facet-serialize
- All facet-reflect tests pass
- Migrat arc tests
- Fix double-free in Partial Drop for map insertions
- Migrate more
- Port array_building tests to new Partial API
- Fix uninitialized type error reporting and improve error readability
- Port variance tests
- Remove yansi
- Start porting old reflect tests
- begin/end is more intuitive than push/pop
- We're about to use begin/end instead of push/pop
- Rename some methods
- Minor cleanups
- Convert Wip tests to use chaining style for alloc().push().set().pop()
- Fix memory leak in Wip::build when initialization check fails
- Fix union and opaque type handling, add comprehensive tuple tests
- Remove SequenceType::Tuple - tuples are now structs
- Add push_variant_named() method and tests
- Implement array element re-initialization in Wip
- Implement Map/HashMap support for Wip incremental building
- Implement List/Vec support for Wip incremental building
- Implement enum support for Wip incremental building
- Fix memory leak when setting struct fields multiple times
- Add Arc<T> support to Wip
- Add Box<T> support to Wip
- Add array building support to Wip
- Fix deallocation of field pointers by tracking frame ownership
- Fix premature drop bug in Wip::put and implement proper cleanup
- struct fully init
- Partially uninit struct
- Save
- facet-reflect reboot

## [0.27.6](https://github.com/facet-rs/facet/compare/facet-reflect-v0.27.5...facet-reflect-v0.27.6) - 2025-05-26

### Other

- Rename ValueVTable::eq to ValueVTable::partial_eq
- Add UnwindSafe and RefUnwindSafe to marker traits

## [0.27.5](https://github.com/facet-rs/facet/compare/facet-reflect-v0.27.4...facet-reflect-v0.27.5) - 2025-05-24

### Other

- Fix cyclic types with indirection for optional fns in `ValueVTable`
- Add `Wip.push_inner()` to handle wrapper types
- Make some methods in `ListVTable` optional
- Fix implementation for unsized types
- Re-enable fixed facts test
- Add failing test for &&str
- Add Peek tests showing UB

## [0.27.4](https://github.com/facet-rs/facet/compare/facet-reflect-v0.27.3...facet-reflect-v0.27.4) - 2025-05-21

### Other

- Support deserializing to `Arc<T>`
- Fix marker traits for references and also test marker traits in facts

## [0.27.3](https://github.com/facet-rs/facet/compare/facet-reflect-v0.27.2...facet-reflect-v0.27.3) - 2025-05-20

### Other

- Use iters from std for various `iter_vtable` impls
- Update `PeekTuple` to use `FieldIter`
- Add custom iterator impl for `HasFields.fields_for_serialize()`
- Add `FieldIter` as a concrete iter type for `HasFields`
- Move `HasFields` trait to separate module

## [0.27.2](https://github.com/facet-rs/facet/compare/facet-reflect-v0.27.1...facet-reflect-v0.27.2) - 2025-05-18

### Other

- Harmonize lifetime names, closes #636
- Introduce `'shape` lifetime, allowing non-'static shapes.
- Deallocate iters properly for `PeekList` and `PeekListLike`
- Update `PeekListLike` to support lists without `as_ptr` impl
- Update `PeekList` to support lists without `as_ptr` impl
- Make `as_ptr` and `as_mut_ptr` optional on `ListVTable`

## [0.27.1](https://github.com/facet-rs/facet/compare/facet-reflect-v0.27.0...facet-reflect-v0.27.1) - 2025-05-16

### Other

- Support more slices for facet-pretty
- Rust 1.87 clippy fixes
- Use a trait to make `IterVTable` generic over the item type
- Refactor `Map` to use `IterVTable`
- implement recursive serialize
- Use test attribute in facet-reflect
- Fix json tests
- Remove unused error variant

## [0.27.0](https://github.com/facet-rs/facet/compare/facet-reflect-v0.26.1...facet-reflect-v0.27.0) - 2025-05-13

### Other

- Allow iterating maps in reverse order
- Well it says the field is not initialized, so.

## [0.26.0](https://github.com/facet-rs/facet/compare/facet-reflect-v0.25.1...facet-reflect-v0.26.0) - 2025-05-12

### Added

- *(core)* add core implementation for `jiff::Zoned`

### Other

- Rename jiff feature to jiff02 (thanks @BurntSushi)
- Remove `camino` / `ulid` / `uuid` features from `facet-reflect`
- Remove camino/UUID/ULID variants from `ScalarType` enum
- Add support for time crate's OffsetDateTime and UtcDateTime

## [0.25.0](https://github.com/facet-rs/facet/compare/facet-reflect-v0.24.0...facet-reflect-v0.25.0) - 2025-05-10

### Other

- Introduce as_mut_ptr, fix array tests under miri
- Add support for partially initializing arrays, closes #504

## [0.20.0](https://github.com/facet-rs/facet/compare/facet-reflect-v0.19.1...facet-reflect-v0.20.0) - 2025-05-10

### Other

- Upgrade facet-core
- References don't have default, too (shame but)
- Clippy warnings
- references are not pointers
- Use hash of source for target dir when running slow tests
- Determine enum variant after default_from_fn
- Uncomment deserialize
- Fix up enums a bit
- Make variant() getters fallible — we might not know the internal enough to read the discriminant etc.
- debug facet-serialize
- Fix memory leak, work on facet-serisalize
- Rework type information (Def)

## [0.19.1](https://github.com/facet-rs/facet/compare/facet-reflect-v0.19.0...facet-reflect-v0.19.1) - 2025-05-08

### Other

- Add `PeekListLike` object

## [0.19.0](https://github.com/facet-rs/facet/compare/facet-reflect-v0.18.2...facet-reflect-v0.19.0) - 2025-05-06

### Fixed

- *(reflect)* add scalar types for camino, uuid & ulid
- *(reflect)* add missing scalar types

### Other

- Pick facet-json2
- Externally-driven facet-deserialize approach
- Get started on an event-based approach to facet-deserialize ([#500](https://github.com/facet-rs/facet/pull/500))

## [0.18.2](https://github.com/facet-rs/facet/compare/facet-reflect-v0.18.1...facet-reflect-v0.18.2) - 2025-05-02

### Added

- parse empty tuples, test cases

### Other

- Fix clone_into for collections
- JSON facet-serialize?
- Introduce facet-serialize
- Fix clone_into functions

## [0.18.1](https://github.com/facet-rs/facet/compare/facet-reflect-v0.18.0...facet-reflect-v0.18.1) - 2025-04-29

### Other

- Post-quote cleanups
- final cleanup
- delete silly idea from code, comment in test, we are golden it seems
- iterative serializer

## [0.11.0](https://github.com/facet-rs/facet/compare/facet-reflect-v0.10.4...facet-reflect-v0.11.0) - 2025-04-23

### Fixed

- *(toml)* ensure alloc is properly used and deny unsafe code

### Other

- Remove outdated comment
- Add missing file
- Fix invariance for lifetime paramters in derive
- Add (unsoundness proving) lifetime variance tests for facet-reflect
- Clippy fixes
- WIP
- WIP
- Back to depot runners.

## [0.10.4](https://github.com/facet-rs/facet/compare/facet-reflect-v0.10.3...facet-reflect-v0.10.4) - 2025-04-21

### Other

- Implement `Facet` for (subset of) function pointers
- Support tuple-enums in JSON
- put into tuples works
- replace format! with format_args! where ever possible
- Support field-level default
- Implement the skip_serializing/skip_serializing_if attribute
- Respect deny_unknown_fields (once again)
- Add tests for `Rc`'s and `Arc`'s smart pointer VTables
- Impl `Facet` for `Rc<T>`
- msrv/nostd fixes
- very nice error reporting as it turns out
- Use TryFrom to deserialize NonZero<T>
- ooh spicy
- Works for structs
- Introduce JSON tokenizer

## [0.10.3](https://github.com/facet-rs/facet/compare/facet-reflect-v0.10.2...facet-reflect-v0.10.3) - 2025-04-20

### Other

- Don't allocate strings in facet-json deserialization unless necessary
- Refactor JSON number deserialization to use Wip::try_put_f64

## [0.10.2](https://github.com/facet-rs/facet/compare/facet-reflect-v0.10.1...facet-reflect-v0.10.2) - 2025-04-19

### Added

- *(json)* Support default attribute.
- feat(json) Support default at the container level
- feat(json) Better error messages when a field is missing

## [0.10.1](https://github.com/facet-rs/facet/compare/facet-reflect-v0.10.0...facet-reflect-v0.10.1) - 2025-04-19

### Added

- feat(json) Support deny_unknown_fields

## [0.10.0](https://github.com/facet-rs/facet/compare/facet-reflect-v0.9.1...facet-reflect-v0.10.0) - 2025-04-18

### Other

- Never restore state when pushing a map key and also attempt not to track them.

## [0.9.1](https://github.com/facet-rs/facet/compare/facet-reflect-v0.9.0...facet-reflect-v0.9.1) - 2025-04-18

### Other

- Attempt to set up release-plz again

## [0.6.2](https://github.com/facet-rs/facet/compare/facet-reflect-v0.6.1...facet-reflect-v0.6.2) - 2025-04-12

### Added

- *(reflect)* add `ScalarType` enum ([#173](https://github.com/facet-rs/facet/pull/173))

### Other

- Impl `Facet` for `Arc<T>` ([#180](https://github.com/facet-rs/facet/pull/180))
- Install cargo-tarpaulin in Docker, and collect + report coverage in CI ([#177](https://github.com/facet-rs/facet/pull/177))
- Use anstyle ([#170](https://github.com/facet-rs/facet/pull/170))
- Opaque initialization of Some ([#169](https://github.com/facet-rs/facet/pull/169))
- TOML enum with unit variant implementation ([#168](https://github.com/facet-rs/facet/pull/168))

## [0.6.1](https://github.com/facet-rs/facet/compare/facet-reflect-v0.6.0...facet-reflect-v0.6.1) - 2025-04-12

### Other

- different place in readme
- Sponsored by depot

## [0.6.0](https://github.com/facet-rs/facet/compare/facet-reflect-v0.5.0...facet-reflect-v0.6.0) - 2025-04-11

### Changed
- Merged `facet-peek` and `facet-poke` into `facet-reflect`
- Combined functionality for reading and writing Facet types
