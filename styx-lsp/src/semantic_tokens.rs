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

/// A semantic token before encoding
#[derive(Debug)]
struct RawToken {
    line: u32,
    start_char: u32,
    length: u32,
    token_type: TokenType,
    modifiers: u32,
}

/// Compute semantic tokens for a parsed document
pub fn compute_semantic_tokens(parse: &Parse) -> Vec<SemanticToken> {
    let content = parse.syntax().to_string();
    let mut raw_tokens = Vec::new();

    // Walk the CST and emit tokens
    walk_node(&parse.syntax(), &content, &mut raw_tokens);

    // Sort tokens by position
    raw_tokens.sort_by(|a, b| a.line.cmp(&b.line).then(a.start_char.cmp(&b.start_char)));

    // Encode as delta positions
    encode_tokens(&raw_tokens)
}

/// Recursively walk a syntax node and collect semantic tokens
fn walk_node(node: &SyntaxNode, content: &str, tokens: &mut Vec<RawToken>) {
    match node.kind() {
        SyntaxKind::KEY => {
            // Key nodes contain scalars - highlight the scalar tokens as properties
            for child in node.children_with_tokens() {
                if let Some(token) = child.as_token() {
                    if is_scalar_token(token.kind()) {
                        add_token_from_syntax(tokens, content, token, TokenType::Property, 0);
                    }
                }
            }
        }
        SyntaxKind::TAG => {
            // Tag nodes - highlight @ as operator and tag name as type
            for child in node.children_with_tokens() {
                if let Some(token) = child.as_token() {
                    if token.kind() == SyntaxKind::AT {
                        add_token_from_syntax(tokens, content, token, TokenType::Operator, 0);
                    }
                } else if let Some(child_node) = child.as_node() {
                    if child_node.kind() == SyntaxKind::TAG_NAME {
                        // Get the token inside TAG_NAME
                        for t in child_node.children_with_tokens() {
                            if let Some(token) = t.as_token() {
                                if is_scalar_token(token.kind()) {
                                    add_token_from_syntax(
                                        tokens,
                                        content,
                                        token,
                                        TokenType::Type,
                                        0,
                                    );
                                }
                            }
                        }
                    } else if child_node.kind() == SyntaxKind::TAG_PAYLOAD {
                        // Recurse into payload
                        walk_node(child_node, content, tokens);
                    }
                }
            }
        }
        SyntaxKind::UNIT => {
            // Unit @ token
            for child in node.children_with_tokens() {
                if let Some(token) = child.as_token() {
                    if token.kind() == SyntaxKind::AT {
                        add_token_from_syntax(tokens, content, token, TokenType::Operator, 0);
                    }
                }
            }
        }
        SyntaxKind::SCALAR => {
            // Scalar values (not in KEY position) get string highlighting
            for child in node.children_with_tokens() {
                if let Some(token) = child.as_token() {
                    if is_scalar_token(token.kind()) {
                        add_token_from_syntax(tokens, content, token, TokenType::String, 0);
                    }
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
                        SyntaxKind::EQ => {
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
                    walk_node(child_node, content, tokens);
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
                    walk_node(child_node, content, tokens);
                }
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
