//! The phon front door — the only crate (besides `phon-codegen`) that depends on
//! facet.
//!
//! This is where Rust types become phon: facet metadata is turned into a
//! [`schema`] and a [`descriptor`], and the typed `encode::<T>` / `decode::<T>`
//! API wraps the engine. With the `jit` feature on, the typed API routes through
//! `phon-jit` when the lowered program is supported by the native backend; with
//! it off, or for ops the native backend does not compile yet, it runs the
//! `phon-engine` interpreter — same results, different speed
//! (`r[crates.jit-opt-in]`).
//!
//! Spec: `docs/content/spec.md` — "Crates and packages" and "Rust".

pub use phon_schema as schema_contract;

pub mod derive;

/// The ergonomic typed API: `encode::<T>` and `decode::<T>`, resolving thunk
/// bindings and selecting interpreter vs. JIT.
///
/// Spec: `r[exec.interpreter-baseline]`, `r[exec.jit-optional]`.
pub mod api {
    use std::marker::PhantomData;
    use std::mem::MaybeUninit;

    use facet::Facet;
    use phon_engine::{CompactError, Registry, typed};
    use phon_ir::{Lowered, MemOp};
    use phon_schema::DecodeError;

    use crate::derive::{self, DeriveError};

    /// Error type for the ergonomic typed API.
    #[derive(Debug)]
    pub enum Error {
        /// Facet metadata could not be lowered into phon's schema/descriptor model.
        Derive(DeriveError),
        /// Lowering, encoding, or decoding failed in the engine.
        Compact(CompactError),
    }

    impl From<DeriveError> for Error {
        fn from(value: DeriveError) -> Self {
            Error::Derive(value)
        }
    }

    impl From<CompactError> for Error {
        fn from(value: CompactError) -> Self {
            Error::Compact(value)
        }
    }

    impl From<DecodeError> for Error {
        fn from(value: DecodeError) -> Self {
            Error::Compact(CompactError::Decode(value))
        }
    }

    impl core::fmt::Display for Error {
        fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
            match self {
                Error::Derive(e) => write!(f, "{e}"),
                Error::Compact(e) => write!(f, "{e}"),
            }
        }
    }

    impl std::error::Error for Error {
        fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
            match self {
                Error::Derive(e) => Some(e),
                Error::Compact(e) => Some(e),
            }
        }
    }

    /// A derived, lowered typed codec for `T`.
    ///
    /// Build it once and reuse it to avoid re-deriving schemas or recompiling the
    /// JIT on every message.
    pub struct Codec<T> {
        lowered: Lowered,
        #[cfg(all(feature = "jit", target_os = "macos", target_arch = "aarch64"))]
        native_decode: Option<phon_jit::native::NativeDecode>,
        #[cfg(all(feature = "jit", target_os = "macos", target_arch = "aarch64"))]
        native_encode: Option<phon_jit::native::NativeEncode>,
        _marker: PhantomData<fn() -> T>,
    }

    impl<'facet, T> Codec<T>
    where
        T: Facet<'facet>,
    {
        /// Derive `T`, lower it to memory IR, and compile the native JIT when the
        /// current target and program shape support it.
        ///
        /// # Errors
        /// [`Error`] if deriving or lowering `T` fails.
        pub fn new() -> Result<Self, Error> {
            let derived = derive::of::<T>()?;
            let reg = Registry::new(derived.schemas.clone());
            let lowered =
                typed::lower_typed(&derived.descriptor, &derived.descriptor_blocks, &reg)?;

            #[cfg(all(feature = "jit", target_os = "macos", target_arch = "aarch64"))]
            {
                let native_decode = native_decode_supported(&lowered)
                    .then(|| phon_jit::native::NativeDecode::compile(&lowered.program));
                let native_encode = native_encode_supported(&lowered)
                    .then(|| phon_jit::native::NativeEncode::compile(&lowered.program));
                Ok(Codec {
                    lowered,
                    native_decode,
                    native_encode,
                    _marker: PhantomData,
                })
            }

            #[cfg(not(all(feature = "jit", target_os = "macos", target_arch = "aarch64")))]
            {
                Ok(Codec {
                    lowered,
                    _marker: PhantomData,
                })
            }
        }

        #[cfg(test)]
        pub(crate) fn decode_uses_native_jit(&self) -> bool {
            #[cfg(all(feature = "jit", target_os = "macos", target_arch = "aarch64"))]
            {
                return self.native_decode.is_some();
            }
            #[cfg(not(all(feature = "jit", target_os = "macos", target_arch = "aarch64")))]
            {
                false
            }
        }

        #[cfg(test)]
        pub(crate) fn encode_uses_native_jit(&self) -> bool {
            #[cfg(all(feature = "jit", target_os = "macos", target_arch = "aarch64"))]
            {
                return self.native_encode.is_some();
            }
            #[cfg(not(all(feature = "jit", target_os = "macos", target_arch = "aarch64")))]
            {
                false
            }
        }

        /// Encode `value` into compact phon bytes.
        ///
        /// # Errors
        /// This currently cannot fail after construction; it returns `Result` so
        /// the one-shot API has one error surface for encode and decode.
        pub fn encode(&self, value: &T) -> Result<Vec<u8>, Error> {
            let base = core::ptr::from_ref(value).cast::<u8>();
            #[cfg(all(feature = "jit", target_os = "macos", target_arch = "aarch64"))]
            {
                if let Some(jit) = &self.native_encode {
                    return Ok(unsafe { jit.run(base) });
                }
            }
            Ok(unsafe { typed::encode_with(&self.lowered, base) })
        }

        /// Decode compact phon bytes into `T`.
        ///
        /// Borrowed leaves in `T` borrow from `bytes`, so the input must outlive
        /// the decoded value.
        ///
        /// # Errors
        /// [`Error`] if the wire bytes are malformed or have trailing data.
        pub fn decode(&self, bytes: &'facet [u8]) -> Result<T, Error> {
            let mut slot = MaybeUninit::<T>::uninit();
            #[cfg(all(feature = "jit", target_os = "macos", target_arch = "aarch64"))]
            {
                if let Some(jit) = &self.native_decode {
                    unsafe { jit.run(bytes, slot.as_mut_ptr().cast::<u8>()) }?;
                    return Ok(unsafe { slot.assume_init() });
                }
            }
            unsafe { typed::decode_with(&self.lowered, bytes, slot.as_mut_ptr().cast::<u8>()) }?;
            Ok(unsafe { slot.assume_init() })
        }
    }

    /// Encode a typed value using a freshly built [`Codec`].
    ///
    /// # Errors
    /// [`Error`] if deriving/lowering or encoding fails.
    pub fn encode<'facet, T>(value: &T) -> Result<Vec<u8>, Error>
    where
        T: Facet<'facet>,
    {
        Codec::<T>::new()?.encode(value)
    }

    /// Decode compact phon bytes using a freshly built [`Codec`].
    ///
    /// # Errors
    /// [`Error`] if deriving/lowering or decoding fails.
    pub fn decode<'facet, T>(bytes: &'facet [u8]) -> Result<T, Error>
    where
        T: Facet<'facet>,
    {
        Codec::<T>::new()?.decode(bytes)
    }

    #[cfg(all(feature = "jit", target_os = "macos", target_arch = "aarch64"))]
    fn native_decode_supported(lowered: &Lowered) -> bool {
        lowered.blocks.is_empty() && decode_program_supported(&lowered.program)
    }

    #[cfg(all(feature = "jit", target_os = "macos", target_arch = "aarch64"))]
    fn native_encode_supported(lowered: &Lowered) -> bool {
        lowered.blocks.is_empty() && encode_program_supported(&lowered.program)
    }

    #[cfg(all(feature = "jit", target_os = "macos", target_arch = "aarch64"))]
    fn decode_program_supported(program: &[MemOp]) -> bool {
        program.iter().all(|op| match op {
            MemOp::Scalar { .. }
            | MemOp::Bytes(_)
            | MemOp::Borrow(_)
            | MemOp::Default(_)
            | MemOp::SkipWire(_) => true,
            MemOp::Sequence(s) => decode_program_supported(&s.element),
            MemOp::Option(o) => decode_program_supported(&o.some),
            MemOp::Enum(e) => e
                .variants
                .iter()
                .all(|variant| decode_program_supported(&variant.payload)),
            MemOp::Map(m) => decode_program_supported(&m.key) && decode_program_supported(&m.value),
            MemOp::Result(_)
            | MemOp::Dynamic { .. }
            | MemOp::Opaque(_)
            | MemOp::CallBlock { .. } => false,
        })
    }

    #[cfg(all(feature = "jit", target_os = "macos", target_arch = "aarch64"))]
    fn encode_program_supported(program: &[MemOp]) -> bool {
        program.iter().all(|op| match op {
            MemOp::Scalar { .. } | MemOp::Bytes(_) | MemOp::Borrow(_) => true,
            MemOp::Sequence(s) => encode_program_supported(&s.element),
            MemOp::Option(o) => encode_program_supported(&o.some),
            MemOp::Enum(e) => e
                .variants
                .iter()
                .all(|variant| encode_program_supported(&variant.payload)),
            MemOp::Map(m) => encode_program_supported(&m.key) && encode_program_supported(&m.value),
            MemOp::SkipWire(_)
            | MemOp::Default(_)
            | MemOp::Result(_)
            | MemOp::Dynamic { .. }
            | MemOp::Opaque(_)
            | MemOp::CallBlock { .. } => false,
        })
    }
}

/// phon's dynamic value, re-exported for convenience at the front door. It *is*
/// `facet_value::Value` (see `phon_schema::value`); there is no separate phon
/// value type and no conversion between them.
///
/// Spec: "Value" (`r[value]`).
pub mod value {
    pub use phon_schema::value::Value;
}

#[cfg(test)]
mod tests {
    use facet::Facet;

    use crate::api;

    #[derive(Debug, PartialEq, Facet)]
    struct ApiMsg {
        id: u64,
        items: Vec<u32>,
    }

    #[test]
    fn api_roundtrips_and_reports_supported_backend() {
        let codec = api::Codec::<ApiMsg>::new().unwrap();
        #[cfg(all(feature = "jit", target_os = "macos", target_arch = "aarch64"))]
        {
            assert!(codec.decode_uses_native_jit());
            assert!(codec.encode_uses_native_jit());
        }
        #[cfg(not(all(feature = "jit", target_os = "macos", target_arch = "aarch64")))]
        {
            assert!(!codec.decode_uses_native_jit());
            assert!(!codec.encode_uses_native_jit());
        }

        let msg = ApiMsg {
            id: 0xCAFE_F00D,
            items: vec![1, 2, 3, 5, 8],
        };
        let bytes = codec.encode(&msg).unwrap();
        let back = codec.decode(&bytes).unwrap();
        assert_eq!(back, msg);
    }

    #[derive(Debug, PartialEq, Facet)]
    struct ApiResultMsg {
        value: Result<u32, u32>,
    }

    #[test]
    fn api_falls_back_for_uncompiled_ops() {
        let codec = api::Codec::<ApiResultMsg>::new().unwrap();
        assert!(!codec.decode_uses_native_jit());
        assert!(!codec.encode_uses_native_jit());

        let msg = ApiResultMsg { value: Ok(0xABCD) };
        let bytes = api::encode(&msg).unwrap();
        let back: ApiResultMsg = codec.decode(&bytes).unwrap();
        assert_eq!(back, msg);
    }
}
