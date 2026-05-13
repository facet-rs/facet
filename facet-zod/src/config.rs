//! Configuration knobs for Zod schema generation.

/// Controls how Rust `Option<T>` fields are mapped to Zod modifiers.
pub enum OptionalMode {
    /// Map to `.optional()` — value may be omitted (`undefined`).
    Optional,
    /// Map to `.nullable()` — value may be explicitly `null`.
    Nullable,
    /// Map to `.nullish()` — value may be `null` or `undefined`.
    Nullish,
}

/// Controls whether large Rust integer types emit as `z.bigint()` or stay as `z.number().int()`.
pub enum BigIntMode {
    /// Always emit `z.number().int()` regardless of width.
    Never,
    /// Emit `z.bigint()` for integer types whose layout is 8 bytes or larger.
    From64Bit,
}

/// Controls what `export` declarations are emitted per named schema.
pub enum ExportStyle {
    /// Emit both the `const ...Schema` value and the inferred `type`.
    ConstAndType,
    /// Emit only the `const ...Schema` value.
    ConstOnly,
    /// Emit only the inferred `type`.
    TypeOnly,
}

/// Generator configuration.
pub struct Config {
    /// How `Option<T>` fields are rendered.
    pub optional_mode: OptionalMode,
    /// When to widen integer types to `z.bigint()`.
    pub bigint_mode: BigIntMode,
    /// Which `export` declarations to emit per named schema.
    pub export_style: ExportStyle,
    /// Optional header prepended to the generated file (e.g. `import { z } from 'zod';`).
    pub header: Option<String>,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            optional_mode: OptionalMode::Nullish,
            bigint_mode: BigIntMode::Never,
            export_style: ExportStyle::ConstAndType,
            header: None,
        }
    }
}
