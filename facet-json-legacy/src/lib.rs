#![warn(missing_docs)]
// Allow unsafe code when cranelift feature is enabled (required for JIT compilation)
#![cfg_attr(not(feature = "cranelift"), deny(unsafe_code))]
#![doc = include_str!("../README.md")]

extern crate alloc;

// Re-export span types from facet-reflect
pub use facet_reflect::{Span, Spanned};

mod deserialize;
pub use deserialize::{
    JsonDeserializer, JsonError, JsonErrorKind, from_slice, from_slice_borrowed, from_str,
    from_str_borrowed,
};

mod serialize;
pub use serialize::*;

mod scanner;
pub use scanner::{
    NumberHint, ScanError, ScanErrorKind, Scanner, SpannedToken, Token as ScanToken,
};

mod adapter;
pub use adapter::{
    AdapterError, AdapterErrorKind, SliceAdapter, SpannedAdapterToken, Token as AdapterToken,
    TokenSource,
};

#[cfg(feature = "streaming")]
mod streaming;
#[cfg(feature = "futures-io")]
pub use streaming::from_async_reader_futures;
#[cfg(feature = "tokio")]
pub use streaming::from_async_reader_tokio;
#[cfg(feature = "streaming")]
pub use streaming::{StreamingAdapter, from_reader};

mod scan_buffer;
pub use scan_buffer::ScanBuffer;

#[cfg(feature = "std")]
mod reader;
#[cfg(feature = "tokio")]
pub use reader::AsyncJsonReader;
#[cfg(feature = "futures-io")]
pub use reader::FuturesJsonReader;
#[cfg(feature = "std")]
pub use reader::{JsonReader, JsonToken, ReaderError, SpannedJsonToken};

mod raw_json;
pub use raw_json::RawJson;

mod json;
pub use json::Json;

/// JIT-compiled JSON deserialization using Cranelift.
///
/// This module provides a JIT-compiled JSON deserializer that generates native
/// code specialized for each type's exact memory layout. On first call for a type,
/// it compiles a specialized deserializer using Cranelift. Subsequent calls use
/// the cached native code directly.
///
/// # Example
///
/// ```ignore
/// use facet::Facet;
/// use facet_json_legacy::cranelift;
///
/// #[derive(Facet)]
/// struct Point { x: f64, y: f64 }
///
/// let point: Point = cranelift::from_str(r#"{"x": 1.0, "y": 2.0}"#).unwrap();
/// ```
#[cfg(feature = "cranelift")]
pub mod cranelift;

#[cfg(feature = "axum")]
mod axum;
#[cfg(feature = "axum")]
pub use self::axum::JsonRejection;

/// Re-export the `Write` trait from facet-core for backwards compatibility.
///
/// This trait is used by the JSON serializer to write output without depending
/// on `std::io::Write`, enabling `no_std` support.
pub use facet_core::Write as JsonWrite;

/// Properly escapes and writes a JSON string
#[inline]
fn write_json_string<W: JsonWrite>(writer: &mut W, s: &str) {
    // Just a little bit of text on how it works. There are two main steps:
    // 1. Check if the string is completely ASCII and doesn't contain any quotes or backslashes or
    //    control characters. This is the fast path, because it means that the bytes can be written
    //    as they are, without any escaping needed. In this case we go over the string in windows
    //    of 16 bytes (which is completely arbitrary, maybe find some real world data to tune this
    //    with? I don't know and you don't have to do this dear reader.) and we just feed them into
    //    the writer.
    // 2. If the string is not completely ASCII or contains quotes or backslashes or control
    //    characters, we need to escape them. This is the slow path, because it means that we need
    //    to write the bytes one by one, and we need to figure out where to put the escapes. So we
    //    just call `write_json_escaped_char` for each character.

    const STEP_SIZE: usize = Window::BITS as usize / 8;
    type Window = u128;
    type Chunk = [u8; STEP_SIZE];

    writer.write(b"\"");

    let mut s = s;
    while let Some(Ok(chunk)) = s.as_bytes().get(..STEP_SIZE).map(Chunk::try_from) {
        let window = Window::from_ne_bytes(chunk);
        // Our window is a concatenation of u8 values. For each value, we need to make sure that:
        // 1. It is ASCII (i.e. the first bit of the u8 is 0, so u8 & 0x80 == 0)
        // 2. It does not contain quotes (i.e. 0x22)
        // 3. It does not contain backslashes (i.e. 0x5c)
        // 4. It does not contain control characters (i.e. characters below 32, including 0)
        //    This means the bit above the 1st, 2nd or 3rd bit must be set, so u8 & 0xe0 != 0
        let completely_ascii = window & 0x80808080808080808080808080808080 == 0;
        let quote_free = !contains_0x22(window);
        let backslash_free = !contains_0x5c(window);
        let control_char_free = top_three_bits_set(window);
        if completely_ascii && quote_free && backslash_free && control_char_free {
            // Yay! Whack it into the writer!
            writer.write(&chunk);
            s = &s[STEP_SIZE..];
        } else {
            // Ahw one of the conditions not met. Let's take our time and artisanally handle each
            // character.
            let mut chars = s.chars();
            let mut count = STEP_SIZE;
            for c in &mut chars {
                write_json_escaped_char(writer, c);
                count = count.saturating_sub(c.len_utf8());
                if count == 0 {
                    // Done with our chunk
                    break;
                }
            }
            s = chars.as_str();
        }
    }

    // In our loop we checked that we were able to consume at least `STEP_SIZE` bytes every
    // iteration. That means there might be a small remnant at the end that we can handle in the
    // slow method.
    for c in s.chars() {
        write_json_escaped_char(writer, c);
    }

    writer.write(b"\"")
}

/// Writes a single JSON escaped character
#[inline]
fn write_json_escaped_char<W: JsonWrite>(writer: &mut W, c: char) {
    match c {
        '"' => writer.write(b"\\\""),
        '\\' => writer.write(b"\\\\"),
        '\n' => writer.write(b"\\n"),
        '\r' => writer.write(b"\\r"),
        '\t' => writer.write(b"\\t"),
        '\u{08}' => writer.write(b"\\b"),
        '\u{0C}' => writer.write(b"\\f"),
        c if c.is_ascii_control() => {
            let code_point = c as u32;
            // Extract individual hex digits (nibbles) from the code point
            let to_hex = |d: u32| char::from_digit(d, 16).unwrap() as u8;
            let buf = [
                b'\\',
                b'u',
                to_hex((code_point >> 12) & 0xF),
                to_hex((code_point >> 8) & 0xF),
                to_hex((code_point >> 4) & 0xF),
                to_hex(code_point & 0xF),
            ];
            writer.write(&buf);
        }
        c if c.is_ascii() => {
            writer.write(&[c as u8]);
        }
        c => {
            let mut buf = [0; 4];
            let len = c.encode_utf8(&mut buf).len();
            writer.write(&buf[..len])
        }
    }
}

#[inline]
fn contains_0x22(val: u128) -> bool {
    let xor_result = val ^ 0x22222222222222222222222222222222;
    let has_zero = (xor_result.wrapping_sub(0x01010101010101010101010101010101))
        & !xor_result
        & 0x80808080808080808080808080808080;
    has_zero != 0
}

#[inline]
fn contains_0x5c(val: u128) -> bool {
    let xor_result = val ^ 0x5c5c5c5c5c5c5c5c5c5c5c5c5c5c5c5c;
    let has_zero = (xor_result.wrapping_sub(0x01010101010101010101010101010101))
        & !xor_result
        & 0x80808080808080808080808080808080;
    has_zero != 0
}

/// For each of the 16 u8s that make up a u128, check if the top three bits are set.
#[inline]
fn top_three_bits_set(value: u128) -> bool {
    let xor_result = value & 0xe0e0e0e0e0e0e0e0e0e0e0e0e0e0e0e0;
    let has_zero = (xor_result.wrapping_sub(0x01010101010101010101010101010101))
        & !xor_result
        & 0x80808080808080808080808080808080;
    has_zero == 0
}

// ============================================================================
// Test helpers for multi-mode testing
// ============================================================================

/// Macro to run deserialize tests in multiple modes (slice, streaming, cranelift).
///
/// Generates test modules that run the same test body with different
/// deserializers:
/// - `slice` - uses `from_str` (slice-based parsing) - always enabled
/// - `streaming` - uses `from_reader` (streaming from a reader) - requires `streaming` feature
/// - `cranelift` - uses `cranelift::from_str` (JIT-compiled) - requires `cranelift` feature
///
/// # Usage
///
/// ```ignore
/// facet_json_legacy::test_modes! {
///     #[test]
///     fn my_test() {
///         #[derive(facet::Facet, Debug, PartialEq)]
///         struct Foo { x: i32 }
///
///         let result: Foo = deserialize(r#"{"x": 42}"#).unwrap();
///         assert_eq!(result, Foo { x: 42 });
///     }
/// }
/// ```
///
/// The macro provides a `deserialize` function that takes `&str` and returns
/// `Result<T, JsonError>`.
///
/// # Skipping streaming mode
///
/// Use `#[skip_streaming]` to skip a test in streaming mode
/// (for features like `#[facet(flatten)]` or `RawJson` that aren't supported):
///
/// ```ignore
/// facet_json_legacy::test_modes! {
///     #[skip_streaming]
///     #[test]
///     fn test_flatten() {
///         // This test only runs in slice and cranelift modes
///     }
/// }
/// ```
// =============================================================================
// Case 1: Both streaming and cranelift features enabled
// =============================================================================
#[macro_export]
#[cfg(all(feature = "streaming", feature = "cranelift"))]
macro_rules! test_modes {
    ($($content:tt)*) => {
        /// Tests using from_str (slice-based)
        mod slice {
            #[allow(unused_imports)]
            use super::*;

            #[allow(dead_code)]
            fn deserialize<T: ::facet::Facet<'static>>(input: &str) -> Result<T, $crate::JsonError> {
                $crate::from_str(input)
            }

            $crate::__test_modes_inner!(@skip_none $($content)*);
        }

        /// Tests using from_reader (streaming)
        mod streaming {
            #[allow(unused_imports)]
            use super::*;

            #[allow(dead_code)]
            fn deserialize<T: ::facet::Facet<'static>>(input: &str) -> Result<T, $crate::JsonError> {
                $crate::from_reader(::std::io::Cursor::new(input))
            }

            $crate::__test_modes_inner!(@skip_streaming $($content)*);
        }

        /// Tests using JIT-compiled deserializer
        mod cranelift {
            #[allow(unused_imports)]
            use super::*;

            #[allow(dead_code)]
            fn deserialize<T: ::facet::Facet<'static>>(input: &str) -> Result<T, $crate::JsonError> {
                $crate::cranelift::from_str(input)
            }

            $crate::__test_modes_inner!(@skip_none $($content)*);
        }
    };
}

#[macro_export]
#[cfg(all(feature = "streaming", feature = "cranelift"))]
#[doc(hidden)]
macro_rules! __test_modes_inner {
    // Skip #[skip_streaming] when in streaming mode
    (@skip_streaming #[skip_streaming] #[test] fn $name:ident() $body:block $($rest:tt)*) => {
        $crate::__test_modes_inner!(@skip_streaming $($rest)*);
    };

    // In non-streaming modes, ignore the #[skip_streaming] attribute
    (@skip_none #[skip_streaming] #[test] fn $name:ident() $body:block $($rest:tt)*) => {
        #[test]
        fn $name() {
            ::facet_testhelpers::setup();
            $body
        }
        $crate::__test_modes_inner!(@skip_none $($rest)*);
    };

    // Regular test - emit it
    (@$mode:ident #[test] fn $name:ident() $body:block $($rest:tt)*) => {
        #[test]
        fn $name() {
            ::facet_testhelpers::setup();
            $body
        }
        $crate::__test_modes_inner!(@$mode $($rest)*);
    };

    // Base case - done
    (@$mode:ident) => {};
}

// =============================================================================
// Case 2: Only streaming feature enabled (no cranelift)
// =============================================================================
/// See the main `test_modes` documentation above.
#[macro_export]
#[cfg(all(feature = "streaming", not(feature = "cranelift")))]
macro_rules! test_modes {
    ($($content:tt)*) => {
        /// Tests using from_str (slice-based)
        mod slice {
            #[allow(unused_imports)]
            use super::*;

            #[allow(dead_code)]
            fn deserialize<T: ::facet::Facet<'static>>(input: &str) -> Result<T, $crate::JsonError> {
                $crate::from_str(input)
            }

            $crate::__test_modes_inner!(@skip_none $($content)*);
        }

        /// Tests using from_reader (streaming)
        mod streaming {
            #[allow(unused_imports)]
            use super::*;

            #[allow(dead_code)]
            fn deserialize<T: ::facet::Facet<'static>>(input: &str) -> Result<T, $crate::JsonError> {
                $crate::from_reader(::std::io::Cursor::new(input))
            }

            $crate::__test_modes_inner!(@skip_streaming $($content)*);
        }
    };
}

#[macro_export]
#[cfg(all(feature = "streaming", not(feature = "cranelift")))]
#[doc(hidden)]
macro_rules! __test_modes_inner {
    // Skip #[skip_streaming] when in streaming mode
    (@skip_streaming #[skip_streaming] #[test] fn $name:ident() $body:block $($rest:tt)*) => {
        $crate::__test_modes_inner!(@skip_streaming $($rest)*);
    };

    // In non-streaming modes, ignore the #[skip_streaming] attribute
    (@skip_none #[skip_streaming] #[test] fn $name:ident() $body:block $($rest:tt)*) => {
        #[test]
        fn $name() {
            ::facet_testhelpers::setup();
            $body
        }
        $crate::__test_modes_inner!(@skip_none $($rest)*);
    };

    // Regular test - emit it
    (@$mode:ident #[test] fn $name:ident() $body:block $($rest:tt)*) => {
        #[test]
        fn $name() {
            ::facet_testhelpers::setup();
            $body
        }
        $crate::__test_modes_inner!(@$mode $($rest)*);
    };

    // Base case - done
    (@$mode:ident) => {};
}

// =============================================================================
// Case 3: Only cranelift feature enabled (no streaming)
// =============================================================================
/// See the main `test_modes` documentation above.
#[macro_export]
#[cfg(all(not(feature = "streaming"), feature = "cranelift"))]
macro_rules! test_modes {
    ($($content:tt)*) => {
        /// Tests using from_str (slice-based)
        mod slice {
            #[allow(unused_imports)]
            use super::*;

            #[allow(dead_code)]
            fn deserialize<T: ::facet::Facet<'static>>(input: &str) -> Result<T, $crate::JsonError> {
                $crate::from_str(input)
            }

            $crate::__test_modes_inner!(@skip_none $($content)*);
        }

        /// Tests using JIT-compiled deserializer
        mod cranelift {
            #[allow(unused_imports)]
            use super::*;

            #[allow(dead_code)]
            fn deserialize<T: ::facet::Facet<'static>>(input: &str) -> Result<T, $crate::JsonError> {
                $crate::cranelift::from_str(input)
            }

            $crate::__test_modes_inner!(@skip_none $($content)*);
        }
    };
}

#[macro_export]
#[cfg(all(not(feature = "streaming"), feature = "cranelift"))]
#[doc(hidden)]
macro_rules! __test_modes_inner {
    // Ignore #[skip_streaming] when not in streaming mode
    (@skip_none #[skip_streaming] #[test] fn $name:ident() $body:block $($rest:tt)*) => {
        #[test]
        fn $name() {
            ::facet_testhelpers::setup();
            $body
        }
        $crate::__test_modes_inner!(@skip_none $($rest)*);
    };

    // Regular test - emit it
    (@$mode:ident #[test] fn $name:ident() $body:block $($rest:tt)*) => {
        #[test]
        fn $name() {
            ::facet_testhelpers::setup();
            $body
        }
        $crate::__test_modes_inner!(@$mode $($rest)*);
    };

    // Base case - done
    (@$mode:ident) => {};
}

// =============================================================================
// Case 4: Neither streaming nor cranelift features enabled (slice only)
// =============================================================================
/// See the main `test_modes` documentation above.
#[macro_export]
#[cfg(all(not(feature = "streaming"), not(feature = "cranelift")))]
macro_rules! test_modes {
    ($($content:tt)*) => {
        /// Tests using from_str (slice-based)
        mod slice {
            #[allow(unused_imports)]
            use super::*;

            #[allow(dead_code)]
            fn deserialize<T: ::facet::Facet<'static>>(input: &str) -> Result<T, $crate::JsonError> {
                $crate::from_str(input)
            }

            $crate::__test_modes_inner!(@skip_none $($content)*);
        }
    };
}

#[macro_export]
#[cfg(all(not(feature = "streaming"), not(feature = "cranelift")))]
#[doc(hidden)]
macro_rules! __test_modes_inner {
    // Ignore #[skip_streaming] in non-streaming build
    (@skip_none #[skip_streaming] #[test] fn $name:ident() $body:block $($rest:tt)*) => {
        #[test]
        fn $name() {
            ::facet_testhelpers::setup();
            $body
        }
        $crate::__test_modes_inner!(@skip_none $($rest)*);
    };

    // Regular test
    (@$mode:ident #[test] fn $name:ident() $body:block $($rest:tt)*) => {
        #[test]
        fn $name() {
            ::facet_testhelpers::setup();
            $body
        }
        $crate::__test_modes_inner!(@$mode $($rest)*);
    };

    // Base case
    (@$mode:ident) => {};
}
