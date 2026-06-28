use arborium_theme::builtin;
use margin::{
    Annotation, AnnotationRole, Diagnostics, Note, NoteKind, Report, Severity, Source, SourceId,
    Span, SyntaxClass,
};

use super::{
    ColorLevel, GlyphMode, HyperlinkMode, TerminalCapabilities, Theme, render,
    render_plan_with_theme,
};

fn sample_diagnostics() -> Diagnostics {
    Diagnostics {
        sources: vec![Source {
            id: SourceId("main".to_string()),
            name: "src/main.vx".to_string(),
            hyperlink: Some("file:///workspace/src/main.vx".to_string()),
            text: "let number = 1\nnumber + missing\n".to_string(),
        }],
        reports: vec![Report {
            severity: Severity::Error,
            title: "unknown name `missing`".to_string(),
            annotations: vec![
                Annotation {
                    spans: vec![Span::new("main", 24, 31)],
                    role: AnnotationRole::PrimaryLabel,
                    syntax_class: None,
                    message: Some("not found in this scope".to_string()),
                    priority: 100,
                },
                Annotation {
                    spans: vec![Span::new("main", 0, 3)],
                    role: AnnotationRole::SyntaxToken,
                    syntax_class: Some(SyntaxClass::Keyword),
                    message: None,
                    priority: 1,
                },
                Annotation {
                    spans: vec![Span::new("main", 13, 14)],
                    role: AnnotationRole::SyntaxToken,
                    syntax_class: Some(SyntaxClass::Number),
                    message: None,
                    priority: 1,
                },
            ],
            notes: vec![Note {
                kind: NoteKind::Help,
                text: "consider defining the binding before using it".to_string(),
            }],
            sections: Vec::new(),
        }],
    }
}

/// d[verify api.renderer-options]
/// d[verify term.explicit-capabilities]
/// d[verify term.plaintext-mode]
/// d[verify test.fixture-first]
/// d[verify layout.no-terminal-wrap]
/// d[verify layout.notes-wrap]
#[test]
fn render_supports_plaintext_ascii_output() {
    let rendered = render(
        &sample_diagnostics(),
        TerminalCapabilities {
            width: 48,
            glyph_mode: GlyphMode::Ascii,
            color_level: ColorLevel::None,
            hyperlink_mode: HyperlinkMode::None,
            tab_width: 4,
        },
    )
    .unwrap();

    println!("{rendered}");
    assert!(rendered.contains("error: unknown name `missing`"));
    assert!(rendered.contains("--> src/main.vx"));
    assert!(rendered.contains("|"));
    assert!(
        !rendered.contains("1 | let number = 1\n  | ^^^"),
        "{rendered}"
    );
    assert!(!rendered.contains("\u{1b}["));
}

/// d[verify term.explicit-capabilities]
/// d[verify api.renderer-options]
#[test]
fn render_can_hyperlink_source_headers() {
    let rendered = render(
        &sample_diagnostics(),
        TerminalCapabilities {
            width: 48,
            glyph_mode: GlyphMode::Unicode,
            color_level: ColorLevel::None,
            hyperlink_mode: HyperlinkMode::Osc8,
            tab_width: 4,
        },
    )
    .unwrap();

    println!("{rendered}");
    assert!(
        rendered.contains("\u{1b}]8;;file:///workspace/src/main.vx\u{1b}\\"),
        "{rendered}"
    );
    assert!(rendered.contains("src/main.vx"), "{rendered}");
}

/// d[verify label.multiline-message-alignment]
#[test]
fn render_below_span_message_stays_anchored_to_the_span() {
    let rendered = render(
        &sample_diagnostics(),
        TerminalCapabilities {
            width: 28,
            glyph_mode: GlyphMode::Ascii,
            color_level: ColorLevel::None,
            hyperlink_mode: HyperlinkMode::None,
            tab_width: 4,
        },
    )
    .unwrap();

    println!("{rendered}");
    assert!(rendered.contains("  |          ^^^^^^^"), "{rendered}");
    assert!(
        rendered.contains("  |          \\- not found"),
        "{rendered}"
    );
}

#[test]
fn render_below_span_connector_uses_the_label_style() {
    let rendered = render(
        &sample_diagnostics(),
        TerminalCapabilities {
            width: 28,
            glyph_mode: GlyphMode::Unicode,
            color_level: ColorLevel::Ansi16,
            hyperlink_mode: HyperlinkMode::None,
            tab_width: 4,
        },
    )
    .unwrap();

    println!("{rendered}");
    assert!(rendered.contains("\u{1b}[31m"), "{rendered}");
    assert!(rendered.contains("┬──────"), "{rendered}");
    assert!(rendered.contains("\u{1b}[31m╰─\u{1b}[0m"), "{rendered}");
    assert!(!rendered.contains("\u{1b}[90m╰─\u{1b}[0m"), "{rendered}");
}

#[test]
fn render_multiline_annotations_only_emit_the_message_once() {
    let diagnostics = Diagnostics {
        sources: vec![Source {
            id: SourceId("main".to_string()),
            name: "src/main.vx".to_string(),
            hyperlink: None,
            text: "alpha\nbeta\ngamma\n".to_string(),
        }],
        reports: vec![Report {
            severity: Severity::Error,
            title: "spans lines".to_string(),
            annotations: vec![Annotation {
                spans: vec![Span::new("main", 2, 8)],
                role: AnnotationRole::PrimaryLabel,
                syntax_class: None,
                message: Some("spans lines".to_string()),
                priority: 100,
            }],
            notes: Vec::new(),
            sections: Vec::new(),
        }],
    };

    let rendered = render(
        &diagnostics,
        TerminalCapabilities {
            width: 32,
            glyph_mode: GlyphMode::Ascii,
            color_level: ColorLevel::None,
            hyperlink_mode: HyperlinkMode::None,
            tab_width: 4,
        },
    )
    .unwrap();

    println!("{rendered}");
    assert_eq!(rendered.matches("spans lines").count(), 2, "{rendered}");
}

/// d[verify test.width-matrix]
/// d[verify test.capability-matrix]
/// d[verify glyph.unicode-ascii]
/// d[verify theme.capability-fallback]
/// d[verify label.multiline-message-alignment]
/// d[verify layout.multiline-labels]
#[test]
fn render_changes_with_width_and_capabilities() {
    let narrow = render(
        &sample_diagnostics(),
        TerminalCapabilities {
            width: 28,
            glyph_mode: GlyphMode::Unicode,
            color_level: ColorLevel::Ansi16,
            hyperlink_mode: HyperlinkMode::None,
            tab_width: 4,
        },
    )
    .unwrap();
    let wide = render(
        &sample_diagnostics(),
        TerminalCapabilities {
            width: 72,
            glyph_mode: GlyphMode::Unicode,
            color_level: ColorLevel::Ansi16,
            hyperlink_mode: HyperlinkMode::None,
            tab_width: 4,
        },
    )
    .unwrap();

    println!("-- narrow --\n{narrow}");
    println!("-- wide --\n{wide}");
    assert!(narrow.contains("\u{1b}[31merror\u{1b}[0m"));
    assert!(narrow.contains("\u{1b}[35mlet\u{1b}[0m"), "{narrow}");
    assert!(narrow.contains("\u{1b}[36m1\u{1b}[0m"), "{narrow}");
    assert!(narrow.contains("src/main.vx"));
    assert!(narrow.contains("╰─"));
    assert!(!narrow.contains("consider defining the binding before using it"));
    assert!(wide.contains("consider defining the binding before using it"));
}

/// d[verify theme.capability-fallback]
/// d[verify test.capability-matrix]
#[test]
fn render_can_use_rgb24_colors_from_arborium_theme() {
    let plan = super::layout(
        &sample_diagnostics(),
        TerminalCapabilities {
            width: 48,
            glyph_mode: GlyphMode::Unicode,
            color_level: ColorLevel::Rgb24,
            hyperlink_mode: HyperlinkMode::None,
            tab_width: 4,
        },
    )
    .unwrap();
    let rendered = render_plan_with_theme(
        &plan,
        TerminalCapabilities {
            width: 48,
            glyph_mode: GlyphMode::Unicode,
            color_level: ColorLevel::Rgb24,
            hyperlink_mode: HyperlinkMode::None,
            tab_width: 4,
        },
        Theme::from_arborium(&builtin::catppuccin_mocha()),
    );

    println!("{rendered}");
    assert!(rendered.contains("\u{1b}[38;2;"), "{rendered}");
}

/// d[verify layout.ellipsis]
#[test]
fn render_clips_long_lines_with_explicit_ellipsis() {
    let diagnostics = Diagnostics {
        sources: vec![Source {
            id: SourceId("main".to_string()),
            name: "src/main.vx".to_string(),
            hyperlink: None,
            text: "012345678901234567890123456789\n".to_string(),
        }],
        reports: vec![Report {
            severity: Severity::Error,
            title: "too long".to_string(),
            annotations: vec![Annotation {
                spans: vec![Span::new("main", 20, 24)],
                role: AnnotationRole::PrimaryLabel,
                syntax_class: None,
                message: Some("past the clip".to_string()),
                priority: 100,
            }],
            notes: Vec::new(),
            sections: Vec::new(),
        }],
    };

    let rendered = render(
        &diagnostics,
        TerminalCapabilities {
            width: 20,
            glyph_mode: GlyphMode::Unicode,
            color_level: ColorLevel::None,
            hyperlink_mode: HyperlinkMode::None,
            tab_width: 4,
        },
    )
    .unwrap();

    println!("{rendered}");
    assert!(rendered.contains("012345678901234..."));
}

#[test]
fn render_wraps_long_path_like_tokens() {
    let diagnostics = Diagnostics {
        sources: vec![Source {
            id: SourceId("main".to_string()),
            name: "src/main.vx".to_string(),
            hyperlink: None,
            text: "command\n".to_string(),
        }],
        reports: vec![Report {
            severity: Severity::Error,
            title: "missing".to_string(),
            annotations: vec![Annotation {
                spans: vec![Span::new("main", 0, 7)],
                role: AnnotationRole::PrimaryLabel,
                syntax_class: None,
                message: Some("src/very/long/path/component/file.c".to_string()),
                priority: 100,
            }],
            notes: Vec::new(),
            sections: Vec::new(),
        }],
    };

    let rendered = render(
        &diagnostics,
        TerminalCapabilities {
            width: 28,
            glyph_mode: GlyphMode::Unicode,
            color_level: ColorLevel::None,
            hyperlink_mode: HyperlinkMode::None,
            tab_width: 4,
        },
    )
    .unwrap();

    println!("{rendered}");
    assert!(
        !rendered.contains("src/very/long/path/component/file.c"),
        "{rendered}"
    );
    assert!(rendered.contains("src/very/long/path/"), "{rendered}");
    assert!(rendered.contains("component/file.c"), "{rendered}");
}

/// d[verify term.ansi-discipline]
/// d[verify theme.roles]
#[test]
fn render_uses_balanced_ansi_sequences_and_role_styles() {
    let plan = super::layout(
        &sample_diagnostics(),
        TerminalCapabilities {
            width: 40,
            glyph_mode: GlyphMode::Unicode,
            color_level: ColorLevel::Ansi16,
            hyperlink_mode: HyperlinkMode::None,
            tab_width: 4,
        },
    )
    .unwrap();
    let rendered = render_plan_with_theme(
        &plan,
        TerminalCapabilities {
            width: 40,
            glyph_mode: GlyphMode::Unicode,
            color_level: ColorLevel::Ansi16,
            hyperlink_mode: HyperlinkMode::None,
            tab_width: 4,
        },
        Theme::default(),
    );

    println!("{rendered}");
    assert!(rendered.matches("\u{1b}[0m").count() >= 3, "{rendered}");
    assert!(rendered.contains("\u{1b}[90m│\u{1b}[0m"), "{rendered}");
    assert!(
        rendered.contains("\u{1b}[32m  = help: \u{1b}[0m"),
        "{rendered}"
    );
}

/// d[verify unicode.tab-policy]
/// d[verify unicode.display-width]
/// d[verify unicode.grapheme-safety]
/// d[verify unicode.normalization-stability]
/// d[verify test.unicode]
#[test]
fn render_expands_tabs_and_keeps_unicode_text_stable() {
    let diagnostics = Diagnostics {
        sources: vec![Source {
            id: SourceId("main".to_string()),
            name: "src/main.vx".to_string(),
            hyperlink: None,
            text: "a\tCafe\u{301}\n".to_string(),
        }],
        reports: vec![Report {
            severity: Severity::Error,
            title: "unicode".to_string(),
            annotations: vec![Annotation {
                spans: vec![Span::new("main", 2, "a\tCafe\u{301}".len())],
                role: AnnotationRole::PrimaryLabel,
                syntax_class: None,
                message: Some("accented".to_string()),
                priority: 100,
            }],
            notes: vec![Note {
                kind: NoteKind::Note,
                text: "tab\taligned".to_string(),
            }],
            sections: Vec::new(),
        }],
    };

    let rendered = render(
        &diagnostics,
        TerminalCapabilities {
            width: 32,
            glyph_mode: GlyphMode::Unicode,
            color_level: ColorLevel::None,
            hyperlink_mode: HyperlinkMode::None,
            tab_width: 4,
        },
    )
    .unwrap();

    println!("{rendered}");
    assert!(rendered.contains("a   Cafe\u{301}"), "{rendered}");
    assert!(rendered.contains("accented"), "{rendered}");
    assert!(rendered.contains("Cafe\u{301}"), "{rendered}");
}

#[test]
fn render_multiline_annotations_emit_the_message_once() {
    let diagnostics = Diagnostics {
        sources: vec![Source {
            id: SourceId("main".to_string()),
            name: "src/main.vx".to_string(),
            hyperlink: None,
            text: "fn compile() {\n  line_one();\n  line_two();\n}\n".to_string(),
        }],
        reports: vec![Report {
            severity: Severity::Error,
            title: "grouped".to_string(),
            annotations: vec![Annotation {
                spans: vec![Span::new("main", 0, 43)],
                role: AnnotationRole::PrimaryLabel,
                syntax_class: None,
                message: Some("shared message".to_string()),
                priority: 100,
            }],
            notes: Vec::new(),
            sections: Vec::new(),
        }],
    };

    let rendered = render(
        &diagnostics,
        TerminalCapabilities {
            width: 52,
            glyph_mode: GlyphMode::Unicode,
            color_level: ColorLevel::None,
            hyperlink_mode: HyperlinkMode::None,
            tab_width: 4,
        },
    )
    .unwrap();

    println!("{rendered}");
    assert_eq!(rendered.matches("shared message").count(), 1, "{rendered}");
    assert!(rendered.contains("▌ fn compile() {"), "{rendered}");
    assert!(rendered.contains("▌   line_one();"), "{rendered}");
    assert!(rendered.contains("▌   line_two();"), "{rendered}");
    assert!(rendered.contains("  ▌ ╰─ shared message"), "{rendered}");
    assert!(!rendered.contains("────"), "{rendered}");
}

#[test]
fn render_wraps_long_unbroken_tokens() {
    let diagnostics = Diagnostics {
        sources: vec![Source {
            id: SourceId("main".to_string()),
            name: "src/main.vx".to_string(),
            hyperlink: None,
            text: "value\n".to_string(),
        }],
        reports: vec![Report {
            severity: Severity::Error,
            title: "path".to_string(),
            annotations: vec![Annotation {
                spans: vec![Span::new("main", 0, 5)],
                role: AnnotationRole::PrimaryLabel,
                syntax_class: None,
                message: Some("/very/long/path/without/spaces/or-breaks".to_string()),
                priority: 100,
            }],
            notes: Vec::new(),
            sections: Vec::new(),
        }],
    };

    let rendered = render(
        &diagnostics,
        TerminalCapabilities {
            width: 24,
            glyph_mode: GlyphMode::Ascii,
            color_level: ColorLevel::None,
            hyperlink_mode: HyperlinkMode::None,
            tab_width: 4,
        },
    )
    .unwrap();

    println!("{rendered}");
    assert!(
        rendered.contains("/very/long/path/")
            && rendered.contains("without/spaces/")
            && rendered.contains("or-breaks"),
        "{rendered}"
    );
    assert!(!rendered.contains("  |    \n"), "{rendered}");
}
