//! Facet derive plugin for dibs.
//!
//! This implements the `#[facet(derive(dibs::Table))]` plugin which automatically
//! registers types with `inventory` so they can be collected by `Schema::collect()`.

/// Plugin chain entry point.
///
/// Called by `#[derive(Facet)]` when `#[facet(derive(dibs::Table))]` is present.
/// Adds the Table plugin template to the chain and forwards to the next plugin or finalize.
#[macro_export]
macro_rules! __facet_invoke {
    (
        @tokens { $($tokens:tt)* }
        @remaining { $($remaining:tt)* }
        @plugins { $($plugins:tt)* }
        @facet_crate { $($facet_crate:tt)* }
    ) => {
        // Forward with our template added to plugins
        $crate::__facet_invoke_internal! {
            @tokens { $($tokens)* }
            @remaining { $($remaining)* }
            @plugins {
                $($plugins)*
                @plugin {
                    @name { "dibs::Table" }
                    @template {
                        // Register this type with inventory for schema collection
                        $crate::inventory::submit!($crate::TableDef::new::<@Self>());
                    }
                }
            }
            @facet_crate { $($facet_crate)* }
        }
    };
}

/// Internal macro that either chains to next plugin or calls finalize
#[doc(hidden)]
#[macro_export]
macro_rules! __facet_invoke_internal {
    // No more plugins - call finalize
    (
        @tokens { $($tokens:tt)* }
        @remaining { }
        @plugins { $($plugins:tt)* }
        @facet_crate { $($facet_crate:tt)* }
    ) => {
        $($facet_crate)*::__facet_finalize! {
            @tokens { $($tokens)* }
            @plugins { $($plugins)* }
            @facet_crate { $($facet_crate)* }
        }
    };

    // More plugins - chain to next
    (
        @tokens { $($tokens:tt)* }
        @remaining { $next:path $(, $rest:path)* $(,)? }
        @plugins { $($plugins:tt)* }
        @facet_crate { $($facet_crate:tt)* }
    ) => {
        $next! {
            @tokens { $($tokens)* }
            @remaining { $($rest),* }
            @plugins { $($plugins)* }
            @facet_crate { $($facet_crate)* }
        }
    };
}
