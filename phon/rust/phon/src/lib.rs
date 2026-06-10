//! The phon front door — the only crate (besides `phon-codegen`) that depends on
//! facet (`r[crates.concern-separation]`).
//!
//! This is where Rust types become phon: facet metadata is turned into a
//! `schema` and `descriptor`, and the typed `encode::<T>` / `decode::<T>`
//! API wraps the engine. With the `jit` feature on, the typed API routes through
//! `phon-jit` when the lowered program is supported by the native backend; with
//! it off, or for ops the native backend does not compile yet, it runs the
//! `phon-engine` interpreter — same results, different speed
//! (`r[crates.jit-opt-in]`).
//!
//! Spec: `docs/content/spec.md` — "Crates and packages" and "Rust".

// r[impl crates.concern-separation]
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
    use phon_ir::Lowered;
    #[cfg(all(feature = "jit", target_os = "macos", target_arch = "aarch64"))]
    use phon_ir::MemOp;
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

    /// One diagnostic record explaining why a subtree is not handled by the
    /// native JIT and therefore falls back to the interpreter.
    #[derive(Clone, Debug, PartialEq, Eq)]
    pub struct JitFallbackRecord {
        pub path: String,
        pub reason: &'static str,
    }

    /// Diagnostic report for the optional native JIT. It is a development aid,
    /// not an execution mode: encode/decode selection is unchanged.
    // r[impl exec.strict-recording]
    #[derive(Clone, Debug, Default, PartialEq, Eq)]
    pub struct JitFallbackReport {
        pub decode: Vec<JitFallbackRecord>,
        pub encode: Vec<JitFallbackRecord>,
    }

    /// One fallback record scoped to a Vox method root.
    #[derive(Clone, Debug, PartialEq, Eq)]
    pub struct MethodJitFallbackRecord {
        pub method: String,
        pub phase: String,
        pub direction: &'static str,
        pub path: String,
        pub reason: &'static str,
    }

    /// Method-scoped fallback report for service-surface audits.
    #[derive(Clone, Debug, Default, PartialEq, Eq)]
    pub struct MethodJitFallbackReport {
        pub records: Vec<MethodJitFallbackRecord>,
    }

    impl MethodJitFallbackReport {
        pub fn is_empty(&self) -> bool {
            self.records.is_empty()
        }
    }

    impl JitFallbackReport {
        pub fn is_empty(&self) -> bool {
            self.decode.is_empty() && self.encode.is_empty()
        }

        pub fn scoped(
            self,
            method: impl Into<String>,
            phase: impl Into<String>,
        ) -> MethodJitFallbackReport {
            let method = method.into();
            let phase = phase.into();
            let mut records = Vec::with_capacity(self.decode.len() + self.encode.len());

            records.extend(
                self.decode
                    .into_iter()
                    .map(|record| MethodJitFallbackRecord {
                        method: method.clone(),
                        phase: phase.clone(),
                        direction: "decode",
                        path: record.path,
                        reason: record.reason,
                    }),
            );
            records.extend(
                self.encode
                    .into_iter()
                    .map(|record| MethodJitFallbackRecord {
                        method: method.clone(),
                        phase: phase.clone(),
                        direction: "encode",
                        path: record.path,
                        reason: record.reason,
                    }),
            );

            MethodJitFallbackReport { records }
        }

        #[cfg(not(all(feature = "jit", target_os = "macos", target_arch = "aarch64")))]
        fn unavailable(reason: &'static str) -> Self {
            Self {
                decode: vec![JitFallbackRecord {
                    path: "$".to_string(),
                    reason,
                }],
                encode: vec![JitFallbackRecord {
                    path: "$".to_string(),
                    reason,
                }],
            }
        }
    }

    /// A derived, lowered typed codec for `T`.
    ///
    /// Build it once and reuse it to avoid re-deriving schemas or recompiling the
    /// JIT on every message.
    // r[impl crates.jit-opt-in]
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
                    .then(|| phon_jit::native::NativeDecode::compile_lowered(&lowered));
                let native_encode = native_encode_supported(&lowered)
                    .then(|| phon_jit::native::NativeEncode::compile_lowered(&lowered));
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
                self.native_decode.is_some()
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
                self.native_encode.is_some()
            }
            #[cfg(not(all(feature = "jit", target_os = "macos", target_arch = "aarch64")))]
            {
                false
            }
        }

        /// Report the subtrees that make this codec fall back from the native JIT.
        ///
        /// This is strict-recording diagnostics only. It does not change whether
        /// encode/decode run with the native JIT or the interpreter.
        pub fn jit_fallback_report(&self) -> JitFallbackReport {
            let report = jit_fallback_report_for_lowered(&self.lowered);
            #[cfg(all(feature = "jit", target_os = "macos", target_arch = "aarch64"))]
            {
                let mut report = report;
                if self.native_decode.is_none() && report.decode.is_empty() {
                    report.decode.push(JitFallbackRecord {
                        path: "$".to_string(),
                        reason: "native decode JIT was not compiled for this program",
                    });
                }
                if self.native_encode.is_none() && report.encode.is_empty() {
                    report.encode.push(JitFallbackRecord {
                        path: "$".to_string(),
                        reason: "native encode JIT was not compiled for this program",
                    });
                }
                report
            }
            #[cfg(not(all(feature = "jit", target_os = "macos", target_arch = "aarch64")))]
            {
                report
            }
        }

        /// Encode `value` into compact phon bytes.
        ///
        /// # Errors
        /// This currently cannot fail after construction; it returns `Result` so
        /// the one-shot API has one error surface for encode and decode.
        // r[impl typed.no-dynamic-bounce]
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
        // r[impl typed.no-dynamic-bounce]
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
        decode_program_supported(&lowered.program)
            && lowered
                .blocks
                .values()
                .all(|block| decode_program_supported(block))
    }

    #[cfg(all(feature = "jit", target_os = "macos", target_arch = "aarch64"))]
    fn record_decode_fallbacks(program: &[MemOp], path: &str, out: &mut Vec<JitFallbackRecord>) {
        for (idx, op) in program.iter().enumerate() {
            let op_path = format!("{path}.{idx}");
            if let MemOp::NativeInt { .. } = op {
                out.push(JitFallbackRecord {
                    path: op_path,
                    reason: "native decode JIT does not support native-sized integer casts yet",
                });
            }
        }
        walk_nested_programs(program, path, out, record_decode_fallbacks);
    }

    #[cfg(all(feature = "jit", target_os = "macos", target_arch = "aarch64"))]
    fn record_encode_fallbacks(program: &[MemOp], path: &str, out: &mut Vec<JitFallbackRecord>) {
        for (idx, op) in program.iter().enumerate() {
            let op_path = format!("{path}.{idx}");
            match op {
                MemOp::NativeInt { .. } => out.push(JitFallbackRecord {
                    path: op_path,
                    reason: "native encode JIT does not support native-sized integer casts yet",
                }),
                MemOp::SkipWire(_) => out.push(JitFallbackRecord {
                    path: op_path,
                    reason: "native encode JIT cannot emit decode-only skip-wire ops",
                }),
                MemOp::Default(_) => out.push(JitFallbackRecord {
                    path: op_path,
                    reason: "native encode JIT cannot emit decode-only default ops",
                }),
                _ => {}
            }
        }
        walk_nested_programs(program, path, out, record_encode_fallbacks);
    }

    #[cfg(all(feature = "jit", target_os = "macos", target_arch = "aarch64"))]
    fn walk_nested_programs(
        program: &[MemOp],
        path: &str,
        out: &mut Vec<JitFallbackRecord>,
        visit: fn(&[MemOp], &str, &mut Vec<JitFallbackRecord>),
    ) {
        for (idx, op) in program.iter().enumerate() {
            let op_path = format!("{path}.{idx}");
            match op {
                MemOp::Sequence(seq) => visit(&seq.element, &format!("{op_path}.element"), out),
                MemOp::Set(set) => visit(&set.element, &format!("{op_path}.element"), out),
                MemOp::Option(option) => visit(&option.some, &format!("{op_path}.some"), out),
                MemOp::Enum(en) => {
                    for variant in &en.variants {
                        visit(
                            &variant.payload,
                            &format!("{op_path}.variant[{}]", variant.wire_index),
                            out,
                        );
                    }
                }
                MemOp::Map(map) => {
                    visit(&map.key, &format!("{op_path}.key"), out);
                    visit(&map.value, &format!("{op_path}.value"), out);
                }
                MemOp::Result(result) => {
                    visit(&result.ok, &format!("{op_path}.ok"), out);
                    visit(&result.err, &format!("{op_path}.err"), out);
                }
                MemOp::Pointer(pointer) => {
                    visit(&pointer.pointee, &format!("{op_path}.pointee"), out);
                }
                _ => {}
            }
        }
    }

    #[cfg(all(feature = "jit", target_os = "macos", target_arch = "aarch64"))]
    fn native_encode_supported(lowered: &Lowered) -> bool {
        encode_program_supported(&lowered.program)
            && lowered
                .blocks
                .values()
                .all(|block| encode_program_supported(block))
    }

    #[cfg(all(feature = "jit", target_os = "macos", target_arch = "aarch64"))]
    fn decode_program_supported(program: &[MemOp]) -> bool {
        program.iter().all(|op| match op {
            MemOp::Scalar { .. }
            | MemOp::Bytes(_)
            | MemOp::Borrow(_)
            | MemOp::Default(_)
            | MemOp::SkipWire(_) => true,
            MemOp::NativeInt { .. } => false,
            MemOp::Sequence(s) => decode_program_supported(&s.element),
            MemOp::Set(s) => decode_program_supported(&s.element),
            MemOp::Option(o) => decode_program_supported(&o.some),
            MemOp::Enum(e) => e
                .variants
                .iter()
                .all(|variant| decode_program_supported(&variant.payload)),
            MemOp::Map(m) => decode_program_supported(&m.key) && decode_program_supported(&m.value),
            MemOp::Result(r) => decode_program_supported(&r.ok) && decode_program_supported(&r.err),
            MemOp::Pointer(p) => decode_program_supported(&p.pointee),
            MemOp::Opaque(_) | MemOp::Dynamic { .. } | MemOp::CallBlock { .. } => true,
        })
    }

    #[cfg(all(feature = "jit", target_os = "macos", target_arch = "aarch64"))]
    fn encode_program_supported(program: &[MemOp]) -> bool {
        program.iter().all(|op| match op {
            MemOp::Scalar { .. } | MemOp::Bytes(_) | MemOp::Borrow(_) => true,
            MemOp::NativeInt { .. } => false,
            MemOp::Sequence(s) => encode_program_supported(&s.element),
            MemOp::Set(s) => encode_program_supported(&s.element),
            MemOp::Option(o) => encode_program_supported(&o.some),
            MemOp::Enum(e) => e
                .variants
                .iter()
                .all(|variant| encode_program_supported(&variant.payload)),
            MemOp::Map(m) => encode_program_supported(&m.key) && encode_program_supported(&m.value),
            MemOp::Result(r) => encode_program_supported(&r.ok) && encode_program_supported(&r.err),
            MemOp::Pointer(p) => encode_program_supported(&p.pointee),
            MemOp::SkipWire(_) | MemOp::Default(_) => false,
            MemOp::Opaque(_) | MemOp::Dynamic { .. } | MemOp::CallBlock { .. } => true,
        })
    }

    /// Record the native-JIT fallback subtrees for an already-lowered typed program.
    ///
    /// This is shared by generic [`Codec`] values and shape-erased/generated RPC
    /// bridges so unsupported op diagnostics stay in one place.
    // r[impl exec.strict-recording]
    pub fn jit_fallback_report_for_lowered(lowered: &Lowered) -> JitFallbackReport {
        #[cfg(all(feature = "jit", target_os = "macos", target_arch = "aarch64"))]
        {
            let mut report = JitFallbackReport::default();
            record_decode_fallbacks(&lowered.program, "$", &mut report.decode);
            record_encode_fallbacks(&lowered.program, "$", &mut report.encode);
            for (schema, block) in &lowered.blocks {
                let path = format!("$block[{schema}]");
                record_decode_fallbacks(block, &path, &mut report.decode);
                record_encode_fallbacks(block, &path, &mut report.encode);
            }
            report
        }
        #[cfg(not(all(feature = "jit", target_os = "macos", target_arch = "aarch64")))]
        {
            let _ = lowered;
            JitFallbackReport::unavailable("native JIT is not enabled for this build target")
        }
    }

    #[cfg(all(test, feature = "jit", target_os = "macos", target_arch = "aarch64"))]
    mod tests {
        use phon_ir::ir::{Lowered, MemOp};

        use super::*;

        // r[verify exec.strict-recording]
        // r[verify crates.jit-opt-in]
        #[test]
        fn native_int_memops_are_reported_instead_of_compiled() {
            let lowered = Lowered {
                program: vec![
                    MemOp::NativeInt {
                        offset: 0,
                        mem_size: 4,
                        signed: false,
                    },
                    MemOp::NativeInt {
                        offset: 4,
                        mem_size: 4,
                        signed: true,
                    },
                ],
                blocks: Default::default(),
            };

            assert!(!native_decode_supported(&lowered));
            assert!(!native_encode_supported(&lowered));

            let report = jit_fallback_report_for_lowered(&lowered);

            assert_eq!(
                report.decode,
                vec![
                    JitFallbackRecord {
                        path: "$.0".to_string(),
                        reason: "native decode JIT does not support native-sized integer casts yet",
                    },
                    JitFallbackRecord {
                        path: "$.1".to_string(),
                        reason: "native decode JIT does not support native-sized integer casts yet",
                    },
                ]
            );
            assert_eq!(
                report.encode,
                vec![
                    JitFallbackRecord {
                        path: "$.0".to_string(),
                        reason: "native encode JIT does not support native-sized integer casts yet",
                    },
                    JitFallbackRecord {
                        path: "$.1".to_string(),
                        reason: "native encode JIT does not support native-sized integer casts yet",
                    },
                ]
            );
        }
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

    // r[verify exec.strict-recording]
    #[test]
    fn api_roundtrips_and_reports_supported_backend() {
        let codec = api::Codec::<ApiMsg>::new().unwrap();
        #[cfg(all(feature = "jit", target_os = "macos", target_arch = "aarch64"))]
        {
            assert!(codec.decode_uses_native_jit());
            assert!(codec.encode_uses_native_jit());
            assert!(codec.jit_fallback_report().is_empty());
        }
        #[cfg(not(all(feature = "jit", target_os = "macos", target_arch = "aarch64")))]
        {
            assert!(!codec.decode_uses_native_jit());
            assert!(!codec.encode_uses_native_jit());
            let report = codec.jit_fallback_report();
            assert!(!report.decode.is_empty());
            assert!(!report.encode.is_empty());
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

    #[derive(Debug, PartialEq, Facet)]
    struct ApiNativeSizedMsg {
        count: usize,
        delta: isize,
    }

    // r[verify crates.jit-opt-in]
    // r[verify exec.jit-optional]
    #[test]
    fn api_result_uses_native_jit_when_available() {
        let codec = api::Codec::<ApiResultMsg>::new().unwrap();
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

        let msg = ApiResultMsg { value: Ok(0xABCD) };
        let bytes = api::encode(&msg).unwrap();
        let back: ApiResultMsg = codec.decode(&bytes).unwrap();
        assert_eq!(back, msg);

        let msg = ApiResultMsg { value: Err(0x1234) };
        let bytes = codec.encode(&msg).unwrap();
        let back = codec.decode(&bytes).unwrap();
        assert_eq!(back, msg);
    }

    // r[verify type-system.rust-subset]
    // r[verify crates.jit-opt-in]
    // r[verify exec.strict-recording]
    #[test]
    fn api_native_sized_ints_roundtrip_and_stay_native_clean_when_layout_matches() {
        let codec = api::Codec::<ApiNativeSizedMsg>::new().unwrap();
        #[cfg(all(feature = "jit", target_os = "macos", target_arch = "aarch64"))]
        {
            assert!(codec.decode_uses_native_jit());
            assert!(codec.encode_uses_native_jit());
            assert!(codec.jit_fallback_report().is_empty());
        }

        let msg = ApiNativeSizedMsg {
            count: 1_234,
            delta: -37,
        };
        let bytes = codec.encode(&msg).unwrap();
        let back = codec.decode(&bytes).unwrap();
        assert_eq!(back, msg);
    }
}
