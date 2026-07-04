//! LSP semantic-token delta encoding.

/// Absolute semantic token before LSP 5-int delta encoding.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct AbsoluteSemanticToken {
    /// Zero-based line.
    pub line: u32,
    /// Zero-based UTF-16 start character.
    pub start_character: u32,
    /// UTF-16 token length.
    pub length: u32,
    /// Token type legend index.
    pub token_type: u32,
    /// Token modifier bitset.
    pub token_modifiers: u32,
}

/// Encode absolute sorted tokens as LSP semantic token data.
pub fn encode_semantic_tokens(tokens: &[AbsoluteSemanticToken]) -> Vec<u32> {
    let mut sorted = tokens.to_vec();
    sorted.sort_by_key(|token| (token.line, token.start_character));

    let mut data = Vec::with_capacity(sorted.len() * 5);
    let mut prev_line = 0;
    let mut prev_start = 0;
    for (idx, token) in sorted.iter().enumerate() {
        let delta_line = if idx == 0 {
            token.line
        } else {
            token.line - prev_line
        };
        let delta_start = if idx == 0 || delta_line != 0 {
            token.start_character
        } else {
            token.start_character - prev_start
        };
        data.extend_from_slice(&[
            delta_line,
            delta_start,
            token.length,
            token.token_type,
            token.token_modifiers,
        ]);
        prev_line = token.line;
        prev_start = token.start_character;
    }
    data
}
