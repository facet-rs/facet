use alloc::borrow::Cow;
use alloc::string::String;
use alloc::vec::Vec;
use core::hash::{Hash, Hasher};
use core::marker::PhantomData;

use facet_core::{ConstTypeId, Facet, PtrConst, ScalarType, Shape};
use weavy::ir::{ControlOp, WeavyOp};
use weavy::jit::{Chain, NativeProgram, ProgSlot, StencilLayout};

use crate::{
    ChildPlan, ExecBlock, ExecOp, HashError, HashIntrinsic, HashMode, HashPlan, ScalarFieldPlan,
    StructFieldPlan, unsupported,
};

mod stencils {
    include!(concat!(env!("OUT_DIR"), "/stencils.rs"));
}

/// Native copy-and-patch hash plan for the supported value-mode scalar subset.
pub struct NativeHashPlan<T, H = std::collections::hash_map::DefaultHasher> {
    native: NativeProgram,
    scalar_infos: Vec<ScalarInfo>,
    scalar_run_fields: Vec<Vec<ScalarInfo>>,
    scalar_run_infos: Vec<ScalarRunInfo>,
    const_usizes: Vec<usize>,
    _marker: PhantomData<fn() -> (T, H)>,
}

// SAFETY: the executable buffer and side tables are fully materialized before
// the plan is returned. `hash` only reads them and uses the caller-owned hasher.
unsafe impl<T, H> Send for NativeHashPlan<T, H> {}
// SAFETY: the executable buffer and side tables are fully materialized before
// the plan is returned. `hash` only reads them and uses the caller-owned hasher.
unsafe impl<T, H> Sync for NativeHashPlan<T, H> {}

impl<T, H> NativeHashPlan<T, H>
where
    T: Facet<'static>,
    H: Hasher,
{
    /// Compile a value-mode native hash plan for `T`.
    pub fn build() -> Result<Self, HashError> {
        let plan = HashPlan::<T>::build()?;
        Compiler::<H>::compile::<T>(T::SHAPE, &plan.lowered)
    }

    /// Hash `value` into `hasher` through the compiled native plan.
    pub fn hash(&self, value: &T, hasher: &mut H) -> Result<(), HashError> {
        let mut ctx = Ctx {
            base: (value as *const T).cast::<u8>(),
            hasher: core::ptr::from_mut(hasher).cast::<()>(),
            prog: self.native.entry_prog(),
        };
        let entry = unsafe { self.native.entry_fn::<Ctx>() };
        unsafe { entry(&mut ctx) };
        Ok(())
    }

    /// Return code-layout counters for this native plan.
    #[must_use]
    pub fn stats(&self) -> NativeHashPlanStats {
        NativeHashPlanStats {
            chain_count: self.native.chain_count(),
            stencil_count: self.native.stencil_count(),
            prog_slot_count: self.native.prog_slot_count(),
            scalar_count: self.scalar_infos.len(),
            scalar_run_count: self.scalar_run_infos.len(),
            scalar_run_field_count: self.scalar_run_fields.iter().map(Vec::len).sum(),
            const_usize_count: self.const_usizes.len(),
        }
    }
}

impl<T> NativeHashPlan<T, std::collections::hash_map::DefaultHasher>
where
    T: Facet<'static>,
{
    /// Hash `value` with [`std::collections::hash_map::DefaultHasher`].
    pub fn hash64(&self, value: &T) -> Result<u64, HashError> {
        let mut hasher = std::collections::hash_map::DefaultHasher::new();
        self.hash(value, &mut hasher)?;
        Ok(hasher.finish())
    }
}

/// Code-layout counters for a [`NativeHashPlan`].
#[non_exhaustive]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct NativeHashPlanStats {
    /// Number of compiled chains.
    pub chain_count: usize,
    /// Number of stencil copies.
    pub stencil_count: usize,
    /// Number of side program-stream words.
    pub prog_slot_count: usize,
    /// Number of standalone scalar ops.
    pub scalar_count: usize,
    /// Number of grouped scalar runs.
    pub scalar_run_count: usize,
    /// Total scalar fields inside grouped runs.
    pub scalar_run_field_count: usize,
    /// Number of compile-time `usize` constants hashed by this plan.
    pub const_usize_count: usize,
}

#[repr(C)]
struct Ctx {
    base: *const u8,
    hasher: *mut (),
    prog: *const u64,
}

type HashFn = unsafe extern "C" fn(hasher: *mut (), ptr: *const u8);

#[repr(C)]
#[derive(Clone, Copy)]
struct ScalarInfo {
    offset: usize,
    absolute: *const u8,
    hash: HashFn,
}

#[repr(C)]
struct ScalarRunInfo {
    fields: *const ScalarInfo,
    field_count: usize,
}

#[derive(Clone, Copy)]
enum EmittedOp {
    Scalar,
    ScalarRun,
}

struct ScalarFixup {
    slot: ProgSlot,
    scalar_index: usize,
}

struct ScalarRunFixup {
    slot: ProgSlot,
    run_index: usize,
}

#[derive(Clone, Copy)]
enum ScalarSource {
    Field { offset: usize },
    ConstUsize { index: usize },
}

#[derive(Clone, Copy)]
struct ScalarInfoBuild {
    source: ScalarSource,
    hash: HashFn,
}

struct Compiler<H> {
    layout: StencilLayout,
    scalar_infos: Vec<ScalarInfoBuild>,
    scalar_fixups: Vec<ScalarFixup>,
    scalar_run_fields: Vec<Vec<ScalarInfoBuild>>,
    scalar_run_infos: Vec<ScalarRunInfo>,
    scalar_run_fixups: Vec<ScalarRunFixup>,
    const_usizes: Vec<usize>,
    _marker: PhantomData<fn() -> H>,
}

impl<H> Compiler<H>
where
    H: Hasher,
{
    fn compile<T>(
        root_shape: &'static Shape,
        lowered: &weavy::DenseLowered<ExecOp>,
    ) -> Result<NativeHashPlan<T, H>, HashError> {
        if !lowered.blocks.is_empty() {
            return Err(unsupported(root_shape, "native recursive blocks"));
        }

        let mut compiler = Self {
            layout: StencilLayout::new(),
            scalar_infos: Vec::new(),
            scalar_fixups: Vec::new(),
            scalar_run_fields: Vec::new(),
            scalar_run_infos: Vec::new(),
            scalar_run_fixups: Vec::new(),
            const_usizes: Vec::new(),
            _marker: PhantomData,
        };
        let entry = compiler.compile_chain(root_shape, &lowered.program)?;
        compiler.finish(entry)
    }

    fn compile_chain(
        &mut self,
        root_shape: &'static Shape,
        program: &[ExecOp],
    ) -> Result<Chain, HashError> {
        let chain = self.layout.start_chain();
        let mut starts = Vec::with_capacity(program.len());
        let mut emitted = Vec::with_capacity(program.len());

        for op in program {
            starts.push(self.layout.code_len());
            emitted.push(self.compile_op(root_shape, chain.prog_index, op)?);
        }

        let done_start = self.layout.code_len();
        self.layout.emit_stencil(stencils::DONE);

        for (index, &op_start) in starts.iter().enumerate() {
            let next = starts.get(index + 1).copied().unwrap_or(done_start);
            let relocs = match emitted[index] {
                EmittedOp::Scalar => stencils::SCALAR_CONT,
                EmittedOp::ScalarRun => stencils::SCALAR_RUN_CONT,
            };
            for &rel in relocs {
                self.layout.patch_branch26(op_start + rel, next);
            }
        }

        Ok(chain)
    }

    fn compile_op(
        &mut self,
        root_shape: &'static Shape,
        prog_index: usize,
        op: &ExecOp,
    ) -> Result<EmittedOp, HashError> {
        let WeavyOp::Intrinsic(intrinsic) = op else {
            return match op {
                WeavyOp::Control(ControlOp::Return) => Err(HashError::MalformedProgram {
                    reason: "native hash root must not contain return",
                }),
                WeavyOp::Control(ControlOp::CallBlock { .. }) => {
                    Err(unsupported(root_shape, "native recursive blocks"))
                }
                _ => Err(HashError::MalformedProgram {
                    reason: "native hash compiler only accepts hash intrinsics",
                }),
            };
        };

        match intrinsic {
            HashIntrinsic::Scalar { shape, scalar } => {
                let info = scalar_info::<H>(shape, *scalar, 0)?;
                self.layout.emit_stencil(stencils::SCALAR);
                let slot = self.layout.reserve_prog_slot(prog_index);
                let scalar_index = self.scalar_infos.len();
                self.scalar_infos.push(info);
                self.scalar_fixups.push(ScalarFixup { slot, scalar_index });
                Ok(EmittedOp::Scalar)
            }
            HashIntrinsic::Struct { mode, fields, .. } => {
                if *mode != HashMode::Value {
                    return Err(unsupported(root_shape, "native structural hashing"));
                }

                let mut run = Vec::new();
                self.collect_struct_fields(root_shape, fields, 0, &mut run)?;
                Ok(self.emit_scalar_run(prog_index, run))
            }
            HashIntrinsic::Array {
                array,
                element_layout,
                element,
            } => {
                let mut run = Vec::new();
                self.collect_array(
                    root_shape,
                    array.n,
                    element_layout.size(),
                    element,
                    0,
                    &mut run,
                )?;
                Ok(self.emit_scalar_run(prog_index, run))
            }
            HashIntrinsic::Shape(_) => Err(unsupported(root_shape, "native structural hashing")),
            HashIntrinsic::Bytes(_)
            | HashIntrinsic::Option { .. }
            | HashIntrinsic::Result { .. }
            | HashIntrinsic::List { .. }
            | HashIntrinsic::Slice { .. }
            | HashIntrinsic::Set { .. }
            | HashIntrinsic::Map { .. }
            | HashIntrinsic::Pointer { .. } => {
                Err(unsupported(root_shape, "native aggregate hashing"))
            }
        }
    }

    fn emit_scalar_run(&mut self, prog_index: usize, run: Vec<ScalarInfoBuild>) -> EmittedOp {
        self.layout.emit_stencil(stencils::SCALAR_RUN);
        let slot = self.layout.reserve_prog_slot(prog_index);
        let run_index = self.scalar_run_fields.len();
        self.scalar_run_infos.push(ScalarRunInfo {
            fields: core::ptr::null(),
            field_count: run.len(),
        });
        self.scalar_run_fields.push(run);
        self.scalar_run_fixups
            .push(ScalarRunFixup { slot, run_index });
        EmittedOp::ScalarRun
    }

    fn collect_program_scalars(
        &mut self,
        root_shape: &'static Shape,
        program: &[ExecOp],
        base_offset: usize,
        run: &mut Vec<ScalarInfoBuild>,
    ) -> Result<(), HashError> {
        for op in program {
            let WeavyOp::Intrinsic(intrinsic) = op else {
                return match op {
                    WeavyOp::Control(ControlOp::Return) => Err(HashError::MalformedProgram {
                        reason: "native hash child must not contain return",
                    }),
                    WeavyOp::Control(ControlOp::CallBlock { .. }) => {
                        Err(unsupported(root_shape, "native recursive blocks"))
                    }
                    _ => Err(HashError::MalformedProgram {
                        reason: "native hash compiler only accepts hash intrinsics",
                    }),
                };
            };

            match intrinsic {
                HashIntrinsic::Scalar { shape, scalar } => {
                    run.push(scalar_info::<H>(shape, *scalar, base_offset)?);
                }
                HashIntrinsic::Struct { mode, fields, .. } => {
                    if *mode != HashMode::Value {
                        return Err(unsupported(root_shape, "native structural hashing"));
                    }
                    self.collect_struct_fields(root_shape, fields, base_offset, run)?;
                }
                HashIntrinsic::Array {
                    array,
                    element_layout,
                    element,
                } => {
                    self.collect_array(
                        root_shape,
                        array.n,
                        element_layout.size(),
                        element,
                        base_offset,
                        run,
                    )?;
                }
                HashIntrinsic::Shape(_) => {
                    return Err(unsupported(root_shape, "native structural hashing"));
                }
                HashIntrinsic::Bytes(_)
                | HashIntrinsic::Option { .. }
                | HashIntrinsic::Result { .. }
                | HashIntrinsic::List { .. }
                | HashIntrinsic::Slice { .. }
                | HashIntrinsic::Set { .. }
                | HashIntrinsic::Map { .. }
                | HashIntrinsic::Pointer { .. } => {
                    return Err(unsupported(root_shape, "native aggregate hashing"));
                }
            }
        }
        Ok(())
    }

    fn collect_struct_fields(
        &mut self,
        root_shape: &'static Shape,
        fields: &[StructFieldPlan<ExecBlock>],
        base_offset: usize,
        run: &mut Vec<ScalarInfoBuild>,
    ) -> Result<(), HashError> {
        for field in fields {
            match field {
                StructFieldPlan::ScalarRun(fields) => {
                    for field in fields.iter() {
                        run.push(scalar_field_info::<H>(field, base_offset)?);
                    }
                }
                StructFieldPlan::Field(field) => {
                    self.collect_child_scalars(
                        root_shape,
                        &field.child,
                        base_offset + field.offset,
                        run,
                    )?;
                }
            }
        }
        Ok(())
    }

    fn collect_array(
        &mut self,
        root_shape: &'static Shape,
        len: usize,
        stride: usize,
        element: &ChildPlan<ExecBlock>,
        base_offset: usize,
        run: &mut Vec<ScalarInfoBuild>,
    ) -> Result<(), HashError> {
        run.push(self.const_usize_info::<H>(len));
        for index in 0..len {
            self.collect_child_scalars(root_shape, element, base_offset + index * stride, run)?;
        }
        Ok(())
    }

    fn collect_child_scalars(
        &mut self,
        root_shape: &'static Shape,
        child: &ChildPlan<ExecBlock>,
        base_offset: usize,
        run: &mut Vec<ScalarInfoBuild>,
    ) -> Result<(), HashError> {
        match child {
            ChildPlan::Scalar {
                include_shape,
                shape,
                scalar,
            } => {
                if *include_shape {
                    return Err(unsupported(shape, "native structural scalar field"));
                }
                run.push(scalar_info::<H>(shape, *scalar, base_offset)?);
                Ok(())
            }
            ChildPlan::Program(program) => {
                self.collect_program_scalars(root_shape, program, base_offset, run)
            }
        }
    }

    fn const_usize_info<T>(&mut self, value: usize) -> ScalarInfoBuild
    where
        T: Hasher,
    {
        let index = self.const_usizes.len();
        self.const_usizes.push(value);
        ScalarInfoBuild {
            source: ScalarSource::ConstUsize { index },
            hash: hash_value::<T, usize>,
        }
    }

    fn finish<T>(mut self, entry: Chain) -> Result<NativeHashPlan<T, H>, HashError> {
        let native = NativeProgram::new(core::mem::take(&mut self.layout), entry);
        let const_usizes = self.const_usizes;
        let scalar_infos = materialize_scalar_infos(&self.scalar_infos, &const_usizes);
        let scalar_run_fields = self
            .scalar_run_fields
            .iter()
            .map(|fields| materialize_scalar_infos(fields, &const_usizes))
            .collect();
        let mut plan = NativeHashPlan {
            native,
            scalar_infos,
            scalar_run_fields,
            scalar_run_infos: self.scalar_run_infos,
            const_usizes,
            _marker: PhantomData,
        };

        let run_ptrs: Vec<*const ScalarInfo> =
            plan.scalar_run_fields.iter().map(Vec::as_ptr).collect();
        for (info, &ptr) in plan.scalar_run_infos.iter_mut().zip(run_ptrs.iter()) {
            info.fields = ptr;
        }

        for fixup in self.scalar_fixups {
            let ptr: *const ScalarInfo = &plan.scalar_infos[fixup.scalar_index];
            plan.native.fill_prog_slot(fixup.slot, ptr as u64);
        }
        for fixup in self.scalar_run_fixups {
            let ptr: *const ScalarRunInfo = &plan.scalar_run_infos[fixup.run_index];
            plan.native.fill_prog_slot(fixup.slot, ptr as u64);
        }

        Ok(plan)
    }
}

fn scalar_field_info<H>(
    field: &ScalarFieldPlan,
    base_offset: usize,
) -> Result<ScalarInfoBuild, HashError>
where
    H: Hasher,
{
    if field.include_shape {
        return Err(unsupported(field.shape, "native structural scalar field"));
    }
    scalar_info::<H>(field.shape, field.scalar, base_offset + field.offset)
}

fn scalar_info<H>(
    shape: &'static Shape,
    scalar: ScalarType,
    offset: usize,
) -> Result<ScalarInfoBuild, HashError>
where
    H: Hasher,
{
    Ok(ScalarInfoBuild {
        source: ScalarSource::Field { offset },
        hash: scalar_hash_fn::<H>(shape, scalar)?,
    })
}

fn materialize_scalar_infos(infos: &[ScalarInfoBuild], const_usizes: &[usize]) -> Vec<ScalarInfo> {
    infos
        .iter()
        .map(|info| match info.source {
            ScalarSource::Field { offset } => ScalarInfo {
                offset,
                absolute: core::ptr::null(),
                hash: info.hash,
            },
            ScalarSource::ConstUsize { index } => ScalarInfo {
                offset: 0,
                absolute: core::ptr::from_ref(&const_usizes[index]).cast::<u8>(),
                hash: info.hash,
            },
        })
        .collect()
}

fn scalar_hash_fn<H>(shape: &'static Shape, scalar: ScalarType) -> Result<HashFn, HashError>
where
    H: Hasher,
{
    Ok(match scalar {
        ScalarType::Unit => hash_unit::<H>,
        ScalarType::Bool => hash_value::<H, bool>,
        ScalarType::Char => hash_value::<H, char>,
        ScalarType::Str if shape.is_type::<&'static str>() => hash_value::<H, &'static str>,
        ScalarType::Str => return Err(unsupported(shape, "native unsized str")),
        ScalarType::String => hash_value::<H, String>,
        ScalarType::CowStr => hash_value::<H, Cow<'static, str>>,
        ScalarType::F32 => hash_f32::<H>,
        ScalarType::F64 => hash_f64::<H>,
        ScalarType::U8 => hash_value::<H, u8>,
        ScalarType::U16 => hash_value::<H, u16>,
        ScalarType::U32 => hash_value::<H, u32>,
        ScalarType::U64 => hash_value::<H, u64>,
        ScalarType::U128 => hash_value::<H, u128>,
        ScalarType::USize => hash_value::<H, usize>,
        ScalarType::I8 => hash_value::<H, i8>,
        ScalarType::I16 => hash_value::<H, i16>,
        ScalarType::I32 => hash_value::<H, i32>,
        ScalarType::I64 => hash_value::<H, i64>,
        ScalarType::I128 => hash_value::<H, i128>,
        ScalarType::ISize => hash_value::<H, isize>,
        ScalarType::ConstTypeId => hash_value::<H, ConstTypeId>,
        #[cfg(feature = "net")]
        ScalarType::SocketAddr => hash_value::<H, core::net::SocketAddr>,
        #[cfg(feature = "net")]
        ScalarType::IpAddr => hash_value::<H, core::net::IpAddr>,
        #[cfg(feature = "net")]
        ScalarType::Ipv4Addr => hash_value::<H, core::net::Ipv4Addr>,
        #[cfg(feature = "net")]
        ScalarType::Ipv6Addr => hash_value::<H, core::net::Ipv6Addr>,
        _ => return Err(unsupported(shape, "native scalar")),
    })
}

unsafe extern "C" fn hash_unit<H>(hasher: *mut (), _ptr: *const u8)
where
    H: Hasher,
{
    unsafe { hasher.cast::<H>().as_mut().unwrap_unchecked() }.write_u8(0);
}

unsafe extern "C" fn hash_value<H, T>(hasher: *mut (), ptr: *const u8)
where
    H: Hasher,
    T: Hash,
{
    let hasher = unsafe { hasher.cast::<H>().as_mut().unwrap_unchecked() };
    unsafe { PtrConst::new_sized(ptr.cast::<T>()).get::<T>() }.hash(hasher);
}

unsafe extern "C" fn hash_f32<H>(hasher: *mut (), ptr: *const u8)
where
    H: Hasher,
{
    let hasher = unsafe { hasher.cast::<H>().as_mut().unwrap_unchecked() };
    unsafe { PtrConst::new_sized(ptr.cast::<f32>()).get::<f32>() }
        .to_bits()
        .hash(hasher);
}

unsafe extern "C" fn hash_f64<H>(hasher: *mut (), ptr: *const u8)
where
    H: Hasher,
{
    let hasher = unsafe { hasher.cast::<H>().as_mut().unwrap_unchecked() };
    unsafe { PtrConst::new_sized(ptr.cast::<f64>()).get::<f64>() }
        .to_bits()
        .hash(hasher);
}
