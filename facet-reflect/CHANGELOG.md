# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

## [0.44.6](https://github.com/facet-rs/facet/compare/facet-reflect-v0.44.5...facet-reflect-v0.44.6) - 2026-04-14

### Added

- *(reflect)* expose push/pop/swap + enum set_field_from_heap
- *(reflect)* add HeapValue-based type-erased mutation variants
- *(reflect)* add is_* predicates to Poke for every into_* case
- *(reflect)* add array/object builders to PokeDynamicValue
- *(reflect)* add Poke API parity with Peek

### Fixed

- *(reflect)* drop unsound iter-vtable fallback in Poke list iter_mut

### Other

- *(reflect)* correct PokePointer summary — it's read-only today
- *(reflect)* add PokeTuple integration tests
- *(reflect)* drop PokeOption::replace_with_raw

## [0.44.5](https://github.com/facet-rs/facet/compare/facet-reflect-v0.44.4...facet-reflect-v0.44.5) - 2026-04-13

### Fixed

- *(reflect)* drop RopeSlot frame contents in-place on cleanup
- *(reflect)* defer rope slot init mark to consume-time, fix doc link
- *(reflect)* avoid UAF when stored MapKey frame co-owns pending-entry key

### Other

- *(reflect)* consume-time pending-slot population, drop Transferred variant
- *(reflect)* remove now-dead MapInsertState init-tracking fields
- *(reflect)* single-source-of-truth ownership via FrameOwnership::Transferred

## [0.44.4](https://github.com/facet-rs/facet/compare/facet-reflect-v0.44.3...facet-reflect-v0.44.4) - 2026-04-13

### Fixed

- *(reflect)* drain rope before dropping Vec in List frame deinit
- *(reflect)* prevent double-free in List deinit when Vec is initialized
- *(reflect)* drop map key when severing failed MapValue pending entry
- *(reflect)* prevent double-free in deferred-mode map/option/smartptr cleanup
- *(deps)* update rust dependencies ([#2158](https://github.com/facet-rs/facet/pull/2158))

### Other

- *(reflect)* drop start_depth from finish_deferred parent lookups
