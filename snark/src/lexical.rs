//! Lexical and external-scanner ABI facts derived from validated grammar facts.

use crate::validated::{
    ExternalTokenDecl, ExternalTokenFact, ExternalTokenId, ExternalTokenOrdinal, GrammarExpr,
    GrammarExprId, ValidatedGrammar,
};

/// Grammar-derived lexical facts needed before parser/runtime lowering.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LexicalFacts {
    external_tokens: Vec<ExternalScannerToken>,
    valid_symbol_mask_width: usize,
    extra_roots: Vec<GrammarExprId>,
    lexical_roots: Vec<LexicalRoot>,
    terminals: Vec<TerminalFact>,
    scanner_abi: ScannerHostAbi,
}

impl LexicalFacts {
    /// Build lexical facts from a validated grammar.
    pub fn from_grammar(grammar: &ValidatedGrammar) -> Self {
        let external_tokens = grammar
            .externals()
            .iter()
            .map(ExternalScannerToken::from_fact)
            .collect::<Vec<_>>();
        let mut lexical_roots = Vec::new();
        let mut terminals = Vec::new();
        for (id, expr) in grammar.expressions() {
            match expr {
                GrammarExpr::Token(content) => lexical_roots.push(LexicalRoot {
                    id,
                    content: *content,
                    immediate: false,
                }),
                GrammarExpr::ImmediateToken(content) => lexical_roots.push(LexicalRoot {
                    id,
                    content: *content,
                    immediate: true,
                }),
                GrammarExpr::StringToken(value) => terminals.push(TerminalFact {
                    expr: id,
                    kind: TerminalKind::String,
                    spelling: value.clone(),
                }),
                GrammarExpr::PatternToken { value, .. } => terminals.push(TerminalFact {
                    expr: id,
                    kind: TerminalKind::Pattern,
                    spelling: value.clone(),
                }),
                _ => {}
            }
        }
        let valid_symbol_mask_width = external_tokens.len();
        Self {
            external_tokens,
            valid_symbol_mask_width,
            extra_roots: grammar.extras().to_vec(),
            lexical_roots,
            terminals,
            scanner_abi: ScannerHostAbi::new(valid_symbol_mask_width),
        }
    }

    /// External scanner tokens in grammar ordinal order.
    pub fn external_tokens(&self) -> &[ExternalScannerToken] {
        &self.external_tokens
    }

    /// Width of the valid-symbol mask passed to scanner calls.
    pub fn valid_symbol_mask_width(&self) -> usize {
        self.valid_symbol_mask_width
    }

    /// Extra rule roots skipped between normal tokens.
    pub fn extra_roots(&self) -> &[GrammarExprId] {
        &self.extra_roots
    }

    /// Token and immediate-token lexical roots.
    pub fn lexical_roots(&self) -> &[LexicalRoot] {
        &self.lexical_roots
    }

    /// Literal and regex terminal expressions.
    pub fn terminals(&self) -> &[TerminalFact] {
        &self.terminals
    }

    /// External scanner host ABI facts.
    pub fn scanner_abi(&self) -> &ScannerHostAbi {
        &self.scanner_abi
    }
}

/// One external scanner token entry.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ExternalScannerToken {
    id: ExternalTokenId,
    ordinal: ExternalTokenOrdinal,
    name: Option<String>,
    declaration: ExternalTokenDecl,
}

impl ExternalScannerToken {
    fn from_fact(fact: &ExternalTokenFact) -> Self {
        Self {
            id: fact.id(),
            ordinal: fact.ordinal(),
            name: fact.name().map(str::to_owned),
            declaration: fact.declaration().clone(),
        }
    }

    /// External token id.
    pub const fn id(&self) -> ExternalTokenId {
        self.id
    }

    /// Ordinal in the scanner valid-symbol mask.
    pub const fn ordinal(&self) -> ExternalTokenOrdinal {
        self.ordinal
    }

    /// Optional token name.
    pub fn name(&self) -> Option<&str> {
        self.name.as_deref()
    }

    /// External token declaration.
    pub const fn declaration(&self) -> &ExternalTokenDecl {
        &self.declaration
    }
}

/// Root of a lexical token expression.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct LexicalRoot {
    /// Wrapper expression id.
    pub id: GrammarExprId,
    /// Wrapped expression id.
    pub content: GrammarExprId,
    /// Whether this came from `token.immediate`.
    pub immediate: bool,
}

/// Literal or regex terminal expression.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TerminalFact {
    /// Terminal expression id.
    pub expr: GrammarExprId,
    /// Terminal kind.
    pub kind: TerminalKind,
    /// Literal text or regex source.
    pub spelling: String,
}

/// Terminal kind.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum TerminalKind {
    /// Literal string token.
    String,
    /// Regex pattern token.
    Pattern,
}

/// External scanner host ABI contract facts.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ScannerHostAbi {
    valid_symbol_mask_width: usize,
    operations: Vec<ScannerHostOperation>,
    supports_serialized_state: bool,
}

impl ScannerHostAbi {
    fn new(valid_symbol_mask_width: usize) -> Self {
        Self {
            valid_symbol_mask_width,
            operations: vec![
                ScannerHostOperation::Advance,
                ScannerHostOperation::MarkEnd,
                ScannerHostOperation::SetResultSymbol,
                ScannerHostOperation::IsAtEnd,
                ScannerHostOperation::Serialize,
                ScannerHostOperation::Deserialize,
            ],
            supports_serialized_state: true,
        }
    }

    /// Width of the valid-symbol mask passed to scanner calls.
    pub fn valid_symbol_mask_width(&self) -> usize {
        self.valid_symbol_mask_width
    }

    /// Host operations visible to scanner implementations.
    pub fn operations(&self) -> &[ScannerHostOperation] {
        &self.operations
    }

    /// Whether scanner state serialization is part of the ABI contract.
    pub fn supports_serialized_state(&self) -> bool {
        self.supports_serialized_state
    }
}

/// Operations the scanner host must provide.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum ScannerHostOperation {
    /// Advance the input cursor.
    Advance,
    /// Mark the end of the accepted token.
    MarkEnd,
    /// Set the accepted external token symbol.
    SetResultSymbol,
    /// Test for end-of-input.
    IsAtEnd,
    /// Serialize scanner state.
    Serialize,
    /// Deserialize scanner state.
    Deserialize,
}
