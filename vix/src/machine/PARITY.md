# Machine Parity Ledger

This ledger is the funeral gate for the frozen evaluators named by
`vix/src/machine/mod.rs`: `vix/src/oracle.rs` and `vix/src/engine.rs` may die
only after every evaluator assertion below is reproduced on the machine, on
both lanes, or is explicitly accepted as outside the evaluator surface.

Classification:

1. Already reproduced on the machine.
2. Reproducible with existing machine features; write the machine test.
3. Needs a named machine feature.
4. Oracle-only or non-machine by design; must be explicitly accepted before the
   funeral if it remains in scope.

## Summary

| Class | Count | Meaning |
| --- | ---: | --- |
| 1 | 57 | Already covered by current machine tests. |
| 2 | 0 | Machine can already express it; missing parity test only. |
| 3 | 0 | Needs feature work before parity. |
| 4 | 3 | Parser/wire-shipping assertions outside the evaluator machine. |

Class 4 is intentionally not used for any evaluator behavior.

## Debts Carried Past The Funeral

- O12 is accepted as Class 4 only for its oracle-instance vehicle. The
  underlying semantic property remains a fleet-arc debt: canonical closure hash
  must stay stable across serialization, and remote invocation must agree with
  local invocation.
- O14 and L01 are accepted as Class 4 with a relocation condition. The funeral
  deletes evaluator test files, not parser/editor contracts; any highlight or
  typed-AST shape assertion hosted in a doomed file must move to a surviving
  parser/editor test file before deletion.

## Engine Pinned Contracts

| ID | Frozen assertion | Class | Machine parity status |
| --- | --- | ---: | --- |
| E01 | `eval_vix_contract_is_pinned`: `eval.vix::demo()` returns `Float(42.0)` and has no finished runs, scheduled runs, created runs, observations, or journal pins. | 1 | `eval_vix_demo_returns_42_on_the_machine` proves bits-exact `42.0`, spawn count, warm memo trace, and both-lane trace equality. No run events are produced. |
| E02 | `types_vix_contract_is_pinned`: `types.vix::partials()` returns `Int(42)` with empty event/journal contract. | 1 | Covered by `types_vix_partials_depths_and_classify_run_on_the_machine` on both lanes; partial application lowers to a pending invocation with named args resolved before the machine boundary. |
| E03 | `types_vix_contract_is_pinned`: `types.vix::depths()` returns `Int(2)` with empty event/journal contract. | 1 | Covered by `types_vix_partials_depths_and_classify_run_on_the_machine` on both lanes via tuple values and tuple-index store projection. |
| E04 | `types_vix_contract_is_pinned`: `classify(Artifact::Object("lua.o"))` returns `"the interpreter object"` with empty event/journal contract. | 1 | Covered by `types_vix_partials_depths_and_classify_run_on_the_machine` on both lanes via guarded variant matching and path comparison. |
| E05 | `types_vix_contract_is_pinned`: `classify(Artifact::Object("lapi.o"))` returns `"an object"` with empty event/journal contract. | 1 | Covered by `types_vix_partials_depths_and_classify_run_on_the_machine` on both lanes via guard fallthrough to the tuple-variant wildcard arm. |
| E06 | `types_vix_contract_is_pinned`: `toolchain(windows target)` returns a `Toolchain` struct with acquired `Cc` and `Ar`, `opt = 1`, `env = {"CFLAGS":"-O2","LDFLAGS":"-lm"}`, observations for both capabilities, and journal pins for both. | 1 | Covered by `types_vix_toolchain_acquires_capabilities_and_updates_records` on both lanes: record update allocates a new store entry, `Target.os` projects through declared layout, `Cc`/`Ar` acquisition emits two non-replayed observation events, and the returned struct/env map is inspected in the value store. |
| E07 | `lua_vix_contract_is_pinned`: `lua.vix::lua(linux target)` returns tree `lua -> obj(da0b3249eab2761b)`, 5 scheduled, 5 created, 3 finished cc runs at `lapi.o`, `lauxlib.o`, and `lua`, plus `Cc`, `Ar`, and fetch observations/journal pins. | 1 | Covered by `lua_vix_runs_on_machine_with_exec_depth_contract` on both lanes: fetch/extract, glob/filter, flag arrays, `ar!`, multi-input `cc!`, and projection-preserving run accounting produce 5 requested/started runs, 3 completed `cc/Ran` events, and the pinned fetch/acquire observations. |
| E08 | `cargo_toml_projection_contract_is_pinned`: `cargo_manifest(Cargo.toml tree)` returns `("mini-real-crate", "0.3.1", "0.50.0-rc.5")` with empty event/journal contract. | 1 | Covered by `cargo_toml_projection_runs_on_the_machine` on both lanes: TOML parses through the driver primitive into structural `Doc` values (`Map<String,Doc>` rows), `.get().unwrap()` stays structural, typed tuple fields coerce `Doc` to `String`, and the returned tuple is inspected in the store. |
| E09 | `json_structural_values_contract_is_pinned`: parsing the inline JSON returns `("mini-real-crate", 3, false)` with empty event/journal contract. | 1 | Covered by `inline_json_structural_values_run_on_the_machine` on both lanes: JSON parses into canonical `Doc` values including bool, nested object projection uses `Doc.get`, and tuple return fields coerce to `String`, `Int`, and `Bool`. |
| E10 | `fetch_without_declared_checksum_contract_is_pinned`: two calls with different `nonce` values return the same source tree; observation key is `fetch:https://example.org/source.tar.gz:observed`; journal pin is the observed SHA-256. | 1 | Covered by `machine_fetch_without_declared_checksum_pins_and_replays` on both lanes with injected fake fetch backend, observed-checksum journal pin, and replay on the second distinct memo key. |
| E11 | `engine_tunnels_path_demand_through_merge`: `selected(linux)` returns `wanted.o -> obj(9259fea8a69f1945)`, schedules 1 run, creates 2 pending runs, observes/pins `Cc`, and has no finished run. | 1 | `merge_demand_selected_tunnels_and_never_runs_left` proves result, `object` spawn count 1, run set `{wanted.o}`, `left.o` absence, warm 2-event trace, and both-lane trace equality. |
| E12 | `engine_falls_left_after_right_merge_absence_is_known`: `fallback(linux)` returns `wanted.o -> obj(9259fea8a69f1945)`, schedules 2, creates 2, finishes `right.o` once, observes/pins `Cc`. | 1 | `merge_demand_fallback_falls_left_after_right_absence` proves result, object spawn count 2, run set `{right.o,wanted.o}`, `left.o` absence, and both-lane trace equality. Machine `RunCompleted` is finer than engine `Finished`; the absence proof is the asserted run set. |
| E13 | `engine_refines_subtree_chain_through_merge`: `subtree_chain(linux)` returns `wanted.o -> obj(9259fea8a69f1945)`, schedules 1, creates 2, observes/pins `Cc`, and has no finished run. | 1 | `merge_demand_subtree_chain_refines_without_left` proves result, `object` spawn count 1, run set `{x/wanted.o}`, `left.o` absence, and both-lane trace equality. |

## Engine/Oracle Differential Contracts

| ID | Frozen assertion | Class | Machine parity status |
| --- | --- | ---: | --- |
| D01 | Engine and oracle match on `eval.vix::demo`: value, finished multiset, scheduled count, observations, journal, miss subset, and created subset. | 1 | Machine already pins the value, zero run events, spawn count, and warm behavior for `eval.vix::demo` on both lanes. |
| D02 | Engine and oracle match on `types.vix::{partials,depths,classify(lua.o),classify(lapi.o),toolchain(windows)}` with the full event/journal subset rules. | 1 | Covered by the B3 machine tests for partials, depths, classify, and toolchain on both lanes, including exact capability observation replay flags. |
| D03 | Engine and oracle match on `lua.vix::lua`: same value, same finished multiset, scheduled count 5, created set equality, observation keys, journal, and miss subset. | 1 | Covered machine-side by `lua_vix_runs_on_machine_with_exec_depth_contract`: same value shape, 5 requested/started runs, 3 completed `cc/Ran` events, fetch/acquire observations, and warm root hit. |
| D04 | Engine and oracle match on Cargo.toml projection. | 1 | Covered by `cargo_toml_projection_runs_on_the_machine`; the machine reproduces the frozen Cargo.toml tuple result through the same fixture and tree/file content projection. |
| D05 | Engine and oracle match on JSON structural values. | 1 | Covered by `inline_json_structural_values_run_on_the_machine`; the machine reproduces the frozen inline JSON tuple result including the bool field. |
| D06 | Engine and oracle match on fetch without declared checksum: both calls equal, both engines equal each other, journal key/value exactly matches observed checksum, and observation keys match. | 1 | Covered by `machine_fetch_without_declared_checksum_pins_and_replays`; the machine now has `with_fetch_backend` and emits fetch observation replay state through `DriveEvent::Observation`. |
| D07 | Merge selected differential: machine/engine value equals oracle, journal equals oracle, engine finished outputs are a strict subset, engine lacks `left.o`, oracle has `left.o` and `wanted.o`. | 1 | Machine milestone tests prove the stricter machine-side assertion: `left.o` producer never materializes and exact run set excludes it on both lanes. |
| D08 | Merge fallback differential: engine value equals oracle; engine finished contains `right.o`, excludes `wanted.o`, while oracle has both. | 1 | Machine milestone test proves the demanded-path behavior with exact run set `{right.o,wanted.o}` and no `left.o` on both lanes. |
| D09 | Merge subtree-chain differential: engine value equals oracle; engine finished excludes `left.o` and `x/wanted.o`, while oracle has both. | 1 | Machine milestone test proves exact run set `{x/wanted.o}` and no `left.o` on both lanes. |

## Engine-Only Lazy/Strictness Contracts

| ID | Frozen assertion | Class | Machine parity status |
| --- | --- | ---: | --- |
| G01 | `collect_argument_strictness_is_pinned`: `[2,1].collect(0)` errors exactly `collect takes no arguments`; `[2,1].collect()` returns sorted array `[1,2]`. | 1 | Covered by `scalar_array_collect_sorts_and_rejects_arguments` on both lanes. Machine lowering rejects the bad source as `lowering bad: collect takes no arguments`, preserving the pinned diagnostic as the leaf text; scalar `Array<Int>` collect returns sorted `[1,2]`. |
| G02 | `resolved_tree_missing_path_errors_immediately`: projecting missing path from a concrete tree errors with the path, schedules 0, and finishes 0. | 1 | Covered by `concrete_tree_missing_path_errors_without_runs` on both lanes. |
| G03 | `pending_tree_path_projection_serves_file_without_finish`: projecting `cc! { -o artifact.o } / p"artifact.o"` returns a one-entry tree, schedules 1, and records no finished run. | 1 | Covered by `pending_tree_projection_serves_file_through_one_run` on both lanes. Machine `RunCompleted` is finer than engine `Finished`; parity is asserted as one requested/completed `artifact.o` run and the projected one-entry tree. |
| G04 | `pending_tree_missing_path_errors_when_producer_finishes`: projecting `never-written.o` from a pending `cc! { -o artifact.o }` errors with `never-written.o`, schedules 1, and finishes one `cc` run. | 1 | Covered by `pending_tree_missing_path_errors_after_one_run` on both lanes. |
| G05 | `warm_engine_lua_second_call_is_one_hit`: second `lua(linux)` call returns same tree and appends exactly one hit for `lua`. | 1 | Covered by `lua_vix_runs_on_machine_with_exec_depth_contract`: after the cold build, warm `lua(linux)` is exactly `[Demanded(lua), MemoHit(lua)]` on both lanes. |
| G06 | `unused_command_binding_is_never_created_by_engine`: unused `let dead = cc! { -o dead }` returns `7`, creates no runs, finishes no runs. | 1 | Covered by `unused_command_binding_emits_no_exec_ops_or_runs` on both lanes via demand-sunk let lowering; the compiled task program contains no `EXEC_HOST` op. |
| G07 | `unused_binding_is_never_demanded`: unused `let x = expensive()` returns `7`; `expensive` has zero misses. | 1 | Covered by `unused_let_call_never_spawns` on both lanes via demand-sunk let lowering. The older undemanded-function test remains separate coverage. |
| G08 | `shared_binding_computes_once`: `let x = f(20); x + x` returns `42`; `f` misses once and has zero hits. | 1 | Covered by `shared_let_binding_computes_once` on both lanes. |
| G09 | `unselected_match_arm_never_evaluates`: match returns `42`; `boom` has zero misses. | 1 | Covered by `untaken_arms_never_spawn` and `untaken_variant_arms_never_spawn` on both lanes. |
| G10 | `memo_hits_across_calls`: `a()+b()` returns `42`; `f(20)` misses once and hits at least once. | 1 | Covered directly by `memo_hits_across_distinct_calls_exact_counts` on both lanes: one spawn and one memo hit for `f`. |
| G11 | Duplicate named arguments are rejected by oracle with text containing `duplicate argument \`x\``. | 1 | Covered by `types_vix_named_argument_diagnostics_are_pinned` on both lanes with machine lowering text containing `duplicate argument \`x\``. |
| G12 | Duplicate named arguments are rejected by engine with text containing `duplicate argument \`x\``. | 1 | Covered by `types_vix_named_argument_diagnostics_are_pinned` on both lanes with the same machine lowering diagnostic. |

## Oracle Warm Reload, Identity, and Value Contracts

| ID | Frozen assertion | Class | Machine parity status |
| --- | --- | ---: | --- |
| O01 | `eval_vix_computes_42`: oracle `eval.vix::demo()` returns `Float(42.0)`. | 1 | Covered by machine `eval_vix_demo_returns_42_on_the_machine` on both lanes. |
| O02 | `memo_hits_and_identity_survives_trivia`: cold `demo` events are all misses; warm second call is one hit for `demo`; trivia/comments preserve `demo` and `eval` hashes; semantic edit changes `demo` hash but not `eval`. | 1 | Covered by `warm_reload_eval_identity_survives_trivia_and_semantic_edits` on both lanes: cold has no memo hits, warm is exactly root `MemoHit`, trivia preserves `demo`/`eval` hashes, and semantic demo edit changes only `demo`. |
| O03 | `warm_reload_trivia_anywhere_costs_zero_misses_and_zero_runs`: after trivia-only reload, `main` returns `37`, zero misses/created events, and only `main` hits. | 1 | Covered by `warm_reload_trivia_costs_only_root_hit` on both lanes; `Machine::reload` rebuilds lowering tables while preserving memo/value store, and the post-reload trace has zero spawns and only `main` hit. |
| O04 | `warm_reload_leaf_semantic_edit_misses_exact_theoretical_blast_radius`: changing `leaf` from 1 to 2 makes `main` return `39`; misses are exactly `{leaf,left,right,main}`; `independent` hits; `never_demanded` does not hit. | 1 | Covered by `warm_reload_leaf_edit_misses_exact_blast_radius` on both lanes, with exact reload diff and exact spawned-function set `{leaf,left,right,main}` plus `independent` hit. |
| O05 | `warm_reload_never_demanded_semantic_edit_costs_zero_misses`: editing unused `never_demanded` keeps `main = 37`, zero misses/created events, and only `main` hits. | 1 | Covered by `warm_reload_unused_edit_costs_zero_misses_and_hashes_only_itself` on both lanes: zero spawns, only `main` hit, and `main` remains `37`. |
| O06 | `editing_unreferenced_function_preserves_other_closure_hashes`: editing `never_demanded` preserves hashes for `{leaf,left,right,independent,main}` and changes only `never_demanded`. | 1 | Covered by `warm_reload_unused_edit_costs_zero_misses_and_hashes_only_itself` using `Machine::fn_hashes()` and `ReloadDiff`; only `never_demanded` changes. |
| O07 | `warm_reload_type_declaration_edit_misses_exact_transitive_users`: reordering `enum Choice { A, B }` to `{ B, A }` keeps `main = 8`, misses exactly `{typed,bridge,main}`, and `independent` hits. | 1 | Covered by `warm_reload_type_decl_edit_misses_transitive_users` on both lanes; closure hashes include type declarations, producing exact diff/spawn set `{typed,bridge,main}` and an `independent` hit. |
| O08 | `recursive_scc_closure_hashes_are_stable_across_definition_order`: mutually recursive `a`/`b` hashes are equal across definition order. | 1 | Covered by `recursive_scc_hashes_survive_definition_order_on_machine` on both lanes via machine-visible `fn_hash`. |
| O09 | `types_vix_partials_guards_and_tuple_indexing`: `partials = 42`, `depths = 2`, `classify(lua.o)` and `classify(lapi.o)` return the two pinned strings, and calling `scaled(k:2)` without `x`/`..` errors. | 1 | Covered by `types_vix_partials_depths_and_classify_run_on_the_machine` and `types_vix_named_argument_diagnostics_are_pinned` on both lanes, including the exact missing-argument text for `scaled(k: 2)`. |
| O10 | `toolchain_acquires_capabilities_and_updates_records`: Windows target returns a `Toolchain` struct with `opt = 1`, two env entries including `CFLAGS=-O2`, and exactly two non-replayed capability observations. | 1 | Covered by `types_vix_toolchain_acquires_capabilities_and_updates_records` on both lanes with direct store inspection and exact observation events. |
| O11 | `fetch_pins_the_journal_and_replays`: fetch with declared checksum returns equal trees for different nonce args; observations are first `replayed=false`, then `replayed=true`; journal pins checksum under `fetch:{url}:sha256:{sha}`. | 1 | Covered by `machine_fetch_declared_checksum_replays_pin` on both lanes with exact declared-checksum observation keys and replay flags. |
| O12 | `closures_ship_between_oracles`: closure values ship/receive across oracle instances, preserve canonical hash, invoke remotely to `42`, and formatting preserves closure hash. | 4 | Closure serialization/wire transport is an oracle exec-prototype contract, not a machine evaluator requirement. If closure values become machine values, this should be reclassified as a machine feature. |
| O13 | `values_are_totally_ordered_canonically`: enum declaration order, float total order/NaN last/`-0.0 == 0.0`, canonical map key order, map hash equality across construction orders, and variant payload ordering. | 1 | Covered by `store_values_are_totally_ordered_canonically_on_the_machine` on both lanes: the driver exposes recursive store-value comparison, declared enum tags/payloads order correctly, float comparison canonicalizes NaN and signed zero, and map storage/hash order is the same canonical value order. |
| O14 | `highlights_query_captures_lua_sample`: highlight captures are non-empty, contain `keyword fn`, `function sources`, `string.special.path p"lua.c"`, and are sorted by start byte. | 4 | Parser/editor highlighting contract, not evaluator or machine runtime. It should stay outside the funeral unless the parser test suite is deliberately moved. |

## Lua Parser Contract

| ID | Frozen assertion | Class | Machine parity status |
| --- | --- | ---: | --- |
| L01 | `lua_sketch_lowers_to_typed_ast`: `lua.vix` parses as 5 items with `use vix::{Tree,Path,Target}`; 3 functions named `sources`, `object`, `lua`; `sources` tail is `/ p"lua-5.4.8/src"`; `object` params are `cc,src,unit,defines`, `defines` is `[Flag]`, and tail is `cc!` with 9 parts/4 splices; `lua` is public, has 8 statements, Linux match arm emits `-DLUA_USE_LINUX`, wildcard arm emits empty array, filter closure param is `u`, and final link command has a `/` splice. | 4 | Parser/typed-AST shape contract, not evaluator behavior. It remains relevant for lowering work but should not block evaluator funeral if parser tests remain. |

## Compose Contracts

| ID | Frozen assertion | Class | Machine parity status |
| --- | --- | ---: | --- |
| C01 | `lua_builds_end_to_end`: `lua(linux)` returns a tree containing `lua` whose contents start with `obj(`; events have 5 `Created`, 5 `Scheduled`, 3 finished `cc` runs, all finished events are `cc/Ran`; warm second call is exactly one hit for `lua`. | 1 | Covered by `lua_vix_runs_on_machine_with_exec_depth_contract` on both lanes with exact requested/started/completed counts and warm root-hit trace. |
| C02 | `fn_memo_and_exec_tiers_compose`: changing an unread README changes the function memo key so `object` misses, but exec tier 2 cuts off and output is equal; changing a read header reruns and output changes. | 1 | Covered by `machine_fn_memo_and_exec_tiers_compose` on both lanes. The one-`-I` machine source shape verifies three read-set observations; the underlying frozen two-search-dir substrate still pins `verified: 4` in `vix/tests/exec.rs`. |

## Exec Substrate Contracts

These tests are not frozen evaluator tests, but the machine exec seam must reuse
this substrate rather than rebuild it.

| ID | Frozen assertion | Class | Machine parity status |
| --- | --- | ---: | --- |
| X01 | Cold exec run records `Ran`, emits `/out/lua.o`, read-set includes source content, vendored absent header, system header content, and excludes unread README. | 1 | Frozen substrate test `cold_run_pins_reads_and_negative_lookups` remains the read-set proof; machine B4 uses the same `ExecCache`/`FakeCc` path and exposes `Ran` through `RunCompleted.serving` in `lua_vix_runs_on_machine_with_exec_depth_contract` and `machine_fn_memo_and_exec_tiers_compose`. |
| X02 | Identical second exec is `Tier1Hit` and returns output. | 1 | Frozen substrate test `tier1_hits_when_nothing_changed` remains the direct cache proof; machine seam proof is `machine_commuting_flags_share_exec_identity`, which observes `RunCompleted { serving: Tier1Hit }` after a semantically identical second command reaches the driver. |
| X03 | Unread README edit causes `Tier2Cutoff { verified: 4 }`, equal outputs, then a third call is `Tier1Hit`. | 1 | Frozen substrate test `unread_change_cuts_off_at_tier2_the_anti_nix_test` pins `verified: 4`; machine seam proof is `machine_fn_memo_and_exec_tiers_compose`, which observes `Tier2Cutoff` and equal outputs for the one-search-dir `cc!` source shape. |
| X04 | Read system header edit causes two `Ran` events and different outputs. | 1 | Frozen substrate test `read_header_change_reruns` pins the cache rule; machine seam proof is `machine_fn_memo_and_exec_tiers_compose`, whose read header edit produces `RunCompleted { serving: Ran }` and different output. |
| X05 | Header appearing earlier in search path reruns, output changes, and the new read-set records vendored header content instead of absence. | 1 | Covered by frozen substrate test `shadowing_header_diverges_the_pinned_absence`; the machine uses the same `assign_roles`/`ExecCache`/`FakeCc` path for `cc!` inputs and search dirs. |
| X06 | Capability fingerprint change disables tier-2 reuse: same files, two different compiler fingerprints both `Ran`. | 1 | Covered by frozen substrate test `capability_change_disables_tier2_reuse`; machine capability handles feed the same capability fingerprint into `ExecCache` through B4 `execute_request`. |
| X07 | Input outside mounts errors with text containing `outside the mounts`. | 1 | Covered by frozen substrate test `ceiling_is_the_mount_set`; machine command splicing constructs the same mount ceiling and propagates `ExecCache`/tool errors through the driver result path. |
| X08 | Directory listing observations verify unchanged and diverge on deletion or addition. | 1 | Covered by frozen substrate test `listings_pin_additions_and_deletions`; machine B4 reuses `MountedWorld`/`ReadSet::Listing` unchanged for `ar!` directory inputs and collected tree mounts. |
| X09 | Reordered commuting flags have different byte hash but same semantic identity, second run tier-1 hits; swapped search dirs have different identity. | 1 | Covered by frozen substrate test `normalization_makes_reordered_flags_share_identity` and machine seam test `machine_commuting_flags_share_exec_identity`, which observes the second command as `RunCompleted { serving: Tier1Hit }` on both lanes. |
