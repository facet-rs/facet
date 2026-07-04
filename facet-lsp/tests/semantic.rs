use facet_lsp::semantic::{AbsoluteSemanticToken, encode_semantic_tokens};
use facet_testhelpers::test;

#[test]
fn semantic_tokens_use_lsp_delta_runs() {
    let tokens = [
        AbsoluteSemanticToken {
            line: 2,
            start_character: 4,
            length: 3,
            token_type: 1,
            token_modifiers: 0,
        },
        AbsoluteSemanticToken {
            line: 0,
            start_character: 1,
            length: 2,
            token_type: 0,
            token_modifiers: 1,
        },
        AbsoluteSemanticToken {
            line: 2,
            start_character: 10,
            length: 1,
            token_type: 2,
            token_modifiers: 4,
        },
    ];

    assert_eq!(
        encode_semantic_tokens(&tokens),
        vec![0, 1, 2, 0, 1, 2, 4, 3, 1, 0, 0, 6, 1, 2, 4]
    );
}
