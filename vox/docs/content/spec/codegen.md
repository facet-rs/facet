+++
title = "Code Generation"
description = "Code generation architecture and IR"
weight = 75
+++

This document describes how Rapace generates client and server bindings from service definitions. Rapace uses **facet** for runtime type introspection and provides code generators for multiple target languages.

## Architecture Overview

```
┌─────────────────────────────────────────────────────────────┐
│                     Rust Source Code                         │
│  #[rapace::service]                                          │
│  trait Calculator {                                          │
│      async fn add(&self, a: i32, b: i32) -> i32;            │
│  }                                                           │
└─────────────────────────────────────────────────────────────┘
                              │
                              ▼
┌─────────────────────────────────────────────────────────────┐
│                    rapace-macros                             │
│  - Parses trait definition                                   │
│  - Computes method IDs (FNV-1a hash)                        │
│  - Generates Client<T>, Server<S>                           │
│  - Generates register function                               │
└─────────────────────────────────────────────────────────────┘
                              │
                              ▼
┌─────────────────────────────────────────────────────────────┐
│                   ServiceRegistry                            │
│  - Stores MethodEntry with facet Shapes                     │
│  - Enables runtime introspection                             │
│  - Powers cross-language codegen                             │
└─────────────────────────────────────────────────────────────┘
                              │
              ┌───────────────┼───────────────┐
              ▼               ▼               ▼
         SwiftCodegen   TypeScriptCodegen   (future)
              │               │
              ▼               ▼
         Generated.swift  Generated.ts
```

## Rust Code Generation (Proc Macros)

The `#[rapace::service]` attribute macro generates all Rust bindings at compile time.

### Generated Artifacts

For a service trait `Foo`:

| Generated Item | Purpose |
|----------------|---------|
| `trait Foo` | Rewritten with `-> impl Future + Send` for RPITIT |
| `FooClient<T: Transport>` | Client stub with method ID constants |
| `FooRegistryClient<T>` | Client with registry-resolved method IDs |
| `FooServer<S: Foo>` | Server dispatcher |
| `FooDispatch<S>` | `ServiceDispatch` wrapper for multi-service cells |
| `FOO_METHOD_ID_*` | Method ID constants |
| `foo_register(registry)` | Registration function |

### Method ID Computation

r[codegen.method-id.computation]
Method IDs MUST be computed using FNV-1a hash of `"ServiceName.method_name"`. See [Core Protocol: Method ID Computation](@/spec/core.md#method-id-computation) for the complete algorithm.

r[codegen.method-id.collision]
Hash collisions within a service MUST be detected at macro expansion time and produce a compile error. Cross-service collisions MUST be detected at runtime during registration.

### Trait Rewriting

The macro rewrites `async fn` methods to use RPITIT (Return Position Impl Trait in Trait) for `Send` futures:

```rust
// Input
async fn add(&self, a: i32, b: i32) -> i32;

// Output
fn add(&self, a: i32, b: i32) -> impl Future<Output = i32> + Send + '_;
```

This allows `RpcSession` to spawn dispatch futures with `tokio::spawn`.

### Argument Encoding

r[codegen.args.encoding]
Multiple arguments MUST be encoded as tuples:

| Arguments | Wire Encoding |
|-----------|---------------|
| `()` | Unit (empty payload) |
| `(a: T)` | Single value `T` |
| `(a: T, b: U)` | Tuple `(T, U)` |
| `(a: T, b: U, c: V)` | Tuple `(T, U, V)` |

### Streaming Methods

Methods with streaming arguments or return types use attached STREAM channels as defined in [Core Protocol: STREAM Channels](@/spec/core.md#stream-channels):

```rust
async fn subscribe(&self, topic: String) -> Stream<Event>;
async fn upload(&self, meta: Meta, data: Stream<Chunk>) -> Result;
```

For server-streaming (`-> Stream<T>`):
1. Client sends request on CALL channel with `DATA | EOS`
2. Server opens a STREAM channel attached to the call (port 101)
3. Server sends items on the STREAM channel, final item with `EOS`
4. Server sends response on CALL channel with `DATA | EOS | RESPONSE`

For client-streaming (`Stream<T>` argument):
1. Client opens a STREAM channel attached to the call (port 1)
2. Client sends request on CALL channel with `DATA | EOS`
3. Client sends items on the STREAM channel, final item with `EOS`
4. Server processes and sends response on CALL channel

See [Core Protocol](@/spec/core.md#stream-channels) for the complete attachment model and port ID assignment.

## Service Registry

The `ServiceRegistry` provides runtime introspection of registered services.

### Registry Structure

```rust
pub struct ServiceRegistry {
    services_by_name: HashMap<&'static str, ServiceEntry>,
    methods_by_id: HashMap<MethodId, MethodLookup>,
}

pub struct ServiceEntry {
    pub id: ServiceId,
    pub name: &'static str,
    pub doc: String,
    pub methods: HashMap<&'static str, MethodEntry>,
}

pub struct MethodEntry {
    pub id: MethodId,
    pub name: &'static str,
    pub full_name: String,           // "Service.method"
    pub doc: String,
    pub args: Vec<ArgInfo>,
    pub request_shape: &'static Shape,
    pub response_shape: &'static Shape,
    pub is_streaming: bool,
    pub supported_encodings: Vec<Encoding>,
}
```

### Facet Shape Integration

Request and response types are captured via facet's `Shape`:

```rust
// Generated register function
pub fn calculator_register(registry: &mut ServiceRegistry) {
    let mut builder = registry.register_service("Calculator", "A calculator service");
    builder.add_method(
        "add",
        "Add two numbers",
        vec![
            ArgInfo { name: "a", type_name: "i32" },
            ArgInfo { name: "b", type_name: "i32" },
        ],
        <(i32, i32) as Facet>::SHAPE,  // request shape
        <i32 as Facet>::SHAPE,          // response shape
    );
    builder.finish();
}
```

The `Shape` contains:
- Type identifier (e.g., `"my_crate::Foo"`)
- Type definition (`Struct`, `Enum`, `Scalar`, etc.)
- Field names, types, and order
- Variant discriminants for enums

### Auto-Registration

Servers auto-register on first instantiation:

```rust
impl<S: Calculator + Send + Sync + 'static> CalculatorServer<S> {
    fn __auto_register() {
        static REGISTERED: OnceLock<()> = OnceLock::new();
        REGISTERED.get_or_init(|| {
            ServiceRegistry::with_global_mut(|registry| {
                calculator_register(registry);
            });
        });
    }

    pub fn new(service: S) -> Self {
        Self::__auto_register();
        Self { service }
    }
}
```

## Cross-Language Code Generation

Swift and TypeScript codegen read from the `ServiceRegistry` at build time.

### Code Generator Interface

Both generators follow the same pattern:

```rust
pub struct SwiftCodegen {
    output: String,
    generated_types: HashSet<&'static str>,
    indent: usize,
}

impl SwiftCodegen {
    pub fn generate_from_registry(&mut self, registry: &ServiceRegistry) {
        // 1. Collect all types from method signatures
        let mut types_to_generate: Vec<&'static Shape> = Vec::new();
        for service in registry.services() {
            for method in service.iter_methods() {
                self.collect_types(method.request_shape, &mut types_to_generate);
                self.collect_types(method.response_shape, &mut types_to_generate);
            }
        }

        // 2. Generate types in dependency order
        for shape in types_to_generate {
            self.generate_type(shape);
        }

        // 3. Generate client classes
        for service in registry.services() {
            self.generate_client(service);
        }
    }
}
```

### Type Collection Algorithm

Types are collected recursively, visiting nested types before containers:

1. Skip already-visited types (deduplication)
2. For container types (`Option<T>`, `Vec<T>`, `HashMap<K,V>`):
   - Recursively collect inner types
3. For user types (structs, enums):
   - Recursively collect field/variant types
   - Add the type itself to the list
4. Skip tuple types (not generated as named types)

### Shape to Type Mapping

The `shape_to_*_type` functions map facet shapes to target language types:

```rust
fn shape_to_swift_type(&self, shape: &'static Shape) -> String {
    // Unit type
    if shape.type_identifier == "()" {
        return "Void".to_string();
    }

    // Container types
    match &shape.def {
        Def::Option(opt) => format!("{}?", self.shape_to_swift_type(opt.t())),
        Def::List(list) => format!("[{}]", self.shape_to_swift_type(list.t())),
        Def::Map(map) => format!("[{}: {}]", ...),
        _ => {}
    }

    // Scalar types
    if let Some(scalar) = shape.scalar_type() {
        return scalar_to_swift(scalar).to_string();
    }

    // User types
    match &shape.ty {
        Type::User(UserType::Struct(_)) | Type::User(UserType::Enum(_)) => {
            clean_type_name(shape.type_identifier)
        }
        _ => "Any".to_string(),
    }
}
```

### Generated Encoder/Decoder Functions

**TypeScript** generates standalone functions:

```typescript
export function encodeUserInfo(encoder: PostcardEncoder, value: UserInfo): void {
    encoder.string(value.name);
    encoder.u32(value.age);
}

export function decodeUserInfo(decoder: PostcardDecoder): UserInfo {
    return {
        name: decoder.string(),
        age: decoder.u32(),
    };
}
```

**Swift** generates methods conforming to `PostcardEncodable`:

```swift
public struct UserInfo: PostcardEncodable, Sendable {
    public func encode(to encoder: inout PostcardEncoder) {
        encoder.encode(name)
        encoder.encode(age)
    }
}
```

### Client Class Generation

Clients are generated with:
- Constructor taking transport/client
- Static factory for connection
- Async methods for each RPC
- Method ID embedded as constant

```typescript
export class CalculatorClient {
    async add(a: number, b: number): Promise<number> {
        const encoder = new PostcardEncoder();
        encoder.i32(a);
        encoder.i32(b);
        const response = await this.client.call(0x12345678, encoder.bytes);
        const decoder = new PostcardDecoder(response);
        return decoder.i32();
    }
}
```

## Build-Time Workflow

### Rust Build

1. `#[rapace::service]` macro expands at compile time
2. Generated code includes:
   - Client/server types
   - Method ID constants
   - Register function
3. No runtime code generation

### Cross-Language Build

Typically done in a build script or xtask:

```rust
// build.rs or xtask
fn main() {
    // 1. Register services (triggers auto-registration)
    let _ = CalculatorServer::new(DummyImpl);

    // 2. Generate Swift code
    let mut swift = SwiftCodegen::new();
    ServiceRegistry::with_global(|registry| {
        swift.generate_from_registry(registry);
    });
    std::fs::write("Generated.swift", swift.output()).unwrap();

    // 3. Generate TypeScript code
    let mut ts = TypeScriptCodegen::new();
    ServiceRegistry::with_global(|registry| {
        ts.generate_from_registry(registry);
    });
    std::fs::write("generated.ts", ts.output()).unwrap();
}
```

## Limitations and Future Work

### Current Limitations

1. **No generics on traits or methods**: The macro rejects generic parameters
2. **No supertraits**: Service traits cannot extend other traits
3. **Tuple types not generated**: Methods using tuple arguments should use structs
4. **Streaming methods incomplete**: Swift/TypeScript generators mark as TODO
5. **No server generation for non-Rust**: Only client stubs are generated

### Planned Improvements

1. **Schema hash integration**: Include `sig_hash` in generated code for handshake validation
2. **Bidirectional streaming**: Client-streaming and bidirectional RPC
3. **Server stubs**: Generate server interfaces for Swift/TypeScript
4. **Protocol buffers interop**: Optional protobuf schema export

## Next Steps

- [Language Mappings](@/spec/language-mappings.md) – Per-language type mappings
- [Schema Evolution](@/spec/schema-evolution.md) – Hash computation details
- [Frame Format](@/spec/frame-format.md) – Wire format for frames
