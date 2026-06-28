//! Snark dialect scaffold for Weavy lowering.
//!
//! This module names the target shape before the full lowering implementation
//! exists: validated Snark grammar, scanner, parser, query, recovery, and
//! incremental facts should become typed Snark intrinsics inside canonical
//! Weavy programs.

use weavy::ir::{
    EffectContract, EffectResource, IntrinsicDescriptor, IntrinsicOp, WeavyLowered, WeavyOp,
};

/// A lowered Snark program carried by canonical Weavy ops.
pub type SnarkWeavyLowered = WeavyLowered<SnarkBlockId, SnarkIntrinsic>;

/// One canonical Snark/Weavy operation.
pub type SnarkWeavyOp = WeavyOp<SnarkBlockId, SnarkIntrinsic>;

macro_rules! id_type {
    ($name:ident, $doc:literal) => {
        #[doc = $doc]
        #[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
        pub struct $name(u32);

        impl $name {
            /// Return the dense numeric identity.
            #[must_use]
            pub const fn get(self) -> u32 {
                self.0
            }
        }
    };
}

id_type!(
    SnarkBlockId,
    "Symbolic block identity in a Snark Weavy lowering."
);
id_type!(
    ParseStateId,
    "Snark parser state identity generated from validated grammar facts."
);
id_type!(
    SymbolId,
    "Snark symbol identity generated from validated grammar facts."
);
id_type!(
    TerminalId,
    "Snark terminal symbol identity generated from validated grammar facts."
);
id_type!(
    NonterminalId,
    "Snark nonterminal symbol identity generated from validated grammar facts."
);
id_type!(
    ProductionId,
    "Snark production identity generated from validated grammar facts."
);
id_type!(
    FieldId,
    "Snark field identity generated from validated grammar facts."
);
id_type!(
    LexModeId,
    "Snark lexical mode identity generated from validated grammar facts."
);
id_type!(
    AliasSequenceId,
    "Snark alias-sequence identity generated from validated grammar facts."
);
id_type!(
    ExternalScannerStateId,
    "Snark external scanner state identity for scanner replay."
);

/// A domain intrinsic emitted by Snark lowering.
#[derive(Clone, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub enum SnarkIntrinsic {
    /// Read the next token according to a Snark lexical mode.
    Lex {
        /// Lexical mode selected by the current parser state.
        mode: LexModeId,
    },
    /// Placeholder for an external scanner call.
    ///
    /// This does not define Snark's scanner host ABI. The valid-symbol mask,
    /// cursor, mark-end, result-symbol, EOF, and serialization contracts belong
    /// to the scanner ABI layer before this intrinsic becomes executable.
    CallExternalScanner {
        /// Parser state whose valid-symbol set is being offered.
        state: ParseStateId,
        /// Scanner state selected by Snark's parser runtime.
        scanner_state: ExternalScannerStateId,
    },
    /// Shift the current lookahead and enter another parser state.
    Shift {
        /// Parser state to enter after the shift.
        state: ParseStateId,
    },
    /// Reduce a production into a nonterminal symbol.
    Reduce {
        /// Production being reduced.
        production: ProductionId,
        /// Symbol emitted by the reduction.
        symbol: NonterminalId,
        /// Optional alias sequence applied to visible children.
        aliases: Option<AliasSequenceId>,
    },
    /// Recover through Snark's generated recovery path.
    Recover {
        /// Parser state whose recovery action is being executed.
        state: ParseStateId,
    },
    /// Emit a visible syntax node or token event.
    EmitNode {
        /// Symbol represented by the emitted event.
        symbol: SymbolId,
    },
    /// Emit a query capture into the query result sink.
    EmitCapture {
        /// Optional field associated with the capture.
        field: Option<FieldId>,
    },
}

impl SnarkIntrinsic {
    /// The Weavy dialect name used by Snark intrinsics.
    pub const DIALECT: &'static str = "snark.tree_sitter";
}

impl IntrinsicOp for SnarkIntrinsic {
    fn descriptor(&self) -> IntrinsicDescriptor {
        let name = match self {
            Self::Lex { .. } => "lex",
            Self::CallExternalScanner { .. } => "call_external_scanner",
            Self::Shift { .. } => "shift",
            Self::Reduce { .. } => "reduce",
            Self::Recover { .. } => "recover",
            Self::EmitNode { .. } => "emit_node",
            Self::EmitCapture { .. } => "emit_capture",
        };
        IntrinsicDescriptor {
            dialect: Self::DIALECT,
            name,
        }
    }

    fn effect(&self) -> EffectContract {
        match self {
            Self::Lex { .. } => EffectContract::new()
                .read_resource(EffectResource::Input("source"))
                .advance_resource(EffectResource::Input("source"))
                .may_fail(),
            Self::CallExternalScanner { .. } => EffectContract::new()
                .read_resource(EffectResource::Input("source"))
                .advance_resource(EffectResource::Input("source"))
                .read_resource(EffectResource::SideChannel("scanner_state"))
                .write_resource(EffectResource::SideChannel("scanner_state"))
                .may_fail()
                .calls_user_code(),
            Self::Shift { .. } => EffectContract::new()
                .read_resource(EffectResource::SideChannel("parser_stack"))
                .write_resource(EffectResource::SideChannel("parser_stack")),
            Self::Reduce { .. } => EffectContract::new()
                .read_resource(EffectResource::SideChannel("parser_stack"))
                .write_resource(EffectResource::SideChannel("parser_stack"))
                .write_resource(EffectResource::Sink("tree_events")),
            Self::Recover { .. } => EffectContract::new()
                .read_resource(EffectResource::Input("source"))
                .advance_resource(EffectResource::Input("source"))
                .write_resource(EffectResource::SideChannel("parser_stack"))
                .write_resource(EffectResource::Sink("tree_events"))
                .may_fail(),
            Self::EmitNode { .. } => {
                EffectContract::new().write_resource(EffectResource::Sink("tree_events"))
            }
            Self::EmitCapture { .. } => {
                EffectContract::new().write_resource(EffectResource::Sink("query_captures"))
            }
        }
    }
}

/// Build the empty initial lowered program.
///
/// This is intentionally only a carrier smoke check. Full lowering must fill
/// this with validated Snark parser/scanner/query facts rather than raw
/// recursive grammar interpretation or generated Tree-sitter implementation
/// files.
#[must_use]
pub fn empty_lowered() -> SnarkWeavyLowered {
    WeavyLowered::new(Vec::new())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn intrinsic_descriptors_use_snark_tree_sitter_dialect() {
        let descriptor = SnarkIntrinsic::Shift {
            state: ParseStateId(7),
        }
        .descriptor();

        assert_eq!(descriptor.dialect, "snark.tree_sitter");
        assert_eq!(descriptor.name, "shift");
    }

    #[test]
    fn empty_lowered_has_no_program_or_blocks() {
        let lowered = empty_lowered();

        assert!(lowered.program.is_empty());
        assert!(lowered.blocks.is_empty());
    }
}
