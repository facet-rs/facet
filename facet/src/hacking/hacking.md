## Hacking Guide to Facet

## The Facet Trait and Its Purpose

The `Facet` trait is the cornerstone of our reflection system. It provides a way to access type information at both compile time and runtime, enabling powerful meta-programming capabilities while maintaining Rust's safety guarantees.

```rust,ignore
pub unsafe trait Facet: Sized {
    /// The shape of this type
    const SHAPE: &'static Shape;
}
```

### Core Concept

The `Facet` trait allows any implementing type to expose its structural information through a static `Shape` object. This enables introspection of types at compile time, powering serialization, deserialization, debugging, and other operations that need to understand the structure of data.

## Specialization via Auto-Deref

Facet uses a technique called "auto-deref-based specialization" to enable trait-like specialization on stable Rust. This approach allows us to conditionally implement functionality based on what traits a type implements, all without requiring the unstable `specialization` feature.

### How Auto-Deref Specialization Works

The specialization technique uses Rust's method resolution rules to our advantage. As described by Lukas Kalbertodt:

> Autoderef-based specialization works by (ab)using the fact that method resolution prefers resolving to methods which require fewer type coercion of the receiver over methods that require more coercions.

For example, in our code:

```rust
# use core::fmt::{self, Debug};
// Wrapper struct for the specialization trick
struct Spez<T>(T);

// Trait for types that implement Debug
trait SpezDebugYes {
    fn spez_debug(&self, f: &mut fmt::Formatter<'_>) -> Result<(), fmt::Error>;
}

// Trait for types that don't implement Debug
trait SpezDebugNo {
    fn spez_debug(&self, f: &mut fmt::Formatter<'_>) -> Result<(), fmt::Error>;
}

// For types that implement Debug
impl<T: Debug> SpezDebugYes for &Spez<T> {
    fn spez_debug(&self, f: &mut fmt::Formatter<'_>) -> Result<(), fmt::Error> {
        Debug::fmt(&self.0, f)
    }
}

// Fallback for types that don't implement Debug
impl<T> SpezDebugNo for Spez<T> {
    fn spez_debug(&self, _f: &mut fmt::Formatter<'_>) -> Result<(), fmt::Error> {
        unreachable!()
    }
}
```

When we use these traits, the compiler will prefer the first implementation if the type implements `Debug`, and fall back to the second one otherwise. This enables a form of specialization without requiring the unstable feature.

### Limitations of Auto-Deref Specialization

It's important to understand when this specialization technique can and cannot be used:

1. **Only works in macros and non-generic contexts**: Auto-deref specialization is primarily useful in macros (like those in `facet-derive`) and for non-generic, scalar types like `i32`, `u32`, `String`, etc.

2. **Not suitable for generic types**: For types with generic parameters (like `HashMap<K, V>`), this approach doesn't work well because the specialization cannot be done based on properties of the generic parameters.

3. **Alternative for generic types**: For generic types, we instead leverage the fact that `SHAPE` is a `const` associated value that can be queried in `const` contexts. This allows us to perform compile-time checks and conditional logic based on properties of the generic parameters.

For example, in a generic implementation like `HashMap<K, V>`, we directly access the marker traits of `K::SHAPE` and `V::SHAPE` at compile time to determine what traits to implement for the containing type.

This pattern is used throughout the codebase for various traits like `Debug`, `Display`, `Clone`, `Hash`, and more, with different specialization approaches depending on whether we're dealing with non-generic or generic types.

## Concrete Example: PartialOrd Specialization

Let's examine how the `PartialOrd` trait is conditionally implemented using both approaches:

### Generic Type Example: Array Implementation

For arrays like `[T; 1]`, we need to check if the inner type `T` implements `PartialOrd`. Since this is a generic type, we use compile-time evaluation of `SHAPE`:

```rust
# use facet::{PtrConst, Shape, Facet};
# use core::cmp::Ordering;
fn create_array_shape<'a, T: Facet<'a>>() {
    let vtable = {
        // Implementation of partial_ord for arrays
        let partial_ord = if T::SHAPE.vtable.partial_ord.is_some() {
            Some(|a: PtrConst, b: PtrConst| {
                let a = unsafe { a.get::<[T; 1]>() };
                let b = unsafe { b.get::<[T; 1]>() };
                unsafe {
                    (T::SHAPE.vtable.partial_ord.unwrap_unchecked())(
                        PtrConst::new(&a[0]),
                        PtrConst::new(&b[0]),
                    )
                }
            })
        } else {
            None
        };
        // Rest of vtable implementation...
    };
}
```

Here's what's happening:
1. We check if `T::SHAPE.vtable.partial_ord` is `Some`, which tells us if `T` implements `PartialOrd`
2. If it does, we provide a `partial_ord` implementation that:
   - Extracts arrays from opaque pointers
   - Gets the first element from each array
   - Delegates to the inner type's `partial_ord` implementation
3. If `T` doesn't implement `PartialOrd`, we set `partial_ord` to `None`

### Non-Generic Type: Using `value_vtable` Macro

For non-generic types, we use the `value_vtable` macro which leverages auto-deref specialization:

```rust
# // This is a simplified version of what happens in the actual code
# use facet::PtrConst;

# #[macro_export]
# macro_rules! value_vtable {
#     ($type_name:ty) => {
#         {
#             // Other vtable fields would be here...
            let partial_ord = if facet::spez::impls!($type_name: core::cmp::PartialOrd) {
                Some(|left: PtrConst, right: PtrConst| {
                    use facet::spez::*;
                    (&&Spez(unsafe { left.get::<$type_name>() }))
                        .spez_partial_cmp(&&Spez(unsafe { right.get::<$type_name>() }))
                })
            } else {
                None
            };
#             // Return the vtable
#             partial_ord
#         }
#     };
# }
#
# fn main() {
#     let _vtable = value_vtable!(u32);
#     println!("Generated vtable for u32");
# }
```

Here's what's happening:
1. The `impls!` macro uses auto-deref specialization to check if `$type_name` implements `PartialOrd`
2. If it does, we create a function that:
   - Extracts the values from opaque pointers
   - Wraps them in `Spez` (specialization helper)
   - Calls `spez_partial_cmp` which uses method resolution to pick the right implementation
3. If it doesn't implement `PartialOrd`, we set `partial_ord` to `None`

### Key Differences

These examples highlight the two approaches to specialization in the Facet codebase:

1. **Generic approach**: Directly inspects `T::SHAPE` at compile time for trait information
2. **Non-generic approach**: Uses the `impls!` macro with auto-deref trick for specialization

Both approaches have the same goal: conditionally implement functionality based on trait implementations, but they use different mechanisms based on whether we're dealing with generic or non-generic types.

## Working with Characteristics and MarkerTraits

The `Characteristic` enum represents various traits that a type can implement, including both marker traits (like `Send`, `Sync`, `Copy`) and functionality traits (like `Debug`, `Clone`, `PartialEq`).

```rust
pub enum Characteristic {
    // Marker traits
    Send,
    Sync,
    Copy,
    Eq,
    // Functionality traits
    Clone,
    Debug,
    PartialEq,
    PartialOrd,
    Ord,
    Hash,
    Default,
}
```
