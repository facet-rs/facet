use super::{
    Annotation, AnnotationRole, Diagnostics, LayoutOptions, Note, NoteKind, Report, ReportSection,
    Severity, Source, SourceId, Span, plan,
};

fn sample_diagnostics() -> Diagnostics {
    Diagnostics {
        sources: vec![
            Source {
                id: SourceId("main".to_string()),
                name: "src/main.vx".to_string(),
                hyperlink: None,
                text: "let number = 1\nnumber + missing\n".to_string(),
            },
            Source {
                id: SourceId("lib".to_string()),
                name: "src/lib.vx".to_string(),
                hyperlink: None,
                text: "fn helper() {}\n".to_string(),
            },
        ],
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
                    spans: vec![Span::new("main", 3, 9)],
                    role: AnnotationRole::SecondaryLabel,
                    syntax_class: None,
                    message: Some("bound here".to_string()),
                    priority: 50,
                },
                Annotation {
                    spans: vec![Span::new("lib", 0, 13)],
                    role: AnnotationRole::RelatedLabel,
                    syntax_class: None,
                    message: Some("related declaration".to_string()),
                    priority: 20,
                },
            ],
            notes: vec![Note {
                kind: NoteKind::Help,
                text: "try importing a binding".to_string(),
            }],
            sections: vec![ReportSection {
                title: "while checking `main`".to_string(),
                notes: vec![Note {
                    kind: NoteKind::Note,
                    text: "secondary context is preserved".to_string(),
                }],
            }],
        }],
    }
}

// d[verify api.explicit-layout-artifact]
// d[verify api.stable-fixtures]
// d[verify api.core-model]
// d[verify api.layout-render-separation]
// d[verify model.report]
// d[verify model.source]
// d[verify model.annotation]
// d[verify model.window]
// d[verify input.multiple-sources]
// d[verify package.name]
// d[verify package.no-vixen-prefix]
// d[verify package.primary-split]
// d[verify package.additional-renderers]
// d[verify layout.pipeline]
// d[verify layout.pure-planning]
// d[verify layout.width-aware]
// d[verify window.primary-context]
// d[verify window.cross-file]
#[test]
fn plan_preserves_structured_reports_and_explicit_windows() {
    let plan = plan(&sample_diagnostics(), &LayoutOptions::with_width(72)).unwrap();

    assert_eq!(plan.width, 72);
    assert_eq!(plan.reports.len(), 1);
    assert_eq!(plan.reports[0].windows.len(), 2);
    assert!(
        plan.reports[0]
            .windows
            .iter()
            .any(|window| window.source_name == "src/main.vx")
    );
    assert!(
        plan.reports[0]
            .windows
            .iter()
            .any(|window| window.source_name == "src/lib.vx")
    );
    assert!(
        plan.reports[0]
            .windows
            .iter()
            .all(|window| window.geometry.source_columns > 0)
    );
    assert_eq!(plan.reports[0].sections[0].title, "while checking `main`");
}

// d[verify input.span-bounds]
// d[verify span.zero-width]
// d[verify span.line-breaks]
// d[verify layout.cell-grid]
#[test]
fn plan_keeps_zero_width_and_multiline_segments_intact() {
    let diagnostics = Diagnostics {
        sources: vec![Source {
            id: SourceId("main".to_string()),
            name: "src/main.vx".to_string(),
            hyperlink: None,
            text: "alpha\nbeta\ngamma\n".to_string(),
        }],
        reports: vec![Report {
            severity: Severity::Error,
            title: "bad insertion".to_string(),
            annotations: vec![
                Annotation {
                    spans: vec![Span::new("main", 0, 0)],
                    role: AnnotationRole::PrimaryLabel,
                    syntax_class: None,
                    message: Some("insert here".to_string()),
                    priority: 100,
                },
                Annotation {
                    spans: vec![Span::new("main", 2, 8)],
                    role: AnnotationRole::SecondaryLabel,
                    syntax_class: None,
                    message: Some("spans lines".to_string()),
                    priority: 50,
                },
            ],
            notes: Vec::new(),
            sections: Vec::new(),
        }],
    };

    let plan = plan(&diagnostics, &LayoutOptions::default()).unwrap();
    let annotations = &plan.reports[0].windows[0].annotations;

    assert!(annotations.iter().any(|annotation| annotation.segments
        == vec![super::ResolvedSpan {
            line_number: 1,
            start_column: 0,
            end_column: 1,
        }]));
    let multiline = annotations
        .iter()
        .find(|annotation| annotation.message.as_deref() == Some("spans lines"))
        .unwrap();
    assert_eq!(multiline.segments.len(), 2);
    assert_eq!(multiline.segments[0].line_number, 1);
    assert_eq!(multiline.segments[1].line_number, 2);
}

// d[verify window.context-policy]
// d[verify window.merge-nearby]
// d[verify window.stable-selection]
// d[verify layout.gutter-geometry]
// d[verify layout.long-lines]
// d[verify layout.message-placement]
// d[verify layout.placement-modes]
// d[verify layout.crowding-policy]
#[test]
fn plan_merges_nearby_windows_and_assigns_explicit_geometry_and_placement() {
    let diagnostics = Diagnostics {
        sources: vec![Source {
            id: SourceId("main".to_string()),
            name: "src/main.vx".to_string(),
            hyperlink: None,
            text: "012345678901234567890123456789\n".to_string(),
        }],
        reports: vec![Report {
            severity: Severity::Error,
            title: "crowded".to_string(),
            annotations: vec![
                Annotation {
                    spans: vec![Span::new("main", 0, 5)],
                    role: AnnotationRole::PrimaryLabel,
                    syntax_class: None,
                    message: Some("first".to_string()),
                    priority: 100,
                },
                Annotation {
                    spans: vec![Span::new("main", 8, 12)],
                    role: AnnotationRole::SecondaryLabel,
                    syntax_class: None,
                    message: Some("second message is wider".to_string()),
                    priority: 90,
                },
                Annotation {
                    spans: vec![Span::new("main", 14, 18)],
                    role: AnnotationRole::RelatedLabel,
                    syntax_class: None,
                    message: Some("third".to_string()),
                    priority: 80,
                },
            ],
            notes: Vec::new(),
            sections: Vec::new(),
        }],
    };

    let plan = plan(
        &diagnostics,
        &LayoutOptions {
            width: 24,
            primary_context_lines: 0,
            secondary_context_lines: 0,
            merge_distance_lines: 1,
            tab_width: 4,
            long_line_mode: super::LongLineMode::Clip,
        },
    )
    .unwrap();

    let window = &plan.reports[0].windows[0];
    assert_eq!(plan.reports[0].windows.len(), 1);
    assert_eq!(window.first_line_number, 1);
    assert_eq!(window.last_line_number, 1);
    assert!(window.lines[0].clipped);
    assert_eq!(window.geometry.line_number_width, 1);
    assert!(
        window
            .annotations
            .iter()
            .any(|annotation| annotation.placement == super::PlacementMode::Side)
    );
    assert!(
        window
            .annotations
            .iter()
            .any(|annotation| annotation.placement == super::PlacementMode::BelowSpan)
    );
    assert!(
        window
            .annotations
            .iter()
            .any(|annotation| annotation.placement == super::PlacementMode::Stacked)
    );
}

#[test]
fn plan_uses_display_width_for_side_placement_budget() {
    let diagnostics = Diagnostics {
        sources: vec![Source {
            id: SourceId("main".to_string()),
            name: "src/main.vx".to_string(),
            hyperlink: None,
            text: "0123456789\n".to_string(),
        }],
        reports: vec![Report {
            severity: Severity::Error,
            title: "wide".to_string(),
            annotations: vec![Annotation {
                spans: vec![Span::new("main", 2, 4)],
                role: AnnotationRole::PrimaryLabel,
                syntax_class: None,
                message: Some("界界界".to_string()),
                priority: 100,
            }],
            notes: Vec::new(),
            sections: Vec::new(),
        }],
    };

    let plan = plan(
        &diagnostics,
        &LayoutOptions {
            width: 12,
            primary_context_lines: 0,
            secondary_context_lines: 0,
            merge_distance_lines: 0,
            tab_width: 4,
            long_line_mode: super::LongLineMode::Clip,
        },
    )
    .unwrap();

    assert_eq!(
        plan.reports[0].windows[0].annotations[0].placement,
        super::PlacementMode::BelowSpan
    );
}

// d[verify model.role-stable]
// d[verify input.structured]
// d[verify input.unified-spans]
// d[verify input.source-identity]
// d[verify input.overlay-kinds]
// d[verify span.authoritative-source]
// d[verify span.unicode-correct]
// d[verify label.clustered]
// d[verify label.multispan]
// d[verify label.priority]
// d[verify label.priority-lattice]
// d[verify label.semantic-over-syntax]
// d[verify label.single-message-bearing]
// d[verify syntax.non-owning]
// d[verify syntax.overlay-model]
// d[verify syntax.overlay-priority]
// d[verify syntax.window-bounded]
// d[verify coalesce.deterministic]
// d[verify coalesce.identical]
// d[verify coalesce.message-bearing]
// d[verify coalesce.nearby]
// d[verify coalesce.report-sections]
// d[impl test.coalescing-fixtures]
// d[verify test.coalescing-fixtures]
#[test]
fn plan_coalesces_annotations_and_respects_priority_lattice() {
    let diagnostics = Diagnostics {
        sources: vec![Source {
            id: SourceId("main".to_string()),
            name: "src/main.vx".to_string(),
            hyperlink: None,
            text: "Cafe\u{301} syntax zone tail\n".to_string(),
        }],
        reports: vec![Report {
            severity: Severity::Error,
            title: "overlap".to_string(),
            annotations: vec![
                Annotation {
                    spans: vec![Span::new("main", 0, 6), Span::new("main", 7, 13)],
                    role: AnnotationRole::PrimaryLabel,
                    syntax_class: None,
                    message: Some("primary".to_string()),
                    priority: 100,
                },
                Annotation {
                    spans: vec![Span::new("main", 0, 6)],
                    role: AnnotationRole::PrimaryLabel,
                    syntax_class: None,
                    message: Some("primary".to_string()),
                    priority: 100,
                },
                Annotation {
                    spans: vec![Span::new("main", 0, 6)],
                    role: AnnotationRole::SyntaxToken,
                    syntax_class: None,
                    message: None,
                    priority: 1,
                },
                Annotation {
                    spans: vec![Span::new("main", 14, 18)],
                    role: AnnotationRole::SecondaryLabel,
                    syntax_class: None,
                    message: Some("cluster".to_string()),
                    priority: 50,
                },
                Annotation {
                    spans: vec![Span::new("main", 19, 21)],
                    role: AnnotationRole::SecondaryLabel,
                    syntax_class: None,
                    message: None,
                    priority: 50,
                },
            ],
            notes: Vec::new(),
            sections: Vec::new(),
        }],
    };

    let first = plan(&diagnostics, &LayoutOptions::with_width(48)).unwrap();
    let second = plan(&diagnostics, &LayoutOptions::with_width(48)).unwrap();
    let window = &first.reports[0].windows[0];

    assert_eq!(first, second);
    assert_eq!(window.annotations.len(), 2);
    assert_eq!(window.annotations[0].segments.len(), 2);
    assert_eq!(window.annotations[1].message.as_deref(), Some("cluster"));
    assert_eq!(window.annotations[1].segments.len(), 2);
    assert!(
        window
            .annotations
            .iter()
            .all(|annotation| annotation.role != AnnotationRole::SyntaxToken)
    );
}

#[test]
fn plan_uses_display_width_for_message_placement() {
    let diagnostics = Diagnostics {
        sources: vec![Source {
            id: SourceId("main".to_string()),
            name: "src/main.vx".to_string(),
            hyperlink: None,
            text: "emoji\n".to_string(),
        }],
        reports: vec![Report {
            severity: Severity::Error,
            title: "wide".to_string(),
            annotations: vec![Annotation {
                spans: vec![Span::new("main", 0, 5)],
                role: AnnotationRole::PrimaryLabel,
                syntax_class: None,
                message: Some("界界界界".to_string()),
                priority: 100,
            }],
            notes: Vec::new(),
            sections: Vec::new(),
        }],
    };

    let plan = plan(
        &diagnostics,
        &LayoutOptions {
            width: 13,
            primary_context_lines: 0,
            secondary_context_lines: 0,
            merge_distance_lines: 0,
            tab_width: 4,
            long_line_mode: super::LongLineMode::Clip,
        },
    )
    .unwrap();

    assert_eq!(
        plan.reports[0].windows[0].annotations[0].placement,
        super::PlacementMode::BelowSpan
    );
}
