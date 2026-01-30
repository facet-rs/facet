//! Semantic token computation for syntax highlighting

use styx_cst::{Parse, SyntaxKind, SyntaxNode, SyntaxToken};
use tower_lsp::lsp_types::*;

/// Semantic token types we support
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u32)]
pub enum TokenType {
    /// Comments
    Comment = 0,
    /// String/scalar values
    String = 1,
    /// Numeric values (schema-aware)
    Number = 2,
    /// Keywords (for booleans if schema-aware)
    Keyword = 3,
    /// Type names (tags)
    Type = 4,
    /// Enum members (enum variant tags)
    EnumMember = 5,
    /// Properties (object keys)
    Property = 6,
    /// Operators (@ =)
    Operator = 7,
}

impl TokenType {
    #[allow(dead_code)]
    pub const COUNT: usize = 8;

    pub fn as_str(self) -> &'static str {
        match self {
            TokenType::Comment => "comment",
            TokenType::String => "string",
            TokenType::Number => "number",
            TokenType::Keyword => "keyword",
            TokenType::Type => "type",
            TokenType::EnumMember => "enumMember",
            TokenType::Property => "property",
            TokenType::Operator => "operator",
        }
    }
}

/// Semantic token modifiers we support
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u32)]
pub enum TokenModifier {
    /// Documentation comments
    Documentation = 0,
    /// Deprecated (from schema)
    Deprecated = 1,
}

impl TokenModifier {
    #[allow(dead_code)]
    pub const COUNT: usize = 2;

    pub fn as_str(self) -> &'static str {
        match self {
            TokenModifier::Documentation => "documentation",
            TokenModifier::Deprecated => "deprecated",
        }
    }
}

/// Build the semantic token legend for LSP
pub fn semantic_token_legend() -> SemanticTokensLegend {
    SemanticTokensLegend {
        token_types: vec![
            SemanticTokenType::new(TokenType::Comment.as_str()),
            SemanticTokenType::new(TokenType::String.as_str()),
            SemanticTokenType::new(TokenType::Number.as_str()),
            SemanticTokenType::new(TokenType::Keyword.as_str()),
            SemanticTokenType::new(TokenType::Type.as_str()),
            SemanticTokenType::new(TokenType::EnumMember.as_str()),
            SemanticTokenType::new(TokenType::Property.as_str()),
            SemanticTokenType::new(TokenType::Operator.as_str()),
        ],
        token_modifiers: vec![
            SemanticTokenModifier::new(TokenModifier::Documentation.as_str()),
            SemanticTokenModifier::new(TokenModifier::Deprecated.as_str()),
        ],
    }
}

/// A semantic token before encoding (for LSP)
#[derive(Debug)]
struct RawToken {
    line: u32,
    start_char: u32,
    length: u32,
    token_type: TokenType,
    modifiers: u32,
}

/// A highlight span with byte range (for CLI/terminal output)
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HighlightSpan {
    /// Byte offset where the span starts
    pub start: usize,
    /// Byte offset where the span ends (exclusive)
    pub end: usize,
    /// The type of token this span represents
    pub token_type: TokenType,
    /// Whether this is a doc comment
    pub is_doc_comment: bool,
}

/// Context for semantic token collection
#[derive(Clone, Copy, Default)]
struct WalkContext {
    /// Are we inside a sequence? (affects how keys are highlighted)
    in_sequence: bool,
}

/// Compute semantic tokens for a parsed document
pub fn compute_semantic_tokens(parse: &Parse) -> Vec<SemanticToken> {
    let content = parse.syntax().to_string();
    let mut raw_tokens = Vec::new();

    // Walk the CST and emit tokens
    walk_node(
        &parse.syntax(),
        &content,
        &mut raw_tokens,
        WalkContext::default(),
    );

    // Sort tokens by position
    raw_tokens.sort_by(|a, b| a.line.cmp(&b.line).then(a.start_char.cmp(&b.start_char)));

    // Encode as delta positions
    encode_tokens(&raw_tokens)
}

/// Compute highlight spans with byte ranges for terminal/CLI output
pub fn compute_highlight_spans(parse: &Parse) -> Vec<HighlightSpan> {
    let mut spans = Vec::new();
    walk_node_for_spans(&parse.syntax(), &mut spans, WalkContext::default());

    // Sort spans by start position
    spans.sort_by_key(|s| s.start);
    spans
}

/// Recursively walk a syntax node and collect highlight spans with byte ranges
fn walk_node_for_spans(node: &SyntaxNode, spans: &mut Vec<HighlightSpan>, ctx: WalkContext) {
    match node.kind() {
        SyntaxKind::KEY => {
            if ctx.in_sequence {
                collect_key_spans_as_values(node, spans);
            } else {
                collect_key_spans(node, spans);
            }
        }
        SyntaxKind::TAG => {
            for child in node.children_with_tokens() {
                if let Some(token) = child.as_token() {
                    if token.kind() == SyntaxKind::TAG_TOKEN {
                        add_span_from_syntax(spans, token, TokenType::Type, false);
                    }
                } else if let Some(child_node) = child.as_node()
                    && child_node.kind() == SyntaxKind::TAG_PAYLOAD
                {
                    walk_node_for_spans(child_node, spans, ctx);
                }
            }
        }
        SyntaxKind::SEQUENCE => {
            let seq_ctx = WalkContext { in_sequence: true };
            for child in node.children_with_tokens() {
                if let Some(child_node) = child.as_node() {
                    walk_node_for_spans(child_node, spans, seq_ctx);
                }
            }
        }
        SyntaxKind::UNIT => {
            for child in node.children_with_tokens() {
                if let Some(token) = child.as_token()
                    && (token.kind() == SyntaxKind::TAG_TOKEN || token.kind() == SyntaxKind::AT)
                {
                    add_span_from_syntax(spans, token, TokenType::Type, false);
                }
            }
        }
        SyntaxKind::SCALAR => {
            for child in node.children_with_tokens() {
                if let Some(token) = child.as_token()
                    && is_scalar_token(token.kind())
                {
                    add_span_from_syntax(spans, token, TokenType::String, false);
                }
            }
        }
        SyntaxKind::HEREDOC => {
            for child in node.children_with_tokens() {
                if let Some(token) = child.as_token() {
                    match token.kind() {
                        SyntaxKind::HEREDOC_START | SyntaxKind::HEREDOC_END => {
                            add_span_from_syntax(spans, token, TokenType::Operator, false);
                        }
                        SyntaxKind::HEREDOC_CONTENT => {
                            add_span_from_syntax(spans, token, TokenType::String, false);
                        }
                        _ => {}
                    }
                }
            }
        }
        SyntaxKind::ATTRIBUTE => {
            for child in node.children_with_tokens() {
                if let Some(token) = child.as_token() {
                    if token.kind() == SyntaxKind::GT {
                        add_span_from_syntax(spans, token, TokenType::Operator, false);
                    }
                } else if let Some(child_node) = child.as_node() {
                    if child_node.kind() == SyntaxKind::KEY {
                        collect_key_spans(child_node, spans);
                    } else {
                        walk_node_for_spans(child_node, spans, ctx);
                    }
                }
            }
        }
        _ => {
            // Handle comments at token level
            for child in node.children_with_tokens() {
                if let Some(token) = child.as_token() {
                    match token.kind() {
                        SyntaxKind::LINE_COMMENT => {
                            add_span_from_syntax(spans, token, TokenType::Comment, false);
                        }
                        SyntaxKind::DOC_COMMENT => {
                            add_span_from_syntax(spans, token, TokenType::Comment, true);
                        }
                        _ => {}
                    }
                } else if let Some(child_node) = child.as_node() {
                    walk_node_for_spans(child_node, spans, ctx);
                }
            }
        }
    }
}

/// Collect tokens from a KEY node (object property)
fn collect_key_spans(node: &SyntaxNode, spans: &mut Vec<HighlightSpan>) {
    for child in node.children_with_tokens() {
        if let Some(child_node) = child.as_node() {
            match child_node.kind() {
                SyntaxKind::SCALAR => {
                    for t in child_node.children_with_tokens() {
                        if let Some(token) = t.as_token()
                            && is_scalar_token(token.kind())
                        {
                            add_span_from_syntax(spans, token, TokenType::Property, false);
                        }
                    }
                }
                SyntaxKind::TAG => {
                    // Tagged key like @schema
                    for t in child_node.children_with_tokens() {
                        if let Some(token) = t.as_token()
                            && token.kind() == SyntaxKind::TAG_TOKEN
                        {
                            add_span_from_syntax(spans, token, TokenType::Property, false);
                        }
                    }
                }
                SyntaxKind::UNIT => {
                    for t in child_node.children_with_tokens() {
                        if let Some(token) = t.as_token()
                            && (token.kind() == SyntaxKind::TAG_TOKEN
                                || token.kind() == SyntaxKind::AT)
                        {
                            add_span_from_syntax(spans, token, TokenType::Type, false);
                        }
                    }
                }
                _ => {}
            }
        }
    }
}

/// Collect tokens from a KEY node as values (in sequence context)
fn collect_key_spans_as_values(node: &SyntaxNode, spans: &mut Vec<HighlightSpan>) {
    for child in node.children_with_tokens() {
        if let Some(child_node) = child.as_node() {
            match child_node.kind() {
                SyntaxKind::SCALAR => {
                    for t in child_node.children_with_tokens() {
                        if let Some(token) = t.as_token()
                            && is_scalar_token(token.kind())
                        {
                            add_span_from_syntax(spans, token, TokenType::String, false);
                        }
                    }
                }
                SyntaxKind::TAG => {
                    walk_node_for_spans(child_node, spans, WalkContext { in_sequence: true });
                }
                SyntaxKind::UNIT => {
                    for t in child_node.children_with_tokens() {
                        if let Some(token) = t.as_token()
                            && (token.kind() == SyntaxKind::TAG_TOKEN
                                || token.kind() == SyntaxKind::AT)
                        {
                            add_span_from_syntax(spans, token, TokenType::Type, false);
                        }
                    }
                }
                SyntaxKind::SEQUENCE => {
                    walk_node_for_spans(child_node, spans, WalkContext { in_sequence: true });
                }
                SyntaxKind::OBJECT => {
                    walk_node_for_spans(child_node, spans, WalkContext::default());
                }
                _ => {}
            }
        }
    }
}

/// Add a highlight span from a syntax token
fn add_span_from_syntax(
    spans: &mut Vec<HighlightSpan>,
    token: &SyntaxToken,
    token_type: TokenType,
    is_doc_comment: bool,
) {
    let start: usize = token.text_range().start().into();
    let end: usize = token.text_range().end().into();

    spans.push(HighlightSpan {
        start,
        end,
        token_type,
        is_doc_comment,
    });
}

/// Recursively walk a syntax node and collect semantic tokens
fn walk_node(node: &SyntaxNode, content: &str, tokens: &mut Vec<RawToken>, ctx: WalkContext) {
    match node.kind() {
        SyntaxKind::KEY => {
            // Key nodes contain scalars or tags
            // In a sequence context, keys are actually values (strings)
            // In object context, keys are properties
            if ctx.in_sequence {
                collect_key_tokens_as_values(node, content, tokens);
            } else {
                collect_key_tokens(node, content, tokens);
            }
        }
        SyntaxKind::TAG => {
            // Tag nodes - TAG_TOKEN is the full @name
            for child in node.children_with_tokens() {
                if let Some(token) = child.as_token() {
                    if token.kind() == SyntaxKind::TAG_TOKEN {
                        add_token_from_syntax(tokens, content, token, TokenType::Type, 0);
                    }
                } else if let Some(child_node) = child.as_node()
                    && child_node.kind() == SyntaxKind::TAG_PAYLOAD
                {
                    // Recurse into payload
                    walk_node(child_node, content, tokens, ctx);
                }
            }
        }
        SyntaxKind::SEQUENCE => {
            // Sequence - entries inside are values, not key-value pairs
            let seq_ctx = WalkContext { in_sequence: true };
            for child in node.children_with_tokens() {
                if let Some(child_node) = child.as_node() {
                    walk_node(child_node, content, tokens, seq_ctx);
                }
            }
        }
        SyntaxKind::UNIT => {
            // Unit @ token
            for child in node.children_with_tokens() {
                if let Some(token) = child.as_token()
                    && (token.kind() == SyntaxKind::TAG_TOKEN || token.kind() == SyntaxKind::AT)
                {
                    add_token_from_syntax(tokens, content, token, TokenType::Type, 0);
                }
            }
        }
        SyntaxKind::SCALAR => {
            // Scalar values (not in KEY position) get string highlighting
            for child in node.children_with_tokens() {
                if let Some(token) = child.as_token()
                    && is_scalar_token(token.kind())
                {
                    add_token_from_syntax(tokens, content, token, TokenType::String, 0);
                }
            }
        }
        SyntaxKind::HEREDOC => {
            // Heredoc - highlight markers and content
            for child in node.children_with_tokens() {
                if let Some(token) = child.as_token() {
                    match token.kind() {
                        SyntaxKind::HEREDOC_START | SyntaxKind::HEREDOC_END => {
                            add_token_from_syntax(tokens, content, token, TokenType::Operator, 0);
                        }
                        SyntaxKind::HEREDOC_CONTENT => {
                            add_token_from_syntax(tokens, content, token, TokenType::String, 0);
                        }
                        _ => {}
                    }
                }
            }
        }
        SyntaxKind::ATTRIBUTE => {
            // Attribute: key=value - highlight key as property, = as operator, value as string
            for child in node.children_with_tokens() {
                if let Some(token) = child.as_token() {
                    match token.kind() {
                        SyntaxKind::GT => {
                            add_token_from_syntax(tokens, content, token, TokenType::Operator, 0);
                        }
                        kind if is_scalar_token(kind) => {
                            // First scalar is key, subsequent are values
                            // We need to track position to distinguish
                            add_token_from_syntax(tokens, content, token, TokenType::Property, 0);
                        }
                        _ => {}
                    }
                } else if let Some(child_node) = child.as_node() {
                    // Recurse into child nodes
                    walk_node(child_node, content, tokens, ctx);
                }
            }
        }
        _ => {
            // Recurse into children, but also check for tokens at this level
            for child in node.children_with_tokens() {
                if let Some(token) = child.as_token() {
                    // Handle comments at any level
                    match token.kind() {
                        SyntaxKind::LINE_COMMENT => {
                            add_token_from_syntax(tokens, content, token, TokenType::Comment, 0);
                        }
                        SyntaxKind::DOC_COMMENT => {
                            let modifiers = 1 << TokenModifier::Documentation as u32;
                            add_token_from_syntax(
                                tokens,
                                content,
                                token,
                                TokenType::Comment,
                                modifiers,
                            );
                        }
                        _ => {}
                    }
                } else if let Some(child_node) = child.as_node() {
                    walk_node(child_node, content, tokens, ctx);
                }
            }
        }
    }
}

/// Collect tokens from a KEY node, highlighting them as properties
/// KEY can contain: SCALAR, TAG, or direct scalar tokens
fn collect_key_tokens(node: &SyntaxNode, content: &str, tokens: &mut Vec<RawToken>) {
    for child in node.children_with_tokens() {
        if let Some(token) = child.as_token() {
            // Direct scalar token under KEY (might happen in some cases)
            if is_scalar_token(token.kind()) {
                add_token_from_syntax(tokens, content, token, TokenType::Property, 0);
            }
        } else if let Some(child_node) = child.as_node() {
            match child_node.kind() {
                SyntaxKind::SCALAR => {
                    // KEY -> SCALAR -> token
                    for scalar_child in child_node.children_with_tokens() {
                        if let Some(token) = scalar_child.as_token()
                            && is_scalar_token(token.kind())
                        {
                            add_token_from_syntax(tokens, content, token, TokenType::Property, 0);
                        }
                    }
                }
                SyntaxKind::TAG => {
                    // KEY -> TAG (tag as key, e.g., `@schema foo.styx`)
                    for tag_child in child_node.children_with_tokens() {
                        if let Some(token) = tag_child.as_token()
                            && token.kind() == SyntaxKind::TAG_TOKEN
                        {
                            add_token_from_syntax(tokens, content, token, TokenType::Property, 0);
                        }
                    }
                }
                _ => {}
            }
        }
    }
}

/// Collect tokens from a KEY node, highlighting them as values (for sequence elements)
/// In sequences, "keys" are actually values (e.g., `(1 2 3)` - the 1, 2, 3 are values)
fn collect_key_tokens_as_values(node: &SyntaxNode, content: &str, tokens: &mut Vec<RawToken>) {
    for child in node.children_with_tokens() {
        if let Some(token) = child.as_token() {
            // Direct scalar token under KEY
            if is_scalar_token(token.kind()) {
                add_token_from_syntax(tokens, content, token, TokenType::String, 0);
            }
        } else if let Some(child_node) = child.as_node() {
            match child_node.kind() {
                SyntaxKind::SCALAR => {
                    // KEY -> SCALAR -> token
                    for scalar_child in child_node.children_with_tokens() {
                        if let Some(token) = scalar_child.as_token()
                            && is_scalar_token(token.kind())
                        {
                            add_token_from_syntax(tokens, content, token, TokenType::String, 0);
                        }
                    }
                }
                SyntaxKind::TAG => {
                    // KEY -> TAG in sequence (e.g., `(@ok @err)` or `@route{...}`)
                    for tag_child in child_node.children_with_tokens() {
                        if let Some(token) = tag_child.as_token()
                            && token.kind() == SyntaxKind::TAG_TOKEN
                        {
                            add_token_from_syntax(tokens, content, token, TokenType::Type, 0);
                        } else if let Some(tag_node) = tag_child.as_node()
                            && tag_node.kind() == SyntaxKind::TAG_PAYLOAD
                        {
                            // Recurse into tag payload (object context for properties)
                            walk_node(tag_node, content, tokens, WalkContext::default());
                        }
                    }
                }
                SyntaxKind::SEQUENCE => {
                    // KEY -> SEQUENCE (nested sequences like `((1 2) (3 4))`)
                    // Recurse into nested sequence with sequence context
                    let seq_ctx = WalkContext { in_sequence: true };
                    walk_node(child_node, content, tokens, seq_ctx);
                }
                SyntaxKind::OBJECT => {
                    // KEY -> OBJECT (objects in sequences like `({a 1} {b 2})`)
                    // Objects should use regular object context (keys are properties)
                    walk_node(child_node, content, tokens, WalkContext::default());
                }
                _ => {}
            }
        }
    }
}

/// Check if a syntax kind is a scalar token
fn is_scalar_token(kind: SyntaxKind) -> bool {
    matches!(
        kind,
        SyntaxKind::BARE_SCALAR | SyntaxKind::QUOTED_SCALAR | SyntaxKind::RAW_SCALAR
    )
}

/// Add a token from a syntax token
fn add_token_from_syntax(
    tokens: &mut Vec<RawToken>,
    content: &str,
    token: &SyntaxToken,
    token_type: TokenType,
    modifiers: u32,
) {
    let start: usize = token.text_range().start().into();
    let end: usize = token.text_range().end().into();
    let (line, start_char) = offset_to_line_col(content, start);
    let length = (end - start) as u32;

    tokens.push(RawToken {
        line,
        start_char,
        length,
        token_type,
        modifiers,
    });
}

/// Convert byte offset to (line, column)
fn offset_to_line_col(content: &str, offset: usize) -> (u32, u32) {
    let mut line = 0u32;
    let mut col = 0u32;

    for (i, ch) in content.char_indices() {
        if i >= offset {
            break;
        }
        if ch == '\n' {
            line += 1;
            col = 0;
        } else {
            col += 1;
        }
    }

    (line, col)
}

/// Encode raw tokens as LSP semantic tokens (delta-encoded)
fn encode_tokens(raw_tokens: &[RawToken]) -> Vec<SemanticToken> {
    let mut result = Vec::with_capacity(raw_tokens.len());
    let mut prev_line = 0u32;
    let mut prev_start = 0u32;

    for token in raw_tokens {
        let delta_line = token.line - prev_line;
        let delta_start = if delta_line == 0 {
            token.start_char - prev_start
        } else {
            token.start_char
        };

        result.push(SemanticToken {
            delta_line,
            delta_start,
            length: token.length,
            token_type: token.token_type as u32,
            token_modifiers_bitset: token.modifiers,
        });

        prev_line = token.line;
        prev_start = token.start_char;
    }

    result
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Decoded token for easier test assertions
    #[derive(Debug, PartialEq, Eq)]
    struct DecodedToken {
        line: u32,
        start: u32,
        length: u32,
        token_type: TokenType,
        modifiers: u32,
    }

    /// Decode delta-encoded tokens back to absolute positions
    fn decode_tokens(tokens: &[SemanticToken]) -> Vec<DecodedToken> {
        let mut result = Vec::with_capacity(tokens.len());
        let mut line = 0u32;
        let mut start = 0u32;

        for token in tokens {
            line += token.delta_line;
            if token.delta_line == 0 {
                start += token.delta_start;
            } else {
                start = token.delta_start;
            }

            let token_type = match token.token_type {
                0 => TokenType::Comment,
                1 => TokenType::String,
                2 => TokenType::Number,
                3 => TokenType::Keyword,
                4 => TokenType::Type,
                5 => TokenType::EnumMember,
                6 => TokenType::Property,
                7 => TokenType::Operator,
                _ => panic!("Unknown token type: {}", token.token_type),
            };

            result.push(DecodedToken {
                line,
                start,
                length: token.length,
                token_type,
                modifiers: token.token_modifiers_bitset,
            });
        }

        result
    }

    /// Parse source and get decoded tokens
    fn get_tokens(source: &str) -> Vec<DecodedToken> {
        let parse = styx_cst::parse(source);
        let tokens = compute_semantic_tokens(&parse);
        decode_tokens(&tokens)
    }

    /// Find tokens by type
    fn filter_by_type(tokens: &[DecodedToken], ty: TokenType) -> Vec<&DecodedToken> {
        tokens.iter().filter(|t| t.token_type == ty).collect()
    }

    // ========== SECTION 1: COMMENTS ==========

    #[test]
    fn line_comment() {
        let tokens = get_tokens("// This is a comment");
        assert_eq!(tokens.len(), 1);
        assert_eq!(tokens[0].token_type, TokenType::Comment);
        assert_eq!(tokens[0].modifiers, 0); // No documentation modifier
        assert_eq!(tokens[0].length, 20);
    }

    #[test]
    fn doc_comment() {
        let tokens = get_tokens("/// This is a doc comment");
        assert_eq!(tokens.len(), 1);
        assert_eq!(tokens[0].token_type, TokenType::Comment);
        // Documentation modifier is bit 0
        assert_eq!(
            tokens[0].modifiers,
            1 << TokenModifier::Documentation as u32
        );
        assert_eq!(tokens[0].length, 25);
    }

    #[test]
    fn inline_comment_after_entry() {
        let tokens = get_tokens("key value // inline comment");
        let comments = filter_by_type(&tokens, TokenType::Comment);
        assert_eq!(comments.len(), 1);
        assert_eq!(comments[0].modifiers, 0);
    }

    #[test]
    fn doc_comment_before_entry() {
        let tokens = get_tokens("/// Documentation\nkey value");
        let comments = filter_by_type(&tokens, TokenType::Comment);
        let properties = filter_by_type(&tokens, TokenType::Property);

        assert_eq!(comments.len(), 1);
        assert_eq!(comments[0].token_type, TokenType::Comment);
        assert_eq!(comments[0].modifiers, 1); // Documentation modifier

        assert_eq!(properties.len(), 1);
        assert_eq!(properties[0].line, 1); // On second line
    }

    // ========== SECTION 2: BARE SCALARS ==========

    #[test]
    fn bare_scalar_key_value() {
        let tokens = get_tokens("simple_key value");
        assert_eq!(tokens.len(), 2);

        // Key is Property
        assert_eq!(tokens[0].token_type, TokenType::Property);
        assert_eq!(tokens[0].length, 10); // "simple_key"

        // Value is String
        assert_eq!(tokens[1].token_type, TokenType::String);
        assert_eq!(tokens[1].length, 5); // "value"
    }

    #[test]
    fn bare_scalar_with_special_chars() {
        // Test bare scalars with dots, colons, slashes
        let tokens = get_tokens("path foo.bar.baz");
        let strings = filter_by_type(&tokens, TokenType::String);
        assert_eq!(strings.len(), 1);
        assert_eq!(strings[0].length, 11); // "foo.bar.baz"
    }

    #[test]
    fn url_as_bare_scalar() {
        let tokens = get_tokens("url https://example.com/path?q=1");
        let strings = filter_by_type(&tokens, TokenType::String);
        assert_eq!(strings.len(), 1);
        assert_eq!(strings[0].length, 28); // The URL (28 chars)
    }

    #[test]
    fn numeric_bare_scalars() {
        let tokens = get_tokens("int 42\nfloat 3.14\nhex 0xff");
        let strings = filter_by_type(&tokens, TokenType::String);
        // All numeric-looking scalars are highlighted as strings (no schema awareness)
        assert_eq!(strings.len(), 3);
    }

    // ========== SECTION 3: QUOTED STRINGS ==========

    #[test]
    fn quoted_string_value() {
        let tokens = get_tokens("key \"hello world\"");
        let strings = filter_by_type(&tokens, TokenType::String);
        assert_eq!(strings.len(), 1);
        assert_eq!(strings[0].length, 13); // Including quotes
    }

    #[test]
    fn quoted_string_with_escapes() {
        let tokens = get_tokens(r#"key "line1\nline2""#);
        let strings = filter_by_type(&tokens, TokenType::String);
        assert_eq!(strings.len(), 1);
    }

    #[test]
    fn quoted_string_as_key() {
        let tokens = get_tokens("\"key with spaces\" value");
        let properties = filter_by_type(&tokens, TokenType::Property);
        assert_eq!(properties.len(), 1);
        assert_eq!(properties[0].length, 17); // "key with spaces"
    }

    #[test]
    fn empty_quoted_string() {
        let tokens = get_tokens("key \"\"");
        let strings = filter_by_type(&tokens, TokenType::String);
        assert_eq!(strings.len(), 1);
        assert_eq!(strings[0].length, 2); // Just ""
    }

    // ========== SECTION 4: RAW STRINGS ==========

    #[test]
    fn raw_string_simple() {
        let tokens = get_tokens(r#"key r"no escapes \n""#);
        let strings = filter_by_type(&tokens, TokenType::String);
        assert_eq!(strings.len(), 1);
    }

    #[test]
    fn raw_string_with_hashes() {
        let tokens = get_tokens(r##"key r#"has "quotes" inside"#"##);
        let strings = filter_by_type(&tokens, TokenType::String);
        assert_eq!(strings.len(), 1);
    }

    #[test]
    fn raw_string_as_key() {
        let tokens = get_tokens(r##"r#"raw key"# value"##);
        let properties = filter_by_type(&tokens, TokenType::Property);
        assert_eq!(properties.len(), 1);
    }

    // ========== SECTION 5: HEREDOCS ==========

    #[test]
    fn simple_heredoc() {
        let tokens = get_tokens("script <<TEXT\nHello world\nTEXT");
        let operators = filter_by_type(&tokens, TokenType::Operator);
        let strings = filter_by_type(&tokens, TokenType::String);

        // HEREDOC_START and HEREDOC_END are operators
        assert_eq!(operators.len(), 2);

        // Content is string
        assert_eq!(strings.len(), 1);
    }

    #[test]
    fn heredoc_with_language_hint() {
        let tokens = get_tokens("code <<SQL,sql\nSELECT * FROM users\nSQL");
        let operators = filter_by_type(&tokens, TokenType::Operator);
        let strings = filter_by_type(&tokens, TokenType::String);

        assert_eq!(operators.len(), 2); // Start and end markers
        assert_eq!(strings.len(), 1); // Content
    }

    #[test]
    fn heredoc_in_object() {
        let tokens = get_tokens("config {\n    script <<BASH\n    echo hello\n    BASH\n}");
        let operators = filter_by_type(&tokens, TokenType::Operator);
        let strings = filter_by_type(&tokens, TokenType::String);
        let properties = filter_by_type(&tokens, TokenType::Property);

        // "config" and "script" are properties
        assert_eq!(properties.len(), 2);

        // HEREDOC_START and HEREDOC_END
        assert!(operators.len() >= 2);

        // Heredoc content
        assert!(!strings.is_empty());
    }

    // ========== SECTION 6: TAGS ==========

    #[test]
    fn simple_tag() {
        let tokens = get_tokens(r#"status @ok"#);
        let types = filter_by_type(&tokens, TokenType::Type);

        // "@ok" is type (full tag token)
        assert_eq!(types.len(), 1);
        assert_eq!(types[0].length, 3); // "@ok"
    }

    #[test]
    fn tag_with_object_payload() {
        let tokens = get_tokens(r#"result @error{code 500, message "fail"}"#);
        let types = filter_by_type(&tokens, TokenType::Type);
        let properties = filter_by_type(&tokens, TokenType::Property);
        let strings = filter_by_type(&tokens, TokenType::String);

        // "@error" is type
        assert_eq!(types.len(), 1);
        assert_eq!(types[0].length, 6); // "@error"

        // "result", "code" and "message" are properties
        assert_eq!(properties.len(), 3);

        // "fail" and "500" are strings
        assert_eq!(strings.len(), 2);
    }

    #[test]
    fn tag_with_sequence_payload() {
        let tokens = get_tokens(r#"point @rgb(255 128 0)"#);
        let types = filter_by_type(&tokens, TokenType::Type);
        let strings = filter_by_type(&tokens, TokenType::String);

        assert_eq!(types.len(), 1); // "@rgb"
        assert_eq!(types[0].length, 4); // "@rgb"
        assert_eq!(strings.len(), 3); // 255, 128, 0
    }

    #[test]
    fn tag_with_string_payload() {
        let tokens = get_tokens(r#"env @env"HOME""#);
        let types = filter_by_type(&tokens, TokenType::Type);
        let strings = filter_by_type(&tokens, TokenType::String);

        assert_eq!(types.len(), 1); // "@env"
        assert_eq!(types[0].length, 4); // "@env"
        assert_eq!(strings.len(), 1); // "HOME"
    }

    #[test]
    fn nested_tags() {
        let tokens = get_tokens(r#"data @ok{value @some(42)}"#);
        let types = filter_by_type(&tokens, TokenType::Type);

        // Both "@ok" and "@some" are types
        assert_eq!(types.len(), 2);
    }

    #[test]
    fn tag_as_key() {
        let tokens = get_tokens(r#"@string "hello""#);
        let properties = filter_by_type(&tokens, TokenType::Property);
        let strings = filter_by_type(&tokens, TokenType::String);

        // "@string" as key is a property
        assert_eq!(properties.len(), 1);
        assert_eq!(properties[0].length, 7); // "@string"
        assert_eq!(strings.len(), 1); // "hello"
    }

    #[test]
    fn consecutive_tags_in_sequence() {
        let tokens = get_tokens(r#"tags (@ok @err @none)"#);
        let types = filter_by_type(&tokens, TokenType::Type);

        // "@ok", "@err", "@none" are types
        assert_eq!(types.len(), 3);
    }

    // ========== SECTION 7: UNIT VALUES ==========

    #[test]
    fn explicit_unit() {
        let tokens = get_tokens(r#"unit @"#);
        let types = filter_by_type(&tokens, TokenType::Type);
        let properties = filter_by_type(&tokens, TokenType::Property);

        assert_eq!(properties.len(), 1); // "unit"
        assert_eq!(types.len(), 1); // "@" (unit tag)
    }

    #[test]
    fn unit_in_object() {
        let tokens = get_tokens("flags {\n    verbose\n    debug\n}");
        let properties = filter_by_type(&tokens, TokenType::Property);

        // "flags", "verbose", and "debug" are all properties
        assert_eq!(properties.len(), 3);
    }

    // ========== SECTION 8: ATTRIBUTES ==========

    #[test]
    fn single_attribute() {
        let tokens = get_tokens("entry host>localhost");
        let operators = filter_by_type(&tokens, TokenType::Operator);
        let properties = filter_by_type(&tokens, TokenType::Property);

        // "entry" and "host" (attribute key) are properties
        // Note: attribute implementation may vary
        assert!(!properties.is_empty());

        // > is operator
        assert!(operators.iter().any(|t| t.length == 1));
    }

    #[test]
    fn multiple_attributes() {
        let tokens = get_tokens("server host>localhost port>8080");
        let operators = filter_by_type(&tokens, TokenType::Operator);

        // Two > operators
        assert_eq!(operators.len(), 2);
    }

    // ========== SECTION 9: OBJECTS ==========

    #[test]
    fn empty_object() {
        let tokens = get_tokens("empty {}");
        let properties = filter_by_type(&tokens, TokenType::Property);
        assert_eq!(properties.len(), 1); // Just "empty"
    }

    #[test]
    fn nested_object() {
        let tokens = get_tokens("outer {\n    inner {\n        deep value\n    }\n}");
        let properties = filter_by_type(&tokens, TokenType::Property);

        // "outer", "inner", and "deep" are properties
        assert_eq!(properties.len(), 3);
    }

    #[test]
    fn inline_object() {
        let tokens = get_tokens("config {host localhost, port 8080}");
        let properties = filter_by_type(&tokens, TokenType::Property);

        // "config", "host", "port" are properties
        assert_eq!(properties.len(), 3);
    }

    // ========== SECTION 10: SEQUENCES ==========

    #[test]
    fn simple_sequence() {
        let tokens = get_tokens("numbers (1 2 3)");
        let properties = filter_by_type(&tokens, TokenType::Property);
        let strings = filter_by_type(&tokens, TokenType::String);

        assert_eq!(properties.len(), 1); // "numbers"
        assert_eq!(strings.len(), 3); // 1, 2, 3
    }

    #[test]
    fn nested_sequences() {
        let tokens = get_tokens("matrix ((1 2) (3 4))");
        let strings = filter_by_type(&tokens, TokenType::String);
        assert_eq!(strings.len(), 4); // 1, 2, 3, 4
    }

    #[test]
    fn sequence_with_objects() {
        let tokens = get_tokens("items ({a 1} {b 2})");
        let properties = filter_by_type(&tokens, TokenType::Property);
        let strings = filter_by_type(&tokens, TokenType::String);

        // "items", "a", "b" are properties
        assert_eq!(properties.len(), 3);
        // 1, 2 are strings
        assert_eq!(strings.len(), 2);
    }

    // ========== SECTION 11: UNICODE ==========

    #[test]
    fn unicode_key() {
        let tokens = get_tokens("日本語 \"Japanese\"");
        let properties = filter_by_type(&tokens, TokenType::Property);
        assert_eq!(properties.len(), 1);
        // Length should be in characters, not bytes
    }

    #[test]
    fn unicode_value() {
        let tokens = get_tokens("greeting \"Hello, 世界!\"");
        let strings = filter_by_type(&tokens, TokenType::String);
        assert_eq!(strings.len(), 1);
    }

    #[test]
    fn emoji_value() {
        let tokens = get_tokens("emoji \"🎉🚀💻\"");
        let strings = filter_by_type(&tokens, TokenType::String);
        assert_eq!(strings.len(), 1);
    }

    // ========== SECTION 12: DOTTED PATHS ==========

    #[test]
    fn dotted_path_key() {
        let tokens = get_tokens("server.host localhost");
        let properties = filter_by_type(&tokens, TokenType::Property);
        assert_eq!(properties.len(), 1);
        assert_eq!(properties[0].length, 11); // "server.host"
    }

    #[test]
    fn deep_dotted_path() {
        let tokens = get_tokens("a.b.c.d value");
        let properties = filter_by_type(&tokens, TokenType::Property);
        assert_eq!(properties.len(), 1);
        assert_eq!(properties[0].length, 7); // "a.b.c.d"
    }

    // ========== SECTION 13: COMPLEX COMBINATIONS ==========

    #[test]
    fn tag_with_heredoc_in_payload() {
        let tokens = get_tokens(
            r#"template @html{
    content <<HTML
<div>Hello</div>
HTML
}"#,
        );
        let types = filter_by_type(&tokens, TokenType::Type);
        let operators = filter_by_type(&tokens, TokenType::Operator);

        assert_eq!(types.len(), 1); // "@html"
        assert!(operators.len() >= 2); // <<HTML, HTML
    }

    #[test]
    fn mixed_content_object() {
        let tokens = get_tokens(
            r#"kitchen_sink {
    bare identifier
    quoted "string"
    number 42
    tag @some(value)
    seq (a b c)
}"#,
        );
        let properties = filter_by_type(&tokens, TokenType::Property);
        let strings = filter_by_type(&tokens, TokenType::String);
        let types = filter_by_type(&tokens, TokenType::Type);

        // Keys: kitchen_sink, bare, quoted, number, tag, seq
        assert_eq!(properties.len(), 6);

        // Values: identifier, "string", 42, value, a, b, c
        assert_eq!(strings.len(), 7);

        // Tag: some
        assert_eq!(types.len(), 1);
    }

    #[test]
    fn api_routes_example() {
        let tokens = get_tokens(
            r#"routes (
    @route{method GET, path /}
    @route{method POST, path /api}
)"#,
        );
        let types = filter_by_type(&tokens, TokenType::Type);
        let properties = filter_by_type(&tokens, TokenType::Property);

        // Two @route tags
        assert_eq!(types.len(), 2);

        // "routes" + 2x("method", "path") = 5 properties
        assert_eq!(properties.len(), 5);
    }

    #[test]
    fn result_type_pattern() {
        let tokens = get_tokens(
            r#"result @ok{
    data {
        users (
            {id 1, name Alice}
            {id 2, name Bob}
        )
    }
}"#,
        );
        let types = filter_by_type(&tokens, TokenType::Type);
        let properties = filter_by_type(&tokens, TokenType::Property);

        // @ok
        assert_eq!(types.len(), 1);

        // result, data, users, id, name, id, name
        assert_eq!(properties.len(), 7);
    }

    // ========== SECTION 14: POSITION ACCURACY ==========

    #[test]
    fn multiline_positions() {
        let tokens = get_tokens("line0 value0\nline1 value1\nline2 value2");
        let properties = filter_by_type(&tokens, TokenType::Property);

        assert_eq!(properties[0].line, 0);
        assert_eq!(properties[0].start, 0);

        assert_eq!(properties[1].line, 1);
        assert_eq!(properties[1].start, 0);

        assert_eq!(properties[2].line, 2);
        assert_eq!(properties[2].start, 0);
    }

    #[test]
    fn indented_content_positions() {
        let tokens = get_tokens("outer {\n    inner value\n}");
        let properties = filter_by_type(&tokens, TokenType::Property);

        // "outer" at line 0, col 0
        assert_eq!(properties[0].line, 0);
        assert_eq!(properties[0].start, 0);

        // "inner" at line 1, col 4 (after 4 spaces)
        assert_eq!(properties[1].line, 1);
        assert_eq!(properties[1].start, 4);
    }

    // ========== SECTION 15: EDGE CASES ==========

    #[test]
    fn empty_document() {
        let tokens = get_tokens("");
        assert!(tokens.is_empty());
    }

    #[test]
    fn whitespace_only() {
        let tokens = get_tokens("   \n   \n   ");
        assert!(tokens.is_empty());
    }

    #[test]
    fn comment_only() {
        let tokens = get_tokens("// Just a comment");
        assert_eq!(tokens.len(), 1);
        assert_eq!(tokens[0].token_type, TokenType::Comment);
    }

    #[test]
    fn deeply_nested_structure() {
        let tokens = get_tokens("a {b {c {d {e value}}}}");
        let properties = filter_by_type(&tokens, TokenType::Property);
        assert_eq!(properties.len(), 5); // a, b, c, d, e
    }

    #[test]
    fn booleans_as_strings() {
        // Without schema awareness, booleans are just strings
        let tokens = get_tokens("flag true\nother false");
        let strings = filter_by_type(&tokens, TokenType::String);
        assert_eq!(strings.len(), 2);
    }
}
