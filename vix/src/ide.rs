//! JSON facade over parse+bind for editor integrations — the playground's wasm
//! bridge today, the LSP server's core tomorrow. Same shapes either way.

use std::cell::RefCell;

use facet::Facet;

use crate::binder::{self, SymbolKind};
use crate::{VixParser, ast};

#[derive(Facet, Debug)]
struct IdeBindings {
    /// Parse failure, if any — bindings are absent/empty then.
    error: Option<String>,
    symbols: Vec<IdeSymbol>,
    refs: Vec<IdeRef>,
    unresolved: Vec<IdeUnresolved>,
}

#[derive(Facet, Debug)]
struct IdeSymbol {
    name: String,
    kind: String,
    /// Byte span of the defining name occurrence.
    start: u32,
    end: u32,
}

#[derive(Facet, Debug)]
struct IdeRef {
    start: u32,
    end: u32,
    /// Index into `symbols`.
    symbol: usize,
}

#[derive(Facet, Debug)]
struct IdeUnresolved {
    name: String,
    start: u32,
    end: u32,
}

fn kind_str(kind: SymbolKind) -> &'static str {
    match kind {
        SymbolKind::Fn => "fn",
        SymbolKind::Param => "param",
        SymbolKind::Let => "let",
        SymbolKind::ClosureParam => "closure param",
        SymbolKind::Import => "import",
    }
}

thread_local! {
    /// Table construction is the expensive part; do it once per thread and reuse.
    static PARSER: RefCell<Option<VixParser>> = const { RefCell::new(None) };
}

fn with_parser<R>(f: impl FnOnce(&VixParser) -> R) -> R {
    PARSER.with(|slot| {
        let mut slot = slot.borrow_mut();
        f(slot.get_or_insert_with(VixParser::new))
    })
}

/// Parse + bind `source`, returning the bindings as JSON.
pub fn bindings_json(source: &str) -> String {
    let out = match with_parser(|p| p.parse(source)) {
        Ok(file) => bindings_of(&file),
        Err(e) => IdeBindings {
            error: Some(e.message),
            symbols: Vec::new(),
            refs: Vec::new(),
            unresolved: Vec::new(),
        },
    };
    facet_json::to_string(&out).expect("IdeBindings serializes (plain structs, no maps)")
}

fn bindings_of(file: &ast::SourceFile) -> IdeBindings {
    let b = binder::bind(file);
    IdeBindings {
        error: None,
        symbols: b
            .symbols()
            .map(|(_, s)| IdeSymbol {
                name: s.name.clone(),
                kind: kind_str(s.kind).to_string(),
                start: s.def.start,
                end: s.def.end,
            })
            .collect(),
        refs: b
            .refs()
            .map(|(span, id)| IdeRef {
                start: span.start,
                end: span.end,
                symbol: id.0,
            })
            .collect(),
        unresolved: b
            .unresolved()
            .iter()
            .map(|s| IdeUnresolved {
                name: s.value.clone(),
                start: s.span.start,
                end: s.span.end,
            })
            .collect(),
    }
}
