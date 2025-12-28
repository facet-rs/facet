# test-external-service

Integration test crate that validates auto-generated ServiceDispatch wrappers work correctly in external crates (not inside rapace-cell itself).

This test demonstrates the fix for [issue #100](https://github.com/bearcove/rapace/issues/100), ensuring that the `#[rapace::service]` macro generates proper `ServiceDispatch` implementations regardless of where the service is defined.
