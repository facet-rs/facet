//! Cranelift-based JIT compiler for JSON deserializers.
//!
//! Generated functions have signature:
//!   fn(input: *const u8, len: usize, pos: usize, out: *mut u8) -> isize
//!
//! Return value >= 0 means success (new position), < 0 means error code.
//! The `pos` parameter is kept in a register throughout execution.

use crate::JsonError;
use cranelift::prelude::*;
use cranelift_jit::{JITBuilder, JITModule};
use cranelift_module::{Linkage, Module};
use facet_core::{Def, Facet, NumericType, PrimitiveType, Shape, Type as FacetType, UserType};
use std::collections::HashMap;

use super::helpers;

// =============================================================================
// C ABI Calling Convention Helpers
// =============================================================================

/// Returns the C ABI calling convention for the current platform.
///
/// This is necessary because Cranelift's `make_signature()` uses a default calling
/// convention that may not match `extern "C"` on all platforms. On Windows x64,
/// `extern "C"` uses the Microsoft x64 calling convention (WindowsFastcall),
/// while Cranelift may default to System V.
#[inline]
fn c_call_conv() -> cranelift::codegen::isa::CallConv {
    #[cfg(all(target_os = "windows", target_arch = "x86_64"))]
    {
        cranelift::codegen::isa::CallConv::WindowsFastcall
    }
    #[cfg(not(all(target_os = "windows", target_arch = "x86_64")))]
    {
        // On non-Windows platforms, System V is the standard C ABI for x86_64
        // For other architectures, Cranelift's default is usually correct
        #[cfg(target_arch = "x86_64")]
        {
            cranelift::codegen::isa::CallConv::SystemV
        }
        #[cfg(target_arch = "aarch64")]
        {
            cranelift::codegen::isa::CallConv::AppleAarch64
        }
        #[cfg(not(any(target_arch = "x86_64", target_arch = "aarch64")))]
        {
            // Fallback - let Cranelift decide
            cranelift::codegen::isa::CallConv::Fast
        }
    }
}

/// Creates a new signature with the correct C ABI calling convention for the current platform.
///
/// Use this instead of `module.make_signature()` when creating signatures for calls to
/// `extern "C"` functions.
#[inline]
fn make_c_sig(module: &JITModule) -> cranelift::codegen::ir::Signature {
    let mut sig = module.make_signature();
    sig.call_conv = c_call_conv();
    sig
}

// =============================================================================
// Prefix Trie for field name dispatch
// =============================================================================

/// A node in the field name trie.
#[derive(Debug, Clone)]
enum TrieNode {
    Branch {
        children: HashMap<u8, TrieNode>,
        terminal: Option<usize>,
    },
    Leaf(usize),
}

impl TrieNode {
    fn new_branch() -> Self {
        TrieNode::Branch {
            children: HashMap::new(),
            terminal: None,
        }
    }

    fn insert(&mut self, name: &[u8], field_index: usize) {
        match self {
            TrieNode::Branch { children, terminal } => {
                if name.is_empty() {
                    *terminal = Some(field_index);
                } else {
                    let first = name[0];
                    let rest = &name[1..];
                    let child = children.entry(first).or_insert_with(TrieNode::new_branch);
                    child.insert(rest, field_index);
                }
            }
            TrieNode::Leaf(_) => panic!("Duplicate field name prefix"),
        }
    }

    fn optimize(&mut self) {
        if let TrieNode::Branch { children, terminal } = self {
            for child in children.values_mut() {
                child.optimize();
            }
            if children.is_empty()
                && let Some(idx) = *terminal
            {
                *self = TrieNode::Leaf(idx);
            }
        }
    }
}

fn build_field_trie(fields: &[FieldInfo]) -> TrieNode {
    let mut root = TrieNode::new_branch();
    for (i, field) in fields.iter().enumerate() {
        root.insert(field.name.as_bytes(), i);
    }
    root.optimize();
    root
}

// =============================================================================
// Field info and parser types
// =============================================================================

struct FieldInfo {
    name: &'static str,
    offset: usize,
    parser: FieldParser,
}

#[derive(Clone, Copy)]
enum FieldParser {
    F64,
    F32,
    I64,
    I32,
    I16,
    I8,
    U64,
    U32,
    U16,
    U8,
    Bool,
    String,
    VecF64,
    VecI64,
    VecU64,
    VecBool,
    VecVecF64,
    VecVecVecF64,
    VecStruct(&'static Shape),
    NestedStruct(&'static Shape),
    /// `Option<T>` - the Shape is the full Option shape (not the inner T)
    Option(&'static Shape),
    Skip,
}

fn extract_fields(shape: &'static Shape) -> Vec<FieldInfo> {
    let mut fields = Vec::new();

    let FacetType::User(UserType::Struct(struct_def)) = &shape.ty else {
        return fields;
    };

    for field in struct_def.fields {
        let name = field.name;
        let offset = field.offset;
        let field_shape = field.shape.get();

        let field_size = field_shape
            .layout
            .sized_layout()
            .map(|l| l.size())
            .unwrap_or(0);

        let parser = match &field_shape.ty {
            FacetType::Primitive(PrimitiveType::Numeric(NumericType::Float)) => match field_size {
                4 => FieldParser::F32,
                8 => FieldParser::F64,
                _ => FieldParser::Skip,
            },
            FacetType::Primitive(PrimitiveType::Numeric(NumericType::Integer { signed: true })) => {
                match field_size {
                    1 => FieldParser::I8,
                    2 => FieldParser::I16,
                    4 => FieldParser::I32,
                    8 => FieldParser::I64,
                    _ => FieldParser::Skip,
                }
            }
            FacetType::Primitive(PrimitiveType::Numeric(NumericType::Integer {
                signed: false,
            })) => match field_size {
                1 => FieldParser::U8,
                2 => FieldParser::U16,
                4 => FieldParser::U32,
                8 => FieldParser::U64,
                _ => FieldParser::Skip,
            },
            FacetType::Primitive(PrimitiveType::Boolean) => FieldParser::Bool,
            // Check for String (Opaque type with "String" identifier)
            FacetType::User(UserType::Opaque) if field_shape.type_identifier == "String" => {
                FieldParser::String
            }
            // Check for nested struct
            FacetType::User(UserType::Struct(_)) => FieldParser::NestedStruct(field_shape),
            _ => {
                // Check for Option types first (via Def::Option)
                if let Def::Option(_) = field_shape.def {
                    FieldParser::Option(field_shape)
                }
                // Check for Vec types (List)
                else if let Def::List(list_def) = field_shape.def {
                    let elem_shape = list_def.t();
                    // Check for Vec<f64>
                    if matches!(
                        elem_shape.ty,
                        FacetType::Primitive(PrimitiveType::Numeric(NumericType::Float))
                    ) {
                        FieldParser::VecF64
                    }
                    // Check for Vec<bool>
                    else if matches!(elem_shape.ty, FacetType::Primitive(PrimitiveType::Boolean))
                    {
                        FieldParser::VecBool
                    }
                    // Check for Vec<i64> or Vec<u64>
                    else if let FacetType::Primitive(PrimitiveType::Numeric(
                        NumericType::Integer { signed },
                    )) = elem_shape.ty
                    {
                        // Check element size from layout
                        let elem_size = elem_shape
                            .layout
                            .sized_layout()
                            .map(|l| l.size())
                            .unwrap_or(0);
                        if elem_size == 8 {
                            if signed {
                                FieldParser::VecI64
                            } else {
                                FieldParser::VecU64
                            }
                        } else {
                            // For now, skip other integer sizes
                            FieldParser::Skip
                        }
                    }
                    // Check for Vec<Vec<...>>
                    else if let Def::List(inner_list) = elem_shape.def {
                        let inner_elem = inner_list.t();
                        // Vec<Vec<f64>>
                        if matches!(
                            inner_elem.ty,
                            FacetType::Primitive(PrimitiveType::Numeric(NumericType::Float))
                        ) {
                            FieldParser::VecVecF64
                        }
                        // Check for Vec<Vec<Vec<f64>>>
                        else if let Def::List(innermost_list) = inner_elem.def {
                            let innermost_elem = innermost_list.t();
                            if matches!(
                                innermost_elem.ty,
                                FacetType::Primitive(PrimitiveType::Numeric(NumericType::Float))
                            ) {
                                FieldParser::VecVecVecF64
                            } else {
                                FieldParser::Skip
                            }
                        } else {
                            FieldParser::Skip
                        }
                    }
                    // Check for Vec<Struct>
                    else if matches!(elem_shape.ty, FacetType::User(UserType::Struct(_))) {
                        FieldParser::VecStruct(elem_shape)
                    } else {
                        FieldParser::Skip
                    }
                } else {
                    FieldParser::Skip
                }
            }
        };

        fields.push(FieldInfo {
            name,
            offset,
            parser,
        });
    }

    fields
}

// =============================================================================
// Compiled deserializer
// =============================================================================

/// A compiled deserializer function pointer.
#[derive(Clone, Copy)]
pub struct CompiledDeserializer {
    ptr: *const u8,
}

unsafe impl Send for CompiledDeserializer {}
unsafe impl Sync for CompiledDeserializer {}

impl CompiledDeserializer {
    /// Get the raw function pointer.
    pub fn ptr(&self) -> *const u8 {
        self.ptr
    }

    /// Call the compiled deserializer.
    ///
    /// # Safety
    /// The caller must ensure T matches the type this deserializer was compiled for.
    pub unsafe fn call<T>(&self, input: &str) -> Result<T, JsonError> {
        let mut output = std::mem::MaybeUninit::<T>::uninit();

        // New signature: fn(input, len, pos, out) -> isize
        // SAFETY: self.ptr was created by the JIT compiler and points to valid code
        let func: unsafe extern "C" fn(*const u8, usize, usize, *mut u8) -> isize =
            unsafe { std::mem::transmute(self.ptr) };

        // SAFETY: func is a valid function pointer, input is valid for the duration of the call
        let result = unsafe {
            func(
                input.as_ptr(),
                input.len(),
                0,
                output.as_mut_ptr() as *mut u8,
            )
        };

        if result >= 0 {
            // SAFETY: result >= 0 indicates successful parsing and output is initialized
            Ok(unsafe { output.assume_init() })
        } else {
            Err(error_from_code(result))
        }
    }
}

fn error_from_code(code: isize) -> JsonError {
    use super::helpers::*;
    let msg = match code {
        ERR_UNEXPECTED_EOF => "unexpected end of input",
        ERR_EXPECTED_COLON => "expected ':'",
        ERR_EXPECTED_COMMA_OR_END => "expected ',' or closing bracket",
        ERR_EXPECTED_OBJECT_START => "expected '{'",
        ERR_EXPECTED_ARRAY_START => "expected '['",
        ERR_INVALID_NUMBER => "invalid number",
        ERR_INVALID_STRING => "invalid string",
        ERR_INVALID_BOOL => "invalid boolean",
        _ => "unknown error",
    };
    crate::from_str::<()>(msg).unwrap_err()
}

// Global compiler instance
use parking_lot::Mutex;
use std::sync::LazyLock;

static COMPILER: LazyLock<Mutex<JitCompiler>> = LazyLock::new(|| Mutex::new(JitCompiler::new()));

/// Try to compile a deserializer for the given shape.
/// Returns None if the shape is not supported.
/// Check if a shape contains any Map types (HashMap, BTreeMap, etc.).
/// The JIT compiler doesn't support Maps yet, so we reject them.
fn contains_map(shape: &'static Shape) -> bool {
    // Check if this shape itself is a Map
    if matches!(shape.def, Def::Map(_)) {
        return true;
    }

    // Recursively check nested types
    match shape.def {
        Def::List(list_def) => contains_map(list_def.t()),
        Def::Set(set_def) => contains_map(set_def.t()),
        Def::Option(opt_def) => contains_map(opt_def.t()),
        Def::Pointer(ptr_def) => {
            if let Some(pointee) = ptr_def.pointee {
                contains_map(pointee)
            } else {
                false
            }
        }
        _ => {
            // Check if it's a struct and recursively check its fields
            if let FacetType::User(UserType::Struct(struct_def)) = &shape.ty {
                struct_def
                    .fields
                    .iter()
                    .any(|field| contains_map(field.shape()))
            } else {
                false
            }
        }
    }
}

pub fn try_compile(shape: &'static Shape) -> Option<CompiledDeserializer> {
    // Reject types containing HashMap - the old JIT doesn't support them
    if contains_map(shape) {
        return None;
    }

    // Check for top-level arrays first
    if let Def::List(list_def) = shape.def {
        let elem_shape = list_def.t();
        // Map element type to helper function
        let helper_ptr: *const u8 = match elem_shape.ty {
            FacetType::Primitive(PrimitiveType::Numeric(NumericType::Float)) => {
                let elem_size = elem_shape
                    .layout
                    .sized_layout()
                    .map(|l| l.size())
                    .unwrap_or(0);
                if elem_size == 8 {
                    helpers::jitson_parse_vec_f64 as *const u8
                } else {
                    return None; // f32 vec not supported yet
                }
            }
            FacetType::Primitive(PrimitiveType::Numeric(NumericType::Integer { signed })) => {
                let elem_size = elem_shape
                    .layout
                    .sized_layout()
                    .map(|l| l.size())
                    .unwrap_or(0);
                if elem_size == 8 {
                    if signed {
                        helpers::jitson_parse_vec_i64 as *const u8
                    } else {
                        helpers::jitson_parse_vec_u64 as *const u8
                    }
                } else {
                    return None; // smaller int sizes not supported yet
                }
            }
            FacetType::Primitive(PrimitiveType::Boolean) => {
                helpers::jitson_parse_vec_bool as *const u8
            }
            _ if elem_shape == <String as Facet>::SHAPE => {
                helpers::jitson_parse_vec_string as *const u8
            }
            _ => return None,
        };
        return Some(CompiledDeserializer { ptr: helper_ptr });
    }

    // Check if it's a struct
    let FacetType::User(UserType::Struct(_)) = &shape.ty else {
        return None;
    };

    let mut compiler = COMPILER.lock();
    Some(compiler.compile(shape))
}

/// Get or compile a deserializer for the given shape.
/// This is used for nested struct compilation from within JitCompiler::compile.
/// IMPORTANT: This must be called while holding the COMPILER lock, so it
/// accesses the compiler directly without re-acquiring the lock.
fn get_or_compile_for_shape_locked(compiler: &mut JitCompiler, shape: &'static Shape) -> *const u8 {
    // Check shape cache first
    if let Some(func) = super::cache::get_by_shape(shape) {
        return func.ptr();
    }

    // Compile it directly using the already-locked compiler
    let func = compiler.compile(shape);
    super::cache::insert_by_shape(shape, func);
    func.ptr()
}

// =============================================================================
// JIT Compiler
// =============================================================================

/// The JIT compiler for JSON deserializers.
pub struct JitCompiler {
    module: JITModule,
    helper_funcs: HashMap<&'static str, cranelift_module::FuncId>,
}

impl JitCompiler {
    /// Create a new JIT compiler.
    pub fn new() -> Self {
        let mut flag_builder = settings::builder();
        // Try speed_and_size - may generate slightly smaller code with similar perf
        let opt_level = std::env::var("JITSON_OPT").unwrap_or_else(|_| "speed".to_string());
        flag_builder.set("opt_level", &opt_level).unwrap();
        let isa_builder = cranelift_native::builder().unwrap();
        let isa = isa_builder
            .finish(settings::Flags::new(flag_builder))
            .unwrap();

        let mut builder = JITBuilder::with_isa(isa, cranelift_module::default_libcall_names());
        helpers::register_helpers(&mut builder);

        let module = JITModule::new(builder);

        Self {
            module,
            helper_funcs: HashMap::new(),
        }
    }

    /// Compile a deserializer for the given shape.
    pub fn compile(&mut self, shape: &'static Shape) -> CompiledDeserializer {
        let fields = extract_fields(shape);
        let trie = build_field_trie(&fields);

        // Pre-compile all nested structs to avoid deadlock
        // (we can't call get_or_compile while holding &mut self in the builder loop)
        let mut nested_func_ptrs: HashMap<*const Shape, *const u8> = HashMap::new();
        for field in &fields {
            match field.parser {
                FieldParser::NestedStruct(nested_shape) => {
                    let ptr = nested_shape as *const Shape;
                    nested_func_ptrs
                        .entry(ptr)
                        .or_insert_with(|| get_or_compile_for_shape_locked(self, nested_shape));
                }
                FieldParser::VecStruct(elem_shape) => {
                    let ptr = elem_shape as *const Shape;
                    nested_func_ptrs
                        .entry(ptr)
                        .or_insert_with(|| get_or_compile_for_shape_locked(self, elem_shape));
                }
                FieldParser::Option(option_shape) => {
                    // Recursively find and pre-compile any nested structs
                    fn collect_nested_structs(
                        shape: &'static Shape,
                        nested_func_ptrs: &mut HashMap<*const Shape, *const u8>,
                        compiler: &mut JitCompiler,
                    ) {
                        match shape.def {
                            Def::Option(opt_def) => {
                                // Recurse into Option's inner type
                                collect_nested_structs(opt_def.t(), nested_func_ptrs, compiler);
                            }
                            Def::Pointer(ptr_def) => {
                                // Recurse into Box/pointer's pointee
                                if let Some(pointee) = ptr_def.pointee {
                                    collect_nested_structs(pointee, nested_func_ptrs, compiler);
                                }
                            }
                            _ => {}
                        }
                        // If it's a struct, pre-compile it
                        if matches!(shape.ty, FacetType::User(UserType::Struct(_))) {
                            let ptr = shape as *const Shape;
                            nested_func_ptrs.entry(ptr).or_insert_with(|| {
                                get_or_compile_for_shape_locked(compiler, shape)
                            });
                        }
                    }
                    collect_nested_structs(option_shape, &mut nested_func_ptrs, self);
                }
                _ => {}
            }
        }

        let ptr_type = self.module.target_config().pointer_type();

        // Function signature: fn(input: ptr, len: usize, pos: usize, out: ptr) -> isize
        let mut sig = make_c_sig(&self.module);
        sig.params.push(AbiParam::new(ptr_type)); // input
        sig.params.push(AbiParam::new(ptr_type)); // len
        sig.params.push(AbiParam::new(ptr_type)); // pos
        sig.params.push(AbiParam::new(ptr_type)); // out
        sig.returns.push(AbiParam::new(ptr_type)); // new_pos or error

        // Declare helper function signatures
        // All helpers now: fn(input, len, pos, out) -> isize  (or fn(input, len, pos) -> isize for skip)
        let sig_parse_value = {
            let mut s = make_c_sig(&self.module);
            s.params.push(AbiParam::new(ptr_type)); // input
            s.params.push(AbiParam::new(ptr_type)); // len
            s.params.push(AbiParam::new(ptr_type)); // pos
            s.params.push(AbiParam::new(ptr_type)); // out
            s.returns.push(AbiParam::new(ptr_type)); // new_pos or error
            s
        };

        let sig_skip_value = {
            let mut s = make_c_sig(&self.module);
            s.params.push(AbiParam::new(ptr_type)); // input
            s.params.push(AbiParam::new(ptr_type)); // len
            s.params.push(AbiParam::new(ptr_type)); // pos
            s.returns.push(AbiParam::new(ptr_type)); // new_pos or error
            s
        };

        let sig_nested_struct = {
            let mut s = make_c_sig(&self.module);
            s.params.push(AbiParam::new(ptr_type)); // input
            s.params.push(AbiParam::new(ptr_type)); // len
            s.params.push(AbiParam::new(ptr_type)); // pos
            s.params.push(AbiParam::new(ptr_type)); // out
            s.params.push(AbiParam::new(ptr_type)); // func_ptr
            s.returns.push(AbiParam::new(ptr_type));
            s
        };

        let sig_vec_struct = {
            let mut s = make_c_sig(&self.module);
            s.params.push(AbiParam::new(ptr_type)); // input
            s.params.push(AbiParam::new(ptr_type)); // len
            s.params.push(AbiParam::new(ptr_type)); // pos
            s.params.push(AbiParam::new(ptr_type)); // out
            s.params.push(AbiParam::new(ptr_type)); // elem_size
            s.params.push(AbiParam::new(ptr_type)); // elem_align
            s.params.push(AbiParam::new(ptr_type)); // func_ptr
            s.returns.push(AbiParam::new(ptr_type));
            s
        };

        let sig_option = {
            let mut s = make_c_sig(&self.module);
            s.params.push(AbiParam::new(ptr_type)); // input
            s.params.push(AbiParam::new(ptr_type)); // len
            s.params.push(AbiParam::new(ptr_type)); // pos
            s.params.push(AbiParam::new(ptr_type)); // out
            s.params.push(AbiParam::new(ptr_type)); // option_shape
            s.params.push(AbiParam::new(ptr_type)); // inner_deser_fn (nullable)
            s.returns.push(AbiParam::new(ptr_type));
            s
        };

        let sig_init_option = {
            let mut s = make_c_sig(&self.module);
            s.params.push(AbiParam::new(ptr_type)); // out
            s.params.push(AbiParam::new(ptr_type)); // option_shape
            s
        };

        // Declare all helpers we need
        let helpers_to_declare = [
            ("jitson_parse_f64", sig_parse_value.clone()),
            ("jitson_parse_f32", sig_parse_value.clone()),
            ("jitson_parse_i64", sig_parse_value.clone()),
            ("jitson_parse_i32", sig_parse_value.clone()),
            ("jitson_parse_i16", sig_parse_value.clone()),
            ("jitson_parse_i8", sig_parse_value.clone()),
            ("jitson_parse_u64", sig_parse_value.clone()),
            ("jitson_parse_u32", sig_parse_value.clone()),
            ("jitson_parse_u16", sig_parse_value.clone()),
            ("jitson_parse_u8", sig_parse_value.clone()),
            ("jitson_parse_bool", sig_parse_value.clone()),
            ("jitson_parse_string", sig_parse_value.clone()),
            ("jitson_parse_vec_f64", sig_parse_value.clone()),
            ("jitson_parse_vec_i64", sig_parse_value.clone()),
            ("jitson_parse_vec_u64", sig_parse_value.clone()),
            ("jitson_parse_vec_bool", sig_parse_value.clone()),
            ("jitson_parse_vec_vec_f64", sig_parse_value.clone()),
            ("jitson_parse_vec_vec_vec_f64", sig_parse_value.clone()),
            ("jitson_skip_value", sig_skip_value.clone()),
            ("jitson_parse_nested_struct", sig_nested_struct.clone()),
            ("jitson_parse_vec_struct", sig_vec_struct.clone()),
            ("jitson_parse_option", sig_option.clone()),
            ("jitson_init_option_none", sig_init_option.clone()),
        ];

        for (name, helper_sig) in &helpers_to_declare {
            if !self.helper_funcs.contains_key(name) {
                let func_id = self
                    .module
                    .declare_function(name, Linkage::Import, helper_sig)
                    .unwrap();
                self.helper_funcs.insert(name, func_id);
            }
        }

        // Create the function
        static COUNTER: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);
        let func_name = format!(
            "deserialize_{}",
            COUNTER.fetch_add(1, std::sync::atomic::Ordering::SeqCst)
        );
        let func_id = self
            .module
            .declare_function(&func_name, Linkage::Local, &sig)
            .unwrap();

        let mut ctx = self.module.make_context();
        ctx.func.signature = sig;

        // Enable disassembly if JITSON_DISASM env var is set
        let want_disasm = std::env::var("JITSON_DISASM").is_ok();
        ctx.set_disasm(want_disasm);

        let mut builder_ctx = FunctionBuilderContext::new();
        let mut builder = FunctionBuilder::new(&mut ctx.func, &mut builder_ctx);

        // Create entry block and get parameters
        let entry_block = builder.create_block();
        builder.append_block_params_for_function_params(entry_block);
        builder.switch_to_block(entry_block);
        builder.seal_block(entry_block);

        let input_ptr = builder.block_params(entry_block)[0];
        let len_val = builder.block_params(entry_block)[1];
        let pos_param = builder.block_params(entry_block)[2];
        let out_ptr = builder.block_params(entry_block)[3];

        // Create a variable for pos - this is the key optimization!
        // pos lives in a register, not memory
        let pos_var = builder.declare_var(ptr_type);
        builder.def_var(pos_var, pos_param);

        // Create blocks
        let error_block = builder.create_block();
        let success_block = builder.create_block();
        let field_loop = builder.create_block();
        let after_field = builder.create_block();

        // Import helper functions
        let skip_value = self
            .module
            .declare_func_in_func(self.helper_funcs["jitson_skip_value"], builder.func);
        let parse_f64 = self
            .module
            .declare_func_in_func(self.helper_funcs["jitson_parse_f64"], builder.func);
        let parse_f32 = self
            .module
            .declare_func_in_func(self.helper_funcs["jitson_parse_f32"], builder.func);
        let parse_i64 = self
            .module
            .declare_func_in_func(self.helper_funcs["jitson_parse_i64"], builder.func);
        let parse_i32 = self
            .module
            .declare_func_in_func(self.helper_funcs["jitson_parse_i32"], builder.func);
        let parse_i16 = self
            .module
            .declare_func_in_func(self.helper_funcs["jitson_parse_i16"], builder.func);
        let parse_i8 = self
            .module
            .declare_func_in_func(self.helper_funcs["jitson_parse_i8"], builder.func);
        let parse_u64 = self
            .module
            .declare_func_in_func(self.helper_funcs["jitson_parse_u64"], builder.func);
        let parse_u32 = self
            .module
            .declare_func_in_func(self.helper_funcs["jitson_parse_u32"], builder.func);
        let parse_u16 = self
            .module
            .declare_func_in_func(self.helper_funcs["jitson_parse_u16"], builder.func);
        let parse_u8 = self
            .module
            .declare_func_in_func(self.helper_funcs["jitson_parse_u8"], builder.func);
        let parse_bool = self
            .module
            .declare_func_in_func(self.helper_funcs["jitson_parse_bool"], builder.func);
        let parse_string = self
            .module
            .declare_func_in_func(self.helper_funcs["jitson_parse_string"], builder.func);
        let parse_vec_f64 = self
            .module
            .declare_func_in_func(self.helper_funcs["jitson_parse_vec_f64"], builder.func);
        let parse_vec_i64 = self
            .module
            .declare_func_in_func(self.helper_funcs["jitson_parse_vec_i64"], builder.func);
        let parse_vec_u64 = self
            .module
            .declare_func_in_func(self.helper_funcs["jitson_parse_vec_u64"], builder.func);
        let parse_vec_bool = self
            .module
            .declare_func_in_func(self.helper_funcs["jitson_parse_vec_bool"], builder.func);
        let parse_vec_vec_f64 = self
            .module
            .declare_func_in_func(self.helper_funcs["jitson_parse_vec_vec_f64"], builder.func);
        let parse_vec_vec_vec_f64 = self.module.declare_func_in_func(
            self.helper_funcs["jitson_parse_vec_vec_vec_f64"],
            builder.func,
        );
        let parse_nested_struct = self.module.declare_func_in_func(
            self.helper_funcs["jitson_parse_nested_struct"],
            builder.func,
        );
        let parse_vec_struct = self
            .module
            .declare_func_in_func(self.helper_funcs["jitson_parse_vec_struct"], builder.func);
        let parse_option = self
            .module
            .declare_func_in_func(self.helper_funcs["jitson_parse_option"], builder.func);
        let init_option_none = self
            .module
            .declare_func_in_func(self.helper_funcs["jitson_init_option_none"], builder.func);

        // Skip initial whitespace (inline)
        Self::emit_skip_ws_inline(&mut builder, input_ptr, len_val, pos_var, ptr_type);

        // Check we have at least one character left
        let pos = builder.use_var(pos_var);
        let in_bounds = builder.ins().icmp(IntCC::UnsignedLessThan, pos, len_val);
        let have_char = builder.create_block();
        builder
            .ins()
            .brif(in_bounds, have_char, &[], error_block, &[]);
        builder.switch_to_block(have_char);

        // Expect '{'
        let pos = builder.use_var(pos_var);
        let char_ptr = builder.ins().iadd(input_ptr, pos);
        let ch = builder
            .ins()
            .load(types::I8, MemFlags::trusted(), char_ptr, 0);
        let is_brace = builder.ins().icmp_imm(IntCC::Equal, ch, b'{' as i64);
        let after_brace_check = builder.create_block();
        builder
            .ins()
            .brif(is_brace, after_brace_check, &[], error_block, &[]);

        builder.switch_to_block(after_brace_check);
        // Advance past '{'
        let pos = builder.use_var(pos_var);
        let new_pos = builder.ins().iadd_imm(pos, 1);
        builder.def_var(pos_var, new_pos);

        // Initialize all Option fields to None upfront
        // (This prevents valgrind errors from uninitialized memory reads)
        // See issue #1242 for planned optimization to only init missing fields
        for field in &fields {
            if let FieldParser::Option(option_shape) = field.parser {
                let field_ptr = builder.ins().iadd_imm(out_ptr, field.offset as i64);
                let option_shape_val = builder
                    .ins()
                    .iconst(ptr_type, option_shape as *const Shape as i64);
                builder
                    .ins()
                    .call(init_option_none, &[field_ptr, option_shape_val]);
            }
        }

        // Skip whitespace
        Self::emit_skip_ws_inline(&mut builder, input_ptr, len_val, pos_var, ptr_type);

        // Check for empty object
        let pos = builder.use_var(pos_var);
        let char_ptr = builder.ins().iadd(input_ptr, pos);
        let ch = builder
            .ins()
            .load(types::I8, MemFlags::trusted(), char_ptr, 0);
        let is_close = builder.ins().icmp_imm(IntCC::Equal, ch, b'}' as i64);
        let start_fields = builder.create_block();
        builder
            .ins()
            .brif(is_close, success_block, &[], start_fields, &[]);

        // Start field loop
        builder.switch_to_block(start_fields);
        builder.ins().jump(field_loop, &[]);

        // Field loop
        builder.switch_to_block(field_loop);

        // Skip whitespace before field name
        Self::emit_skip_ws_inline(&mut builder, input_ptr, len_val, pos_var, ptr_type);

        // Expect '"' for field name
        let pos = builder.use_var(pos_var);
        let char_ptr = builder.ins().iadd(input_ptr, pos);
        let ch = builder
            .ins()
            .load(types::I8, MemFlags::trusted(), char_ptr, 0);
        let is_quote = builder.ins().icmp_imm(IntCC::Equal, ch, b'"' as i64);
        let parse_field_name = builder.create_block();
        builder
            .ins()
            .brif(is_quote, parse_field_name, &[], error_block, &[]);

        builder.switch_to_block(parse_field_name);
        // Advance past opening quote
        let pos = builder.use_var(pos_var);
        let new_pos = builder.ins().iadd_imm(pos, 1);
        builder.def_var(pos_var, new_pos);

        // Create field blocks
        let mut field_blocks = Vec::new();
        for _ in &fields {
            field_blocks.push(builder.create_block());
        }
        let default_block = builder.create_block(); // Unknown field
        let skip_to_quote_block = builder.create_block();
        let after_skip_block = builder.create_block();

        // Generate trie dispatch
        let mut blocks_to_seal = Vec::new();
        Self::emit_trie_node(
            &mut builder,
            input_ptr,
            pos_var,
            ptr_type,
            &trie,
            &field_blocks,
            default_block,
            skip_to_quote_block,
            error_block,
            &mut blocks_to_seal,
        );

        // Generate skip-to-quote loop for partial matches
        builder.switch_to_block(skip_to_quote_block);
        let pos = builder.use_var(pos_var);
        let in_bounds = builder.ins().icmp(IntCC::UnsignedLessThan, pos, len_val);
        let check_char = builder.create_block();
        builder
            .ins()
            .brif(in_bounds, check_char, &[], error_block, &[]);
        blocks_to_seal.push(check_char);

        builder.switch_to_block(check_char);
        let char_ptr = builder.ins().iadd(input_ptr, pos);
        let ch = builder
            .ins()
            .load(types::I8, MemFlags::trusted(), char_ptr, 0);
        let new_pos = builder.ins().iadd_imm(pos, 1);
        builder.def_var(pos_var, new_pos);
        let is_quote = builder.ins().icmp_imm(IntCC::Equal, ch, b'"' as i64);
        builder
            .ins()
            .brif(is_quote, after_skip_block, &[], skip_to_quote_block, &[]);

        builder.switch_to_block(after_skip_block);
        builder.ins().jump(default_block, &[]);

        // Generate field parsing blocks
        for (i, field) in fields.iter().enumerate() {
            builder.switch_to_block(field_blocks[i]);

            // Skip whitespace and expect ':'
            Self::emit_skip_ws_inline(&mut builder, input_ptr, len_val, pos_var, ptr_type);
            let pos = builder.use_var(pos_var);
            let char_ptr = builder.ins().iadd(input_ptr, pos);
            let ch = builder
                .ins()
                .load(types::I8, MemFlags::trusted(), char_ptr, 0);
            let is_colon = builder.ins().icmp_imm(IntCC::Equal, ch, b':' as i64);
            let after_colon = builder.create_block();
            builder
                .ins()
                .brif(is_colon, after_colon, &[], error_block, &[]);

            builder.switch_to_block(after_colon);
            let pos = builder.use_var(pos_var);
            let new_pos = builder.ins().iadd_imm(pos, 1);
            builder.def_var(pos_var, new_pos);

            // Skip whitespace before value
            Self::emit_skip_ws_inline(&mut builder, input_ptr, len_val, pos_var, ptr_type);

            // Calculate field pointer
            let field_offset = builder.ins().iconst(ptr_type, field.offset as i64);
            let field_ptr = builder.ins().iadd(out_ptr, field_offset);

            // Call appropriate parser
            let pos = builder.use_var(pos_var);
            let call_result = match field.parser {
                FieldParser::Skip => builder.ins().call(skip_value, &[input_ptr, len_val, pos]),
                FieldParser::NestedStruct(nested_shape) => {
                    // Use pre-compiled nested deserializer
                    let nested_ptr = nested_func_ptrs[&(nested_shape as *const Shape)];
                    let func_ptr_val = builder.ins().iconst(ptr_type, nested_ptr as i64);
                    builder.ins().call(
                        parse_nested_struct,
                        &[input_ptr, len_val, pos, field_ptr, func_ptr_val],
                    )
                }
                FieldParser::VecStruct(elem_shape) => {
                    let elem_layout = elem_shape.layout.sized_layout().unwrap();
                    let elem_size = builder.ins().iconst(ptr_type, elem_layout.size() as i64);
                    let elem_align = builder.ins().iconst(ptr_type, elem_layout.align() as i64);
                    // Use pre-compiled nested deserializer
                    let nested_ptr = nested_func_ptrs[&(elem_shape as *const Shape)];
                    let func_ptr_val = builder.ins().iconst(ptr_type, nested_ptr as i64);
                    builder.ins().call(
                        parse_vec_struct,
                        &[
                            input_ptr,
                            len_val,
                            pos,
                            field_ptr,
                            elem_size,
                            elem_align,
                            func_ptr_val,
                        ],
                    )
                }
                FieldParser::Option(option_shape) => {
                    // Get the inner type's deserializer if available
                    let inner_shape = if let Def::Option(opt_def) = option_shape.def {
                        opt_def.t()
                    } else {
                        unreachable!()
                    };

                    // Check if we have a compiled deserializer for the inner type
                    let inner_func_ptr = nested_func_ptrs
                        .get(&(inner_shape as *const Shape))
                        .copied()
                        .unwrap_or(std::ptr::null());

                    let option_shape_val = builder
                        .ins()
                        .iconst(ptr_type, option_shape as *const Shape as i64);
                    let inner_func_val = builder.ins().iconst(ptr_type, inner_func_ptr as i64);
                    builder.ins().call(
                        parse_option,
                        &[
                            input_ptr,
                            len_val,
                            pos,
                            field_ptr,
                            option_shape_val,
                            inner_func_val,
                        ],
                    )
                }
                _ => {
                    let parser_func = match field.parser {
                        FieldParser::F64 => parse_f64,
                        FieldParser::F32 => parse_f32,
                        FieldParser::I64 => parse_i64,
                        FieldParser::I32 => parse_i32,
                        FieldParser::I16 => parse_i16,
                        FieldParser::I8 => parse_i8,
                        FieldParser::U64 => parse_u64,
                        FieldParser::U32 => parse_u32,
                        FieldParser::U16 => parse_u16,
                        FieldParser::U8 => parse_u8,
                        FieldParser::Bool => parse_bool,
                        FieldParser::String => parse_string,
                        FieldParser::VecF64 => parse_vec_f64,
                        FieldParser::VecI64 => parse_vec_i64,
                        FieldParser::VecU64 => parse_vec_u64,
                        FieldParser::VecBool => parse_vec_bool,
                        FieldParser::VecVecF64 => parse_vec_vec_f64,
                        FieldParser::VecVecVecF64 => parse_vec_vec_vec_f64,
                        _ => unreachable!(),
                    };
                    builder
                        .ins()
                        .call(parser_func, &[input_ptr, len_val, pos, field_ptr])
                }
            };

            // Check result and update pos
            let result = builder.inst_results(call_result)[0];
            let is_error = builder.ins().icmp_imm(IntCC::SignedLessThan, result, 0);
            let update_pos_block = builder.create_block();
            builder
                .ins()
                .brif(is_error, error_block, &[], update_pos_block, &[]);

            builder.switch_to_block(update_pos_block);
            builder.def_var(pos_var, result); // result IS the new pos
            builder.ins().jump(after_field, &[]);

            builder.seal_block(after_colon);
            builder.seal_block(update_pos_block);
        }

        // Default block - skip unknown field value
        builder.switch_to_block(default_block);
        Self::emit_skip_ws_inline(&mut builder, input_ptr, len_val, pos_var, ptr_type);

        // Expect ':'
        let pos = builder.use_var(pos_var);
        let char_ptr = builder.ins().iadd(input_ptr, pos);
        let ch = builder
            .ins()
            .load(types::I8, MemFlags::trusted(), char_ptr, 0);
        let is_colon = builder.ins().icmp_imm(IntCC::Equal, ch, b':' as i64);
        let after_default_colon = builder.create_block();
        builder
            .ins()
            .brif(is_colon, after_default_colon, &[], error_block, &[]);

        builder.switch_to_block(after_default_colon);
        let pos = builder.use_var(pos_var);
        let new_pos = builder.ins().iadd_imm(pos, 1);
        builder.def_var(pos_var, new_pos);

        // Skip whitespace and value
        Self::emit_skip_ws_inline(&mut builder, input_ptr, len_val, pos_var, ptr_type);
        let pos = builder.use_var(pos_var);
        let skip_call = builder.ins().call(skip_value, &[input_ptr, len_val, pos]);
        let skip_result = builder.inst_results(skip_call)[0];
        let is_error = builder
            .ins()
            .icmp_imm(IntCC::SignedLessThan, skip_result, 0);
        let update_skip_pos = builder.create_block();
        builder
            .ins()
            .brif(is_error, error_block, &[], update_skip_pos, &[]);

        builder.switch_to_block(update_skip_pos);
        builder.def_var(pos_var, skip_result);
        builder.ins().jump(after_field, &[]);

        builder.seal_block(after_default_colon);
        builder.seal_block(update_skip_pos);

        // After field - check for comma or closing brace
        builder.switch_to_block(after_field);
        Self::emit_skip_ws_inline(&mut builder, input_ptr, len_val, pos_var, ptr_type);
        let pos = builder.use_var(pos_var);
        let char_ptr = builder.ins().iadd(input_ptr, pos);
        let ch = builder
            .ins()
            .load(types::I8, MemFlags::trusted(), char_ptr, 0);

        let is_comma = builder.ins().icmp_imm(IntCC::Equal, ch, b',' as i64);
        let check_close = builder.create_block();
        let advance_comma = builder.create_block();
        builder
            .ins()
            .brif(is_comma, advance_comma, &[], check_close, &[]);

        builder.switch_to_block(advance_comma);
        let pos = builder.use_var(pos_var);
        let new_pos = builder.ins().iadd_imm(pos, 1);
        builder.def_var(pos_var, new_pos);
        builder.ins().jump(field_loop, &[]);

        builder.switch_to_block(check_close);
        let is_close = builder.ins().icmp_imm(IntCC::Equal, ch, b'}' as i64);
        builder
            .ins()
            .brif(is_close, success_block, &[], error_block, &[]);

        // Success block - return new pos
        builder.switch_to_block(success_block);
        let pos = builder.use_var(pos_var);
        let final_pos = builder.ins().iadd_imm(pos, 1); // Skip closing '}'
        builder.ins().return_(&[final_pos]);

        // Error block - return error code
        builder.switch_to_block(error_block);
        let err = builder
            .ins()
            .iconst(ptr_type, helpers::ERR_UNEXPECTED_EOF as i64);
        builder.ins().return_(&[err]);

        // Seal remaining blocks
        builder.seal_block(have_char);
        builder.seal_block(after_brace_check);
        builder.seal_block(field_loop);
        builder.seal_block(parse_field_name);
        builder.seal_block(default_block);
        builder.seal_block(skip_to_quote_block);
        builder.seal_block(after_skip_block);
        builder.seal_block(after_field);
        builder.seal_block(check_close);
        builder.seal_block(advance_comma);
        builder.seal_block(start_fields);
        builder.seal_block(success_block);
        builder.seal_block(error_block);
        for block in blocks_to_seal {
            builder.seal_block(block);
        }
        for block in &field_blocks {
            builder.seal_block(*block);
        }

        builder.finalize();

        self.module.define_function(func_id, &mut ctx).unwrap();

        // Print disassembly if requested
        if want_disasm
            && let Some(compiled) = ctx.compiled_code()
            && let Some(disasm) = &compiled.vcode
        {
            eprintln!("=== JIT Disassembly for {func_name} ===");
            eprintln!("{disasm}");
            eprintln!("=== End Disassembly ===\n");
        }

        self.module.clear_context(&mut ctx);
        self.module.finalize_definitions().unwrap();

        let ptr = self.module.get_finalized_function(func_id);

        CompiledDeserializer { ptr }
    }

    /// Emit inline whitespace skipping. Updates pos_var directly.
    fn emit_skip_ws_inline(
        builder: &mut FunctionBuilder,
        input_ptr: Value,
        len_val: Value,
        pos_var: Variable,
        _ptr_type: Type,
    ) {
        let ws_loop = builder.create_block();
        let ws_body = builder.create_block();
        let ws_done = builder.create_block();

        builder.ins().jump(ws_loop, &[]);

        builder.switch_to_block(ws_loop);
        let pos = builder.use_var(pos_var);
        let in_bounds = builder.ins().icmp(IntCC::UnsignedLessThan, pos, len_val);
        builder.ins().brif(in_bounds, ws_body, &[], ws_done, &[]);

        builder.switch_to_block(ws_body);
        let char_ptr = builder.ins().iadd(input_ptr, pos);
        let ch = builder
            .ins()
            .load(types::I8, MemFlags::trusted(), char_ptr, 0);
        let ch_i32 = builder.ins().uextend(types::I32, ch);

        let is_space = builder.ins().icmp_imm(IntCC::Equal, ch_i32, 32);
        let is_tab = builder.ins().icmp_imm(IntCC::Equal, ch_i32, 9);
        let is_newline = builder.ins().icmp_imm(IntCC::Equal, ch_i32, 10);
        let is_cr = builder.ins().icmp_imm(IntCC::Equal, ch_i32, 13);

        let is_ws1 = builder.ins().bor(is_space, is_tab);
        let is_ws2 = builder.ins().bor(is_newline, is_cr);
        let is_ws = builder.ins().bor(is_ws1, is_ws2);

        let inc_block = builder.create_block();
        builder.ins().brif(is_ws, inc_block, &[], ws_done, &[]);

        builder.switch_to_block(inc_block);
        let new_pos = builder.ins().iadd_imm(pos, 1);
        builder.def_var(pos_var, new_pos);
        builder.ins().jump(ws_loop, &[]);

        builder.seal_block(ws_body);
        builder.seal_block(inc_block);
        builder.seal_block(ws_loop);
        builder.seal_block(ws_done);

        builder.switch_to_block(ws_done);
    }

    /// Emit trie node dispatch code.
    #[allow(clippy::too_many_arguments, clippy::only_used_in_recursion)]
    fn emit_trie_node(
        builder: &mut FunctionBuilder,
        input_ptr: Value,
        pos_var: Variable,
        ptr_type: Type,
        node: &TrieNode,
        field_blocks: &[Block],
        default_block: Block,
        skip_to_quote_block: Block,
        error_block: Block,
        blocks_to_seal: &mut Vec<Block>,
    ) {
        match node {
            TrieNode::Leaf(field_idx) => {
                // Matched! Check for closing quote
                let pos = builder.use_var(pos_var);
                let char_ptr = builder.ins().iadd(input_ptr, pos);
                let ch = builder
                    .ins()
                    .load(types::I8, MemFlags::trusted(), char_ptr, 0);
                let is_quote = builder.ins().icmp_imm(IntCC::Equal, ch, b'"' as i64);

                let advance_block = builder.create_block();
                blocks_to_seal.push(advance_block);
                builder
                    .ins()
                    .brif(is_quote, advance_block, &[], skip_to_quote_block, &[]);

                builder.switch_to_block(advance_block);
                let new_pos = builder.ins().iadd_imm(pos, 1);
                builder.def_var(pos_var, new_pos);
                builder.ins().jump(field_blocks[*field_idx], &[]);
            }

            TrieNode::Branch { children, terminal } => {
                let pos = builder.use_var(pos_var);
                let char_ptr = builder.ins().iadd(input_ptr, pos);
                let ch = builder
                    .ins()
                    .load(types::I8, MemFlags::trusted(), char_ptr, 0);
                let ch_i32 = builder.ins().uextend(types::I32, ch);

                // Check for terminal (closing quote means field name ends here)
                if let Some(field_idx) = terminal {
                    let is_quote = builder.ins().icmp_imm(IntCC::Equal, ch_i32, b'"' as i64);
                    let advance_terminal = builder.create_block();
                    let check_children = builder.create_block();
                    blocks_to_seal.push(advance_terminal);
                    blocks_to_seal.push(check_children);
                    builder
                        .ins()
                        .brif(is_quote, advance_terminal, &[], check_children, &[]);

                    builder.switch_to_block(advance_terminal);
                    let new_pos = builder.ins().iadd_imm(pos, 1);
                    builder.def_var(pos_var, new_pos);
                    builder.ins().jump(field_blocks[*field_idx], &[]);

                    builder.switch_to_block(check_children);
                }

                // Advance pos for the character we're about to check
                let new_pos = builder.ins().iadd_imm(pos, 1);
                builder.def_var(pos_var, new_pos);

                // Generate switch-like code for children
                let mut sorted_children: Vec<_> = children.iter().collect();
                sorted_children.sort_by_key(|(k, _)| *k);

                for (byte, child_node) in sorted_children {
                    let child_block = builder.create_block();
                    let next_block = builder.create_block();
                    blocks_to_seal.push(child_block);
                    blocks_to_seal.push(next_block);

                    let is_match = builder.ins().icmp_imm(IntCC::Equal, ch_i32, *byte as i64);
                    builder
                        .ins()
                        .brif(is_match, child_block, &[], next_block, &[]);

                    builder.switch_to_block(child_block);
                    Self::emit_trie_node(
                        builder,
                        input_ptr,
                        pos_var,
                        ptr_type,
                        child_node,
                        field_blocks,
                        default_block,
                        skip_to_quote_block,
                        error_block,
                        blocks_to_seal,
                    );

                    builder.switch_to_block(next_block);
                }

                // No match - skip to closing quote
                builder.ins().jump(skip_to_quote_block, &[]);
            }
        }
    }
}

impl Default for JitCompiler {
    fn default() -> Self {
        Self::new()
    }
}
