use facet_lsp::position::LineIndex;
use facet_lsp::types::Position;
use facet_testhelpers::test;

#[test]
fn utf16_positions_pin_multibyte_lines() {
    let text = "aé𝄞b\nz";
    let index = LineIndex::new(text);

    assert_eq!(
        index.offset_to_position(0),
        Some(Position {
            line: 0,
            character: 0
        })
    );
    assert_eq!(
        index.offset_to_position(1),
        Some(Position {
            line: 0,
            character: 1
        })
    );
    assert_eq!(
        index.offset_to_position(3),
        Some(Position {
            line: 0,
            character: 2
        })
    );
    assert_eq!(
        index.offset_to_position(7),
        Some(Position {
            line: 0,
            character: 4
        })
    );
    assert_eq!(
        index.offset_to_position(8),
        Some(Position {
            line: 0,
            character: 5
        })
    );
    assert_eq!(
        index.offset_to_position(10),
        Some(Position {
            line: 1,
            character: 1
        })
    );

    assert_eq!(
        index.position_to_offset(Position {
            line: 0,
            character: 4
        }),
        Some(7)
    );
    assert_eq!(
        index.position_to_offset(Position {
            line: 0,
            character: 3
        }),
        None,
        "inside the surrogate pair must not round to a byte offset"
    );
}
