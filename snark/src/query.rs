//! Imported Tree-sitter query files.

use std::collections::BTreeSet;

use facet::Facet;

use crate::parser::{ParserGrammar, ParserSymbol, RuntimeParseReport, TreeEvent, TreeNodeId};
use crate::runtime_input::{ByteRange, PointRange};
use crate::source::SourceFile;

/// Raw Tree-sitter query source.
#[derive(Debug, Clone, Facet, PartialEq, Eq)]
pub struct QuerySource(pub String);

impl QuerySource {
    /// Quoted anonymous node literals referenced by this query source.
    ///
    /// This is an oracle/import fact, not a query evaluator. Predicate forms
    /// such as `(#match? @capture "regex")` are skipped structurally.
    pub fn anonymous_node_literals(&self) -> BTreeSet<String> {
        anonymous_node_literals(&self.0)
    }

    /// Capture names declared by this query source.
    ///
    /// This scanner ignores comments, strings, and predicate bodies. It is an
    /// oracle/import fact, not a query evaluator.
    pub fn capture_names(&self) -> BTreeSet<String> {
        capture_names(&self.0)
    }

    /// Named node kinds referenced by this query source.
    ///
    /// Predicate forms, captures, anonymous string nodes, field labels,
    /// wildcard/anchor operators, and quantifiers are not reported.
    pub fn named_node_references(&self) -> BTreeSet<String> {
        named_node_references(&self.0)
    }

    /// Execute the supported highlight-query subset against a runtime parse.
    ///
    /// This is the first oracle-driven evaluator slice: named node captures,
    /// anonymous literal captures, direct parent/child captures, and captured-text
    /// `#match?`/`#eq?`/`#any-of?` predicates. Unsupported query constructs are ignored
    /// rather than approximated.
    pub fn execute_runtime_highlights(
        &self,
        parser: &ParserGrammar,
        report: &RuntimeParseReport,
        input: &str,
    ) -> Vec<HighlightCapture> {
        let tree_events = report.accepted_tree_events();
        self.execute_runtime_highlights_from_tree_events(parser, &tree_events, input)
    }

    /// Execute the supported highlight-query subset against runtime tree events.
    ///
    /// This is shared by the direct Snark runtime and Weavy-carried runtime
    /// reports so query execution is checked against the same structured tree
    /// event surface.
    pub fn execute_runtime_highlights_from_tree_events(
        &self,
        parser: &ParserGrammar,
        tree_events: &[TreeEvent],
        input: &str,
    ) -> Vec<HighlightCapture> {
        execute_runtime_highlights(&self.0, parser, tree_events, input)
    }

    /// Extract supported language-injection regions from accepted runtime tree events.
    ///
    /// This is the first layering-runtime input slice: `@injection.content`,
    /// `@injection.language`, `#set! injection.language`, `#set! injection.combined`,
    /// `#set! injection.include-children`, and captured-text
    /// `#match?`/`#eq?`/`#any-of?` predicates. Unsupported predicates are ignored
    /// rather than approximated.
    pub fn execute_runtime_injections(
        &self,
        parser: &ParserGrammar,
        report: &RuntimeParseReport,
        input: &str,
    ) -> Vec<InjectionRegion> {
        let tree_events = report.accepted_tree_events();
        self.execute_runtime_injections_from_tree_events(parser, &tree_events, input)
    }

    /// Extract supported language-injection regions from runtime tree events.
    pub fn execute_runtime_injections_from_tree_events(
        &self,
        parser: &ParserGrammar,
        tree_events: &[TreeEvent],
        input: &str,
    ) -> Vec<InjectionRegion> {
        execute_runtime_injections(&self.0, parser, tree_events, input)
    }
}

/// One query capture produced by the supported runtime highlight evaluator.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HighlightCapture {
    capture_name: String,
    bytes: ByteRange,
    points: PointRange,
    text: String,
}

impl HighlightCapture {
    /// Capture name without the leading `@`.
    pub fn capture_name(&self) -> &str {
        &self.capture_name
    }

    /// Captured byte range.
    pub const fn bytes(&self) -> ByteRange {
        self.bytes
    }

    /// Captured point range.
    pub const fn points(&self) -> PointRange {
        self.points
    }

    /// Captured source text.
    pub fn text(&self) -> &str {
        &self.text
    }
}

/// One injection content region selected by the supported injection-query evaluator.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InjectionRegion {
    language: String,
    combined: bool,
    include_children: bool,
    bytes: ByteRange,
    points: PointRange,
    text: String,
}

impl InjectionRegion {
    /// Injected language name.
    pub fn language(&self) -> &str {
        &self.language
    }

    /// Whether sibling regions of this language should be parsed as one virtual input.
    pub const fn combined(&self) -> bool {
        self.combined
    }

    /// Whether child nodes should remain visible inside the injected range.
    pub const fn include_children(&self) -> bool {
        self.include_children
    }

    /// Captured byte range in the host input.
    pub const fn bytes(&self) -> ByteRange {
        self.bytes
    }

    /// Captured point range in the host input.
    pub const fn points(&self) -> PointRange {
        self.points
    }

    /// Captured host source text.
    pub fn text(&self) -> &str {
        &self.text
    }
}

/// Quoted anonymous node literals referenced by a Tree-sitter query source.
pub fn anonymous_node_literals(query: &str) -> BTreeSet<String> {
    let mut scanner = QueryScanner::new(query);
    let mut contexts = vec![QueryContext::Root];
    let mut literals = BTreeSet::new();
    while let Some(token) = scanner.next_token() {
        match token {
            QueryToken::OpenParen => contexts.push(QueryContext::Form {
                seen_head: false,
                predicate: false,
            }),
            QueryToken::CloseParen => {
                if contexts.len() > 1 {
                    contexts.pop();
                }
            }
            QueryToken::OpenBracket => contexts.push(QueryContext::List),
            QueryToken::CloseBracket => {
                if contexts.len() > 1 {
                    contexts.pop();
                }
            }
            QueryToken::Symbol(symbol) => {
                if let Some(QueryContext::Form {
                    seen_head,
                    predicate,
                }) = contexts.last_mut()
                {
                    if !*seen_head {
                        *predicate = symbol.starts_with('#');
                        *seen_head = true;
                    }
                }
            }
            QueryToken::String(literal) => {
                if let Some(QueryContext::Form { seen_head, .. }) = contexts.last_mut() {
                    *seen_head = true;
                }
                if !contexts.iter().any(QueryContext::is_predicate) {
                    literals.insert(literal);
                }
            }
        }
    }
    literals
}

/// Capture names declared by a Tree-sitter query source.
pub fn capture_names(query: &str) -> BTreeSet<String> {
    let mut scanner = QueryScanner::new(query);
    let mut contexts = vec![QueryContext::Root];
    let mut captures = BTreeSet::new();
    while let Some(token) = scanner.next_token() {
        match token {
            QueryToken::OpenParen => contexts.push(QueryContext::Form {
                seen_head: false,
                predicate: false,
            }),
            QueryToken::CloseParen => {
                if contexts.len() > 1 {
                    contexts.pop();
                }
            }
            QueryToken::OpenBracket => contexts.push(QueryContext::List),
            QueryToken::CloseBracket => {
                if contexts.len() > 1 {
                    contexts.pop();
                }
            }
            QueryToken::String(_) => {
                if let Some(QueryContext::Form { seen_head, .. }) = contexts.last_mut() {
                    *seen_head = true;
                }
            }
            QueryToken::Symbol(symbol) => {
                if let Some(QueryContext::Form {
                    seen_head,
                    predicate,
                }) = contexts.last_mut()
                {
                    if !*seen_head {
                        *predicate = symbol.starts_with('#');
                        *seen_head = true;
                    }
                }
                if !contexts.iter().any(QueryContext::is_predicate)
                    && let Some(capture) = symbol.strip_prefix('@')
                    && !capture.is_empty()
                    && capture.chars().all(is_capture_name_char)
                {
                    captures.insert(capture.to_owned());
                }
            }
        }
    }
    captures
}

/// Named node kinds referenced by a Tree-sitter query source.
pub fn named_node_references(query: &str) -> BTreeSet<String> {
    let mut scanner = QueryScanner::new(query);
    let mut contexts = vec![QueryContext::Root];
    let mut nodes = BTreeSet::new();
    while let Some(token) = scanner.next_token() {
        match token {
            QueryToken::OpenParen => contexts.push(QueryContext::Form {
                seen_head: false,
                predicate: false,
            }),
            QueryToken::CloseParen => {
                if contexts.len() > 1 {
                    contexts.pop();
                }
            }
            QueryToken::OpenBracket => contexts.push(QueryContext::List),
            QueryToken::CloseBracket => {
                if contexts.len() > 1 {
                    contexts.pop();
                }
            }
            QueryToken::String(_) => {
                if let Some(QueryContext::Form { seen_head, .. }) = contexts.last_mut() {
                    *seen_head = true;
                }
            }
            QueryToken::Symbol(symbol) => {
                if let Some(QueryContext::Form {
                    seen_head,
                    predicate,
                }) = contexts.last_mut()
                {
                    if !*seen_head {
                        *predicate = symbol.starts_with('#');
                        *seen_head = true;
                    }
                }
                if !contexts.iter().any(QueryContext::is_predicate)
                    && is_named_node_reference(&symbol)
                {
                    nodes.insert(symbol);
                }
            }
        }
    }
    nodes
}

fn execute_runtime_highlights(
    query: &str,
    parser: &ParserGrammar,
    tree_events: &[TreeEvent],
    input: &str,
) -> Vec<HighlightCapture> {
    let rules = highlight_rules(query);
    let nodes = runtime_highlight_nodes(parser, tree_events, input);
    let tokens = runtime_highlight_tokens(parser, tree_events, input);
    let fields = runtime_highlight_fields(parser, tree_events);
    let mut captures = Vec::new();

    for rule in &rules {
        match &rule.target {
            HighlightTarget::Node(kind) => {
                for node in nodes.iter().filter(|node| &node.kind == kind) {
                    if !node_satisfies_edge_constraints(node, rule, &nodes, &fields) {
                        continue;
                    }
                    if rule
                        .predicates
                        .iter()
                        .all(|predicate| predicate.matches(&node.text))
                    {
                        captures.push(HighlightCapture {
                            capture_name: rule.capture_name.clone(),
                            bytes: node.bytes,
                            points: node.points,
                            text: node.text.clone(),
                        });
                    }
                }
            }
            HighlightTarget::AnyNode => {
                for node in &nodes {
                    if !node_satisfies_edge_constraints(node, rule, &nodes, &fields) {
                        continue;
                    }
                    if rule
                        .predicates
                        .iter()
                        .all(|predicate| predicate.matches(&node.text))
                    {
                        captures.push(HighlightCapture {
                            capture_name: rule.capture_name.clone(),
                            bytes: node.bytes,
                            points: node.points,
                            text: node.text.clone(),
                        });
                    }
                }
            }
            HighlightTarget::Literal(literal) => {
                for token in tokens.iter().filter(|token| &token.text == literal) {
                    if rule.field_name.is_some() {
                        continue;
                    }
                    if !rule
                        .parent_kind
                        .as_ref()
                        .is_none_or(|parent| token_has_direct_parent_kind(token, parent, &nodes))
                    {
                        continue;
                    }
                    if rule
                        .predicates
                        .iter()
                        .all(|predicate| predicate.matches(&token.text))
                    {
                        captures.push(HighlightCapture {
                            capture_name: rule.capture_name.clone(),
                            bytes: token.bytes,
                            points: token.points,
                            text: token.text.clone(),
                        });
                    }
                }
            }
        }
    }

    captures
}

fn execute_runtime_injections(
    query: &str,
    parser: &ParserGrammar,
    tree_events: &[TreeEvent],
    input: &str,
) -> Vec<InjectionRegion> {
    let patterns = injection_patterns(query);
    let nodes = runtime_highlight_nodes(parser, tree_events, input);
    let tokens = runtime_highlight_tokens(parser, tree_events, input);
    let fields = runtime_highlight_fields(parser, tree_events);
    let mut regions = Vec::new();

    for pattern in &patterns {
        let language_captures = pattern
            .captures
            .iter()
            .filter(|capture| capture.capture_name == "injection.language")
            .flat_map(|capture| capture_binding_matches(capture, &nodes, &tokens, &fields))
            .collect::<Vec<_>>();
        for content in pattern
            .captures
            .iter()
            .filter(|capture| capture.capture_name == "injection.content")
        {
            for capture in capture_binding_matches(content, &nodes, &tokens, &fields) {
                if !pattern.predicates.iter().all(|predicate| {
                    predicate.matches_nearest_capture(
                        &capture,
                        &nodes,
                        &tokens,
                        &fields,
                        &pattern.captures,
                    )
                }) {
                    continue;
                }
                let Some(language) = pattern
                    .language
                    .clone()
                    .or_else(|| dynamic_injection_language(&capture, &language_captures, &nodes))
                else {
                    continue;
                };
                regions.push(InjectionRegion {
                    language,
                    combined: pattern.combined,
                    include_children: pattern.include_children,
                    bytes: capture.bytes,
                    points: capture.points,
                    text: capture.text,
                });
            }
        }
    }

    regions
}

fn dynamic_injection_language(
    content: &HighlightCapture,
    languages: &[HighlightCapture],
    nodes: &[RuntimeHighlightNode],
) -> Option<String> {
    languages
        .iter()
        .min_by_key(|language| injection_language_score(content.bytes, language.bytes, nodes))
        .map(|language| language.text.clone())
}

fn injection_language_score(
    content: ByteRange,
    language: ByteRange,
    nodes: &[RuntimeHighlightNode],
) -> (u8, u32, u32) {
    let same_parent = direct_parent_range(content, nodes)
        .is_some_and(|parent| byte_range_contains(parent.bytes, language));
    let preceding = language.end() <= content.start();
    let category =
        if byte_range_contains(language, content) || byte_range_contains(content, language) {
            0
        } else if same_parent && preceding {
            1
        } else if same_parent {
            2
        } else if preceding {
            3
        } else {
            4
        };
    (
        category,
        byte_range_gap(content, language),
        language.start().get(),
    )
}

fn byte_range_gap(left: ByteRange, right: ByteRange) -> u32 {
    if right.end() <= left.start() {
        left.start().get().saturating_sub(right.end().get())
    } else if left.end() <= right.start() {
        right.start().get().saturating_sub(left.end().get())
    } else {
        0
    }
}

fn capture_binding_matches(
    binding: &QueryCaptureBinding,
    nodes: &[RuntimeHighlightNode],
    tokens: &[RuntimeHighlightToken],
    fields: &[RuntimeHighlightField],
) -> Vec<HighlightCapture> {
    let rule = HighlightRule {
        capture_name: binding.capture_name.clone(),
        target: binding.target.clone(),
        parent_kind: binding.parent_kind.clone(),
        field_name: binding.field_name.clone(),
        predicates: Vec::new(),
    };
    match &binding.target {
        HighlightTarget::Node(kind) => nodes
            .iter()
            .filter(|node| &node.kind == kind)
            .filter(|node| node_satisfies_edge_constraints(node, &rule, nodes, fields))
            .map(|node| HighlightCapture {
                capture_name: binding.capture_name.clone(),
                bytes: node.bytes,
                points: node.points,
                text: node.text.clone(),
            })
            .collect(),
        HighlightTarget::AnyNode => nodes
            .iter()
            .filter(|node| node_satisfies_edge_constraints(node, &rule, nodes, fields))
            .map(|node| HighlightCapture {
                capture_name: binding.capture_name.clone(),
                bytes: node.bytes,
                points: node.points,
                text: node.text.clone(),
            })
            .collect(),
        HighlightTarget::Literal(literal) => tokens
            .iter()
            .filter(|token| &token.text == literal)
            .filter(|_| binding.field_name.is_none())
            .filter(|token| {
                binding
                    .parent_kind
                    .as_ref()
                    .is_none_or(|parent| token_has_direct_parent_kind(token, parent, nodes))
            })
            .map(|token| HighlightCapture {
                capture_name: binding.capture_name.clone(),
                bytes: token.bytes,
                points: token.points,
                text: token.text.clone(),
            })
            .collect(),
    }
}

#[derive(Debug, Clone)]
struct HighlightRule {
    capture_name: String,
    target: HighlightTarget,
    parent_kind: Option<String>,
    field_name: Option<String>,
    predicates: Vec<HighlightPredicate>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct QueryCaptureBinding {
    capture_name: String,
    target: HighlightTarget,
    parent_kind: Option<String>,
    field_name: Option<String>,
}

#[derive(Debug, Clone, Default)]
struct InjectionPattern {
    captures: Vec<QueryCaptureBinding>,
    predicates: Vec<HighlightPredicateBinding>,
    language: Option<String>,
    combined: bool,
    include_children: bool,
}

#[derive(Debug, Clone, Default)]
struct ParsedInjectionItem {
    target: Option<HighlightTarget>,
    capture_targets: Vec<HighlightTarget>,
    captures: Vec<QueryCaptureBinding>,
    predicates: Vec<HighlightPredicateBinding>,
    language: Option<String>,
    combined: bool,
    include_children: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum HighlightTarget {
    AnyNode,
    Node(String),
    Literal(String),
}

#[derive(Debug, Clone)]
enum HighlightPredicate {
    Regex(regex::Regex),
    Exact(String),
    AnyOf(Vec<String>),
}

impl HighlightPredicate {
    fn from_match_pattern(pattern: String) -> Option<Self> {
        regex::Regex::new(&pattern).ok().map(Self::Regex)
    }

    fn matches(&self, text: &str) -> bool {
        match self {
            Self::Regex(regex) => regex.is_match(text),
            Self::Exact(expected) => text == expected,
            Self::AnyOf(expected) => expected.iter().any(|candidate| text == candidate),
        }
    }
}

#[derive(Debug, Clone)]
struct HighlightPredicateBinding {
    capture_name: String,
    predicate: HighlightPredicate,
}

impl HighlightPredicateBinding {
    fn matches_nearest_capture(
        &self,
        content: &HighlightCapture,
        nodes: &[RuntimeHighlightNode],
        tokens: &[RuntimeHighlightToken],
        fields: &[RuntimeHighlightField],
        bindings: &[QueryCaptureBinding],
    ) -> bool {
        bindings
            .iter()
            .filter(|binding| binding.capture_name == self.capture_name)
            .flat_map(|binding| capture_binding_matches(binding, nodes, tokens, fields))
            .min_by_key(|capture| injection_language_score(content.bytes, capture.bytes, nodes))
            .is_some_and(|capture| self.predicate.matches(&capture.text))
    }
}

fn predicate_bindings_from_tokens(
    op: &str,
    symbols: &[String],
    strings: Vec<String>,
) -> Vec<HighlightPredicateBinding> {
    let Some(capture_name) = symbols
        .iter()
        .find_map(|symbol| symbol.strip_prefix('@'))
        .filter(|capture| !capture.is_empty() && capture.chars().all(is_capture_name_char))
        .map(str::to_owned)
    else {
        return Vec::new();
    };

    let predicate = match op {
        "#match?" => strings
            .into_iter()
            .next()
            .and_then(HighlightPredicate::from_match_pattern),
        "#eq?" => strings.into_iter().next().map(HighlightPredicate::Exact),
        "#any-of?" => {
            if strings.is_empty() {
                None
            } else {
                Some(HighlightPredicate::AnyOf(strings))
            }
        }
        _ => None,
    };

    predicate
        .map(|predicate| HighlightPredicateBinding {
            capture_name,
            predicate,
        })
        .into_iter()
        .collect()
}

#[derive(Debug, Clone)]
struct ParsedHighlightItem {
    target: Option<HighlightTarget>,
    capture_targets: Vec<HighlightTarget>,
    rules: Vec<HighlightRule>,
    predicates: Vec<HighlightPredicateBinding>,
}

fn highlight_rules(query: &str) -> Vec<HighlightRule> {
    let mut scanner = QueryScanner::new(query);
    let mut tokens = Vec::new();
    while let Some(token) = scanner.next_token() {
        tokens.push(token);
    }
    let mut parser = HighlightQueryParser { tokens, index: 0 };
    parser.parse_all()
}

fn injection_patterns(query: &str) -> Vec<InjectionPattern> {
    let mut scanner = QueryScanner::new(query);
    let mut tokens = Vec::new();
    while let Some(token) = scanner.next_token() {
        tokens.push(token);
    }
    let mut parser = InjectionQueryParser { tokens, index: 0 };
    parser.parse_all()
}

struct HighlightQueryParser {
    tokens: Vec<QueryToken>,
    index: usize,
}

impl HighlightQueryParser {
    fn parse_all(&mut self) -> Vec<HighlightRule> {
        let mut rules = Vec::new();
        while self.index < self.tokens.len() {
            let item = self.parse_item(None, None);
            rules.extend(item.rules);
        }
        rules
    }

    fn parse_item(
        &mut self,
        parent_kind: Option<&str>,
        field_name: Option<&str>,
    ) -> ParsedHighlightItem {
        let mut item = match self.next() {
            Some(QueryToken::OpenParen) => self.parse_form(parent_kind, field_name),
            Some(QueryToken::OpenBracket) => self.parse_list(parent_kind, field_name),
            Some(QueryToken::String(literal)) => ParsedHighlightItem {
                target: Some(HighlightTarget::Literal(literal.clone())),
                capture_targets: vec![HighlightTarget::Literal(literal)],
                rules: Vec::new(),
                predicates: Vec::new(),
            },
            Some(QueryToken::Symbol(symbol)) if symbol == "_" => ParsedHighlightItem {
                target: Some(HighlightTarget::AnyNode),
                capture_targets: vec![HighlightTarget::AnyNode],
                rules: Vec::new(),
                predicates: Vec::new(),
            },
            Some(QueryToken::Symbol(_))
            | Some(QueryToken::CloseParen)
            | Some(QueryToken::CloseBracket)
            | None => ParsedHighlightItem {
                target: None,
                capture_targets: Vec::new(),
                rules: Vec::new(),
                predicates: Vec::new(),
            },
        };

        let captures = self.consume_captures();
        let capture_targets = if let Some(target) = &item.target {
            vec![target.clone()]
        } else {
            item.capture_targets.clone()
        };
        for target in capture_targets {
            for capture_name in &captures {
                item.rules.push(HighlightRule {
                    capture_name: capture_name.clone(),
                    target: target.clone(),
                    parent_kind: parent_kind.map(str::to_owned),
                    field_name: field_name.map(str::to_owned),
                    predicates: Vec::new(),
                });
            }
        }
        apply_predicates(&mut item.rules, &item.predicates);
        item
    }

    fn parse_form(
        &mut self,
        parent_kind: Option<&str>,
        field_name: Option<&str>,
    ) -> ParsedHighlightItem {
        match self.peek() {
            Some(QueryToken::Symbol(symbol)) if symbol.starts_with('#') => self.parse_predicate(),
            Some(QueryToken::Symbol(symbol)) if symbol == "_" => {
                let _ = self.next();
                while !self.consume_close_paren() {
                    let child_field = self.consume_field_label();
                    let _ = self.parse_item(parent_kind, child_field.as_deref());
                    if self.index >= self.tokens.len() {
                        break;
                    }
                }
                ParsedHighlightItem {
                    target: Some(HighlightTarget::AnyNode),
                    capture_targets: vec![HighlightTarget::AnyNode],
                    rules: Vec::new(),
                    predicates: Vec::new(),
                }
            }
            Some(QueryToken::Symbol(symbol)) if is_named_node_reference(symbol) => {
                let kind = match self.next() {
                    Some(QueryToken::Symbol(kind)) => kind,
                    _ => unreachable!("peeked a symbol before consuming a symbol"),
                };
                let mut item = ParsedHighlightItem {
                    target: Some(HighlightTarget::Node(kind.clone())),
                    capture_targets: vec![HighlightTarget::Node(kind.clone())],
                    rules: Vec::new(),
                    predicates: Vec::new(),
                };
                while !self.consume_close_paren() {
                    let child_field = self.consume_field_label();
                    let child = self.parse_item(Some(&kind), child_field.as_deref());
                    item.rules.extend(child.rules);
                    item.predicates.extend(child.predicates);
                    if self.index >= self.tokens.len() {
                        break;
                    }
                }
                apply_predicates(&mut item.rules, &item.predicates);
                item
            }
            _ => {
                let mut item = ParsedHighlightItem {
                    target: None,
                    capture_targets: Vec::new(),
                    rules: Vec::new(),
                    predicates: Vec::new(),
                };
                while !self.consume_close_paren() {
                    let child_field = self.consume_field_label();
                    let child = self.parse_item(parent_kind, child_field.as_deref().or(field_name));
                    item.rules.extend(child.rules);
                    item.predicates.extend(child.predicates);
                    if self.index >= self.tokens.len() {
                        break;
                    }
                }
                apply_predicates(&mut item.rules, &item.predicates);
                item
            }
        }
    }

    fn parse_list(
        &mut self,
        parent_kind: Option<&str>,
        field_name: Option<&str>,
    ) -> ParsedHighlightItem {
        let mut item = ParsedHighlightItem {
            target: None,
            capture_targets: Vec::new(),
            rules: Vec::new(),
            predicates: Vec::new(),
        };
        while !self.consume_close_bracket() {
            let child = self.parse_item(parent_kind, field_name);
            if let Some(target) = child.target.clone() {
                item.capture_targets.push(target);
            } else {
                item.capture_targets.extend(child.capture_targets.clone());
            }
            item.rules.extend(child.rules);
            item.predicates.extend(child.predicates);
            if self.index >= self.tokens.len() {
                break;
            }
        }
        item
    }

    fn parse_predicate(&mut self) -> ParsedHighlightItem {
        let op = match self.next() {
            Some(QueryToken::Symbol(op)) => op,
            _ => unreachable!("peeked a predicate before consuming one"),
        };
        let mut symbols = Vec::new();
        let mut strings = Vec::new();
        while !self.consume_close_paren() {
            match self.next() {
                Some(QueryToken::Symbol(symbol)) => symbols.push(symbol),
                Some(QueryToken::String(string)) => strings.push(string),
                Some(QueryToken::OpenParen) => {
                    let _ = self.parse_form(None, None);
                }
                Some(QueryToken::OpenBracket) => {
                    let _ = self.parse_list(None, None);
                }
                Some(QueryToken::CloseParen) | Some(QueryToken::CloseBracket) | None => break,
            }
            if self.index >= self.tokens.len() {
                break;
            }
        }
        let predicates = predicate_bindings_from_tokens(&op, &symbols, strings);
        ParsedHighlightItem {
            target: None,
            capture_targets: Vec::new(),
            rules: Vec::new(),
            predicates,
        }
    }

    fn consume_captures(&mut self) -> Vec<String> {
        let mut captures = Vec::new();
        while let Some(QueryToken::Symbol(symbol)) = self.peek() {
            let Some(capture) = symbol.strip_prefix('@') else {
                break;
            };
            if capture.is_empty() || !capture.chars().all(is_capture_name_char) {
                break;
            }
            let capture = capture.to_owned();
            self.index += 1;
            captures.push(capture);
        }
        captures
    }

    fn consume_field_label(&mut self) -> Option<String> {
        let Some(QueryToken::Symbol(symbol)) = self.peek() else {
            return None;
        };
        let field = symbol.strip_suffix(':')?;
        if field.is_empty()
            || !field
                .chars()
                .all(|ch| ch.is_ascii_alphanumeric() || ch == '_')
        {
            return None;
        }
        let field = field.to_owned();
        self.index += 1;
        Some(field)
    }

    fn consume_close_paren(&mut self) -> bool {
        if matches!(self.peek(), Some(QueryToken::CloseParen)) {
            self.index += 1;
            true
        } else {
            false
        }
    }

    fn consume_close_bracket(&mut self) -> bool {
        if matches!(self.peek(), Some(QueryToken::CloseBracket)) {
            self.index += 1;
            true
        } else {
            false
        }
    }

    fn peek(&self) -> Option<&QueryToken> {
        self.tokens.get(self.index)
    }

    fn next(&mut self) -> Option<QueryToken> {
        let token = self.tokens.get(self.index).cloned();
        if token.is_some() {
            self.index += 1;
        }
        token
    }
}

struct InjectionQueryParser {
    tokens: Vec<QueryToken>,
    index: usize,
}

impl InjectionQueryParser {
    fn parse_all(&mut self) -> Vec<InjectionPattern> {
        let mut patterns = Vec::new();
        while self.index < self.tokens.len() {
            let item = self.parse_item(None, None);
            if item
                .captures
                .iter()
                .any(|capture| capture.capture_name == "injection.content")
            {
                patterns.push(InjectionPattern {
                    captures: item.captures,
                    predicates: item.predicates,
                    language: item.language,
                    combined: item.combined,
                    include_children: item.include_children,
                });
            }
        }
        patterns
    }

    fn parse_item(
        &mut self,
        parent_kind: Option<&str>,
        field_name: Option<&str>,
    ) -> ParsedInjectionItem {
        let mut item = match self.next() {
            Some(QueryToken::OpenParen) => self.parse_form(parent_kind, field_name),
            Some(QueryToken::OpenBracket) => self.parse_list(parent_kind, field_name),
            Some(QueryToken::String(literal)) => ParsedInjectionItem {
                target: Some(HighlightTarget::Literal(literal.clone())),
                capture_targets: vec![HighlightTarget::Literal(literal)],
                ..ParsedInjectionItem::default()
            },
            Some(QueryToken::Symbol(symbol)) if symbol == "_" => ParsedInjectionItem {
                target: Some(HighlightTarget::AnyNode),
                capture_targets: vec![HighlightTarget::AnyNode],
                ..ParsedInjectionItem::default()
            },
            Some(QueryToken::Symbol(_))
            | Some(QueryToken::CloseParen)
            | Some(QueryToken::CloseBracket)
            | None => ParsedInjectionItem::default(),
        };

        let captures = self.consume_captures();
        let capture_targets = if let Some(target) = &item.target {
            vec![target.clone()]
        } else {
            item.capture_targets.clone()
        };
        for target in capture_targets {
            for capture_name in &captures {
                item.captures.push(QueryCaptureBinding {
                    capture_name: capture_name.clone(),
                    target: target.clone(),
                    parent_kind: parent_kind.map(str::to_owned),
                    field_name: field_name.map(str::to_owned),
                });
            }
        }
        item
    }

    fn parse_form(
        &mut self,
        parent_kind: Option<&str>,
        field_name: Option<&str>,
    ) -> ParsedInjectionItem {
        match self.peek() {
            Some(QueryToken::Symbol(symbol)) if symbol.starts_with('#') => self.parse_predicate(),
            Some(QueryToken::Symbol(symbol)) if symbol == "_" => {
                let _ = self.next();
                let mut item = ParsedInjectionItem {
                    target: Some(HighlightTarget::AnyNode),
                    capture_targets: vec![HighlightTarget::AnyNode],
                    ..ParsedInjectionItem::default()
                };
                while !self.consume_close_paren() {
                    let child_field = self.consume_field_label();
                    let child = self.parse_item(parent_kind, child_field.as_deref());
                    item.merge(child);
                    if self.index >= self.tokens.len() {
                        break;
                    }
                }
                item
            }
            Some(QueryToken::Symbol(symbol)) if is_named_node_reference(symbol) => {
                let kind = match self.next() {
                    Some(QueryToken::Symbol(kind)) => kind,
                    _ => unreachable!("peeked a symbol before consuming a symbol"),
                };
                let mut item = ParsedInjectionItem {
                    target: Some(HighlightTarget::Node(kind.clone())),
                    capture_targets: vec![HighlightTarget::Node(kind.clone())],
                    ..ParsedInjectionItem::default()
                };
                while !self.consume_close_paren() {
                    let child_field = self.consume_field_label();
                    let child = self.parse_item(Some(&kind), child_field.as_deref());
                    item.merge(child);
                    if self.index >= self.tokens.len() {
                        break;
                    }
                }
                item
            }
            _ => {
                let mut item = ParsedInjectionItem::default();
                while !self.consume_close_paren() {
                    let child_field = self.consume_field_label();
                    let child = self.parse_item(parent_kind, child_field.as_deref().or(field_name));
                    item.merge(child);
                    if self.index >= self.tokens.len() {
                        break;
                    }
                }
                item
            }
        }
    }

    fn parse_list(
        &mut self,
        parent_kind: Option<&str>,
        field_name: Option<&str>,
    ) -> ParsedInjectionItem {
        let mut item = ParsedInjectionItem::default();
        while !self.consume_close_bracket() {
            let child = self.parse_item(parent_kind, field_name);
            if let Some(target) = child.target.clone() {
                item.capture_targets.push(target);
            } else {
                item.capture_targets.extend(child.capture_targets.clone());
            }
            item.merge(child);
            if self.index >= self.tokens.len() {
                break;
            }
        }
        item
    }

    fn parse_predicate(&mut self) -> ParsedInjectionItem {
        let op = match self.next() {
            Some(QueryToken::Symbol(op)) => op,
            _ => unreachable!("peeked a predicate before consuming one"),
        };
        let mut symbols = Vec::new();
        let mut strings = Vec::new();
        while !self.consume_close_paren() {
            match self.next() {
                Some(QueryToken::Symbol(symbol)) => symbols.push(symbol),
                Some(QueryToken::String(string)) => strings.push(string),
                Some(QueryToken::OpenParen) => {
                    let child = self.parse_form(None, None);
                    let mut item = ParsedInjectionItem::default();
                    item.merge(child);
                    return item;
                }
                Some(QueryToken::OpenBracket) => {
                    let child = self.parse_list(None, None);
                    let mut item = ParsedInjectionItem::default();
                    item.merge(child);
                    return item;
                }
                Some(QueryToken::CloseParen) | Some(QueryToken::CloseBracket) | None => break,
            }
            if self.index >= self.tokens.len() {
                break;
            }
        }
        let mut item = ParsedInjectionItem::default();
        if op != "#set!" {
            item.predicates = predicate_bindings_from_tokens(&op, &symbols, strings);
            return item;
        }
        if symbols.iter().any(|symbol| symbol == "injection.combined") {
            item.combined = true;
        }
        if symbols
            .iter()
            .any(|symbol| symbol == "injection.include-children")
        {
            item.include_children = true;
        }
        if symbols.iter().any(|symbol| symbol == "injection.language")
            && let Some(language) = strings.into_iter().next()
        {
            item.language = Some(language);
        }
        item
    }

    fn consume_captures(&mut self) -> Vec<String> {
        let mut captures = Vec::new();
        while let Some(QueryToken::Symbol(symbol)) = self.peek() {
            let Some(capture) = symbol.strip_prefix('@') else {
                break;
            };
            if capture.is_empty() || !capture.chars().all(is_capture_name_char) {
                break;
            }
            let capture = capture.to_owned();
            self.index += 1;
            captures.push(capture);
        }
        captures
    }

    fn consume_field_label(&mut self) -> Option<String> {
        let Some(QueryToken::Symbol(symbol)) = self.peek() else {
            return None;
        };
        let field = symbol.strip_suffix(':')?;
        if field.is_empty()
            || !field
                .chars()
                .all(|ch| ch.is_ascii_alphanumeric() || ch == '_')
        {
            return None;
        }
        let field = field.to_owned();
        self.index += 1;
        Some(field)
    }

    fn consume_close_paren(&mut self) -> bool {
        if matches!(self.peek(), Some(QueryToken::CloseParen)) {
            self.index += 1;
            true
        } else {
            false
        }
    }

    fn consume_close_bracket(&mut self) -> bool {
        if matches!(self.peek(), Some(QueryToken::CloseBracket)) {
            self.index += 1;
            true
        } else {
            false
        }
    }

    fn peek(&self) -> Option<&QueryToken> {
        self.tokens.get(self.index)
    }

    fn next(&mut self) -> Option<QueryToken> {
        let token = self.tokens.get(self.index).cloned();
        if token.is_some() {
            self.index += 1;
        }
        token
    }
}

impl ParsedInjectionItem {
    fn merge(&mut self, child: Self) {
        self.captures.extend(child.captures);
        self.predicates.extend(child.predicates);
        if self.language.is_none() {
            self.language = child.language;
        }
        self.combined |= child.combined;
        self.include_children |= child.include_children;
    }
}

fn apply_predicates(rules: &mut [HighlightRule], predicates: &[HighlightPredicateBinding]) {
    for predicate in predicates {
        for rule in rules
            .iter_mut()
            .filter(|rule| rule.capture_name == predicate.capture_name)
        {
            rule.predicates.push(predicate.predicate.clone());
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct RuntimeHighlightNode {
    id: TreeNodeId,
    kind: String,
    bytes: ByteRange,
    points: PointRange,
    text: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct RuntimeHighlightToken {
    text: String,
    bytes: ByteRange,
    points: PointRange,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct RuntimeHighlightField {
    parent: TreeNodeId,
    child: Option<TreeNodeId>,
    field_name: String,
}

fn runtime_highlight_nodes(
    parser: &ParserGrammar,
    tree_events: &[TreeEvent],
    input: &str,
) -> Vec<RuntimeHighlightNode> {
    tree_events
        .iter()
        .filter_map(|event| match event {
            TreeEvent::Reduce {
                metadata,
                node,
                bytes,
                points,
                ..
            } => {
                let metadata_row = &parser.production_metadata()[metadata.get() as usize];
                let public_node = metadata_row.public_node()?;
                let kind = parser.public_node_kinds()[public_node.get() as usize]
                    .name()
                    .to_owned();
                Some(RuntimeHighlightNode {
                    id: *node,
                    kind,
                    bytes: *bytes,
                    points: *points,
                    text: input_text(input, *bytes).to_owned(),
                })
            }
            TreeEvent::Alias {
                node,
                alias,
                named: true,
                bytes,
                points,
                ..
            } => {
                let kind = parser.aliases()[alias.get() as usize].value().to_owned();
                Some(RuntimeHighlightNode {
                    id: *node,
                    kind,
                    bytes: *bytes,
                    points: *points,
                    text: input_text(input, *bytes).to_owned(),
                })
            }
            TreeEvent::CloseNode {
                node,
                public_node: Some(public_node),
                bytes,
                points,
                ..
            } => {
                let kind = parser.public_node_kinds()[public_node.get() as usize]
                    .name()
                    .to_owned();
                Some(RuntimeHighlightNode {
                    id: *node,
                    kind,
                    bytes: *bytes,
                    points: *points,
                    text: input_text(input, *bytes).to_owned(),
                })
            }
            _ => None,
        })
        .collect()
}

fn runtime_highlight_tokens(
    parser: &ParserGrammar,
    tree_events: &[TreeEvent],
    input: &str,
) -> Vec<RuntimeHighlightToken> {
    tree_events
        .iter()
        .filter_map(|event| match event {
            TreeEvent::Token {
                symbol,
                bytes,
                points,
                ..
            } => {
                let text = input_text(input, *bytes);
                let query_visible = match symbol {
                    ParserSymbol::Terminal(terminal) => {
                        let terminal = &parser.symbols().terminals()[terminal.get() as usize];
                        terminal.public_names().iter().any(|name| name == text)
                            || terminal.spelling() == text
                    }
                    ParserSymbol::External(_) => true,
                    _ => false,
                };
                query_visible.then(|| RuntimeHighlightToken {
                    text: text.to_owned(),
                    bytes: *bytes,
                    points: *points,
                })
            }
            _ => None,
        })
        .collect()
}

fn runtime_highlight_fields(
    parser: &ParserGrammar,
    tree_events: &[TreeEvent],
) -> Vec<RuntimeHighlightField> {
    tree_events
        .iter()
        .filter_map(|event| match event {
            TreeEvent::Field {
                node, child, field, ..
            } => Some(RuntimeHighlightField {
                parent: *node,
                child: *child,
                field_name: parser.fields()[field.get() as usize].name().to_owned(),
            }),
            _ => None,
        })
        .collect()
}

fn node_satisfies_edge_constraints(
    node: &RuntimeHighlightNode,
    rule: &HighlightRule,
    nodes: &[RuntimeHighlightNode],
    fields: &[RuntimeHighlightField],
) -> bool {
    if let Some(field_name) = &rule.field_name {
        return fields.iter().any(|field| {
            if field.child != Some(node.id) || &field.field_name != field_name {
                return false;
            }
            let Some(parent) = nodes.iter().find(|candidate| candidate.id == field.parent) else {
                return false;
            };
            rule.parent_kind
                .as_ref()
                .is_none_or(|parent_kind| &parent.kind == parent_kind)
        });
    }
    let parent = direct_parent_node(node, nodes);
    if let Some(parent_kind) = &rule.parent_kind
        && !parent.is_some_and(|parent| &parent.kind == parent_kind)
    {
        return false;
    }
    true
}

fn direct_parent_node<'a>(
    node: &RuntimeHighlightNode,
    nodes: &'a [RuntimeHighlightNode],
) -> Option<&'a RuntimeHighlightNode> {
    direct_parent_range(node.bytes, nodes).filter(|parent| parent.id != node.id)
}

fn direct_parent_range(
    bytes: ByteRange,
    nodes: &[RuntimeHighlightNode],
) -> Option<&RuntimeHighlightNode> {
    nodes
        .iter()
        .filter(|candidate| byte_range_strictly_contains(candidate.bytes, bytes))
        .min_by_key(|candidate| byte_range_len(candidate.bytes))
}

fn token_has_direct_parent_kind(
    token: &RuntimeHighlightToken,
    parent_kind: &str,
    nodes: &[RuntimeHighlightNode],
) -> bool {
    nodes
        .iter()
        .filter(|candidate| byte_range_contains(candidate.bytes, token.bytes))
        .min_by_key(|candidate| {
            candidate
                .bytes
                .end()
                .get()
                .saturating_sub(candidate.bytes.start().get())
        })
        .is_some_and(|parent| parent.kind == parent_kind)
}

fn byte_range_len(bytes: ByteRange) -> u32 {
    bytes.end().get().saturating_sub(bytes.start().get())
}

fn byte_range_contains(outer: ByteRange, inner: ByteRange) -> bool {
    outer.start() <= inner.start() && inner.end() <= outer.end()
}

fn byte_range_strictly_contains(outer: ByteRange, inner: ByteRange) -> bool {
    outer.start() <= inner.start()
        && inner.end() <= outer.end()
        && (outer.start() < inner.start() || inner.end() < outer.end())
}

fn input_text(input: &str, bytes: ByteRange) -> &str {
    let start = bytes.start().get() as usize;
    let end = bytes.end().get() as usize;
    input.get(start..end).unwrap_or("")
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum QueryContext {
    Root,
    List,
    Form { seen_head: bool, predicate: bool },
}

impl QueryContext {
    const fn is_predicate(&self) -> bool {
        matches!(
            self,
            Self::Form {
                predicate: true,
                ..
            }
        )
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum QueryToken {
    OpenParen,
    CloseParen,
    OpenBracket,
    CloseBracket,
    String(String),
    Symbol(String),
}

struct QueryScanner<'a> {
    source: &'a str,
    index: usize,
}

impl<'a> QueryScanner<'a> {
    const fn new(source: &'a str) -> Self {
        Self { source, index: 0 }
    }

    fn next_token(&mut self) -> Option<QueryToken> {
        self.skip_ws_and_comments();
        let ch = self.peek_char()?;
        match ch {
            '(' => {
                self.index += ch.len_utf8();
                Some(QueryToken::OpenParen)
            }
            ')' => {
                self.index += ch.len_utf8();
                Some(QueryToken::CloseParen)
            }
            '[' => {
                self.index += ch.len_utf8();
                Some(QueryToken::OpenBracket)
            }
            ']' => {
                self.index += ch.len_utf8();
                Some(QueryToken::CloseBracket)
            }
            '"' => Some(QueryToken::String(self.string_token())),
            _ => Some(QueryToken::Symbol(self.symbol_token())),
        }
    }

    fn skip_ws_and_comments(&mut self) {
        loop {
            let Some(ch) = self.peek_char() else {
                return;
            };
            if ch.is_whitespace() {
                self.index += ch.len_utf8();
                continue;
            }
            if ch == ';' {
                while let Some(ch) = self.peek_char() {
                    self.index += ch.len_utf8();
                    if ch == '\n' {
                        break;
                    }
                }
                continue;
            }
            return;
        }
    }

    fn string_token(&mut self) -> String {
        debug_assert_eq!(self.peek_char(), Some('"'));
        self.index += '"'.len_utf8();
        let mut value = String::new();
        let mut escaped = false;
        while let Some(ch) = self.peek_char() {
            self.index += ch.len_utf8();
            if escaped {
                value.push(match ch {
                    'n' => '\n',
                    'r' => '\r',
                    't' => '\t',
                    '\\' => '\\',
                    '"' => '"',
                    other => other,
                });
                escaped = false;
                continue;
            }
            match ch {
                '\\' => escaped = true,
                '"' => break,
                _ => value.push(ch),
            }
        }
        value
    }

    fn symbol_token(&mut self) -> String {
        let start = self.index;
        while let Some(ch) = self.peek_char() {
            if ch.is_whitespace() || matches!(ch, '(' | ')' | '[' | ']' | '"') || ch == ';' {
                break;
            }
            self.index += ch.len_utf8();
        }
        self.source[start..self.index].to_owned()
    }

    fn peek_char(&self) -> Option<char> {
        self.source.get(self.index..)?.chars().next()
    }
}

pub(crate) fn is_capture_name_char(ch: char) -> bool {
    ch.is_ascii_alphanumeric() || matches!(ch, '_' | '-' | '.')
}

fn is_named_node_reference(symbol: &str) -> bool {
    if symbol.is_empty()
        || symbol.starts_with('@')
        || symbol.starts_with('#')
        || symbol.ends_with(':')
        || matches!(symbol, "_" | "." | "*" | "+" | "?" | "...")
    {
        return false;
    }
    symbol
        .chars()
        .all(|ch| ch.is_ascii_alphanumeric() || matches!(ch, '_' | '-'))
}

/// Well-known Tree-sitter query categories.
#[derive(Debug, Clone, Copy, Facet, PartialEq, Eq)]
#[repr(u8)]
pub enum WellKnownQuery {
    /// Highlight query.
    Highlights,
    /// Locals query.
    Locals,
    /// Injections query.
    Injections,
    /// Tags query.
    Tags,
}

impl WellKnownQuery {
    /// Default filename used by Tree-sitter packages.
    pub const fn filename(self) -> &'static str {
        match self {
            Self::Highlights => "highlights.scm",
            Self::Locals => "locals.scm",
            Self::Injections => "injections.scm",
            Self::Tags => "tags.scm",
        }
    }
}

/// Imported query files. Unknown query files are preserved.
#[derive(Debug, Clone, Default, Facet, PartialEq, Eq)]
pub struct QueryBundle {
    /// Query source files with category resolution.
    pub files: Vec<QueryFile>,
}

/// Imported query source file with Tree-sitter category metadata.
#[derive(Debug, Clone, Facet, PartialEq, Eq)]
pub struct QueryFile {
    /// Well-known category, when this file was resolved through category semantics.
    pub category: Option<WellKnownQuery>,
    /// Whether the file came from `tree-sitter.json` rather than fallback discovery.
    pub configured: bool,
    /// Query source file.
    pub source: SourceFile<QuerySource>,
}

impl QueryBundle {
    /// Get a well-known query file by default filename.
    pub fn well_known(&self, query: WellKnownQuery) -> Option<&SourceFile<QuerySource>> {
        self.files
            .iter()
            .find(|file| file.category == Some(query))
            .map(|file| &file.source)
    }

    /// Iterate well-known query files in configured order.
    pub fn well_known_files(
        &self,
        query: WellKnownQuery,
    ) -> impl Iterator<Item = &SourceFile<QuerySource>> {
        self.files
            .iter()
            .filter(move |file| file.category == Some(query))
            .map(|file| &file.source)
    }

    /// Iterate all query files.
    pub fn iter(&self) -> impl Iterator<Item = &SourceFile<QuerySource>> {
        self.files.iter().map(|file| &file.source)
    }

    /// Iterate all query files with category metadata.
    pub fn iter_files(&self) -> impl Iterator<Item = &QueryFile> {
        self.files.iter()
    }
}

#[cfg(test)]
mod tests {
    use crate::{
        grammar::RawGrammarJson,
        lexical::LexicalFacts,
        parser::{ParseTable, ParserGrammar, RuntimeParser},
        validated::ValidatedGrammar,
    };

    use super::{
        QuerySource, anonymous_node_literals, capture_names, injection_patterns,
        named_node_references,
    };

    #[test]
    fn extracts_query_anonymous_node_literals() {
        let query = r##"
          "~" @operator
          ["#" "," "."] @punctuation.delimiter
          (("and") @keyword)
          ("\"" @punctuation.delimiter)
          ((property_name) @variable
            (#match? @variable "^--"))
          ; comments can contain "ignored" strings
        "##;

        let literals = anonymous_node_literals(query);

        assert!(literals.contains("~"));
        assert!(literals.contains("#"));
        assert!(literals.contains(","));
        assert!(literals.contains("."));
        assert!(literals.contains("and"));
        assert!(literals.contains("\""));
        assert!(!literals.contains("^--"));
        assert!(!literals.contains("ignored"));
    }

    #[test]
    fn query_source_reports_anonymous_node_literals() {
        let source = QuerySource(r#""@media" @keyword"#.to_owned());

        assert!(source.anonymous_node_literals().contains("@media"));
    }

    #[test]
    fn extracts_query_capture_names_without_comments_strings_or_predicates() {
        let query = r##"
          ; @commented-out
          ((property_name) @property)
          ((string_value) @string.special)
          ((custom_property_name) @custom
            (#match? @variable "^--")
            (#eq? @variable "@inside-string"))
          "@literal" @operator
        "##;

        let captures = capture_names(query);

        assert!(captures.contains("property"));
        assert!(captures.contains("string.special"));
        assert!(captures.contains("custom"));
        assert!(captures.contains("operator"));
        assert!(!captures.contains("commented-out"));
        assert!(!captures.contains("variable"));
        assert!(!captures.contains("inside-string"));
        assert!(!captures.contains("literal"));
    }

    #[test]
    fn query_source_reports_capture_names() {
        let source = QuerySource(r#"((property_name) @property)"#.to_owned());

        assert!(source.capture_names().contains("property"));
    }

    #[test]
    fn extracts_named_node_references_without_predicates_or_fields() {
        let query = r##"
          (attribute_selector
            name: (attribute_name) @attribute
            (plain_value) @string)
          ((custom_property_name) @custom
            (#match? @custom "^--"))
          ["~" ">"] @operator
          ; (commented_node) @ignored
        "##;

        let nodes = named_node_references(query);

        assert!(nodes.contains("attribute_selector"));
        assert!(nodes.contains("attribute_name"));
        assert!(nodes.contains("plain_value"));
        assert!(nodes.contains("custom_property_name"));
        assert!(!nodes.contains("name:"));
        assert!(!nodes.contains("match?"));
        assert!(!nodes.contains("operator"));
        assert!(!nodes.contains("commented_node"));
    }

    #[test]
    fn query_source_reports_named_node_references() {
        let source = QuerySource(r#"((property_name) @property)"#.to_owned());

        assert!(source.named_node_references().contains("property_name"));
    }

    #[test]
    fn query_string_escapes_decode_once() {
        let literals = anonymous_node_literals(r#""\n" "\t" "\r" "\\" "\"" "#);

        assert!(literals.contains("\n"));
        assert!(literals.contains("\t"));
        assert!(literals.contains("\r"));
        assert!(literals.contains("\\"));
        assert!(literals.contains("\""));
    }

    #[test]
    fn parses_injection_set_properties() {
        let patterns = injection_patterns(
            r#"
              ((lua_code) @injection.content
                (#set! injection.language "lua")
                (#set! injection.combined)
                (#set! injection.include-children))
            "#,
        );

        assert_eq!(patterns.len(), 1);
        let pattern = &patterns[0];
        assert_eq!(pattern.language.as_deref(), Some("lua"));
        assert!(pattern.combined);
        assert!(pattern.include_children);
        assert_eq!(pattern.captures.len(), 1);
        assert_eq!(pattern.captures[0].capture_name, "injection.content");
    }

    #[test]
    fn extracts_runtime_injection_regions() {
        let raw = RawGrammarJson::from_tree_sitter_json_str(
            r#"{
              "name": "injection_smoke",
              "rules": {
                "document": { "type": "SYMBOL", "name": "lua_code" },
                "lua_code": {
                  "type": "TOKEN",
                  "content": { "type": "PATTERN", "value": "[A-Za-z_]+" }
                }
              },
              "extras": [],
              "conflicts": [],
              "precedences": [],
              "externals": [],
              "inline": [],
              "supertypes": []
            }"#,
        )
        .unwrap();
        let validated = ValidatedGrammar::from_raw(&raw).unwrap();
        let lexical = LexicalFacts::from_grammar(&validated);
        let parser = ParserGrammar::normalize_from_validated(&validated, &lexical)
            .unwrap()
            .prepare_productions_for_items()
            .unwrap();
        let table = ParseTable::from_grammar(&parser).unwrap();
        let report = RuntimeParser::new(&validated, &parser, &table)
            .unwrap()
            .parse_compact_with_report("print")
            .unwrap();
        let query = QuerySource(
            r#"((lua_code) @injection.content
                (#set! injection.language "lua")
                (#set! injection.combined))"#
                .to_owned(),
        );

        let regions = query.execute_runtime_injections(&parser, &report, "print");

        assert_eq!(regions.len(), 1);
        assert_eq!(regions[0].language(), "lua");
        assert!(regions[0].combined());
        assert!(!regions[0].include_children());
        assert_eq!(regions[0].text(), "print");
        assert_eq!(regions[0].bytes().start().get(), 0);
        assert_eq!(regions[0].bytes().end().get(), 5);
    }

    #[test]
    fn pairs_dynamic_injection_languages_with_sibling_content() {
        let raw = RawGrammarJson::from_tree_sitter_json_str(
            r#"{
              "name": "injection_dynamic",
              "rules": {
                "document": {
                  "type": "REPEAT",
                  "content": { "type": "SYMBOL", "name": "block" }
                },
                "block": {
                  "type": "SEQ",
                  "members": [
                    { "type": "SYMBOL", "name": "lang" },
                    { "type": "STRING", "value": ":" },
                    { "type": "SYMBOL", "name": "code" },
                    { "type": "STRING", "value": ";" }
                  ]
                },
                "lang": {
                  "type": "TOKEN",
                  "content": { "type": "PATTERN", "value": "[a-z]+" }
                },
                "code": {
                  "type": "TOKEN",
                  "content": { "type": "PATTERN", "value": "[A-Z]+" }
                }
              },
              "extras": [],
              "conflicts": [],
              "precedences": [],
              "externals": [],
              "inline": [],
              "supertypes": []
            }"#,
        )
        .unwrap();
        let validated = ValidatedGrammar::from_raw(&raw).unwrap();
        let lexical = LexicalFacts::from_grammar(&validated);
        let parser = ParserGrammar::normalize_from_validated(&validated, &lexical)
            .unwrap()
            .prepare_productions_for_items()
            .unwrap();
        let table = ParseTable::from_grammar(&parser).unwrap();
        let input = "lua:PRINT;js:RUN;";
        let report = RuntimeParser::new(&validated, &parser, &table)
            .unwrap()
            .parse_compact_with_report(input)
            .unwrap();
        let query = QuerySource(
            r#"((block
                  (lang) @injection.language
                  (code) @injection.content))"#
                .to_owned(),
        );

        let regions = query.execute_runtime_injections(&parser, &report, input);

        assert_eq!(
            regions
                .iter()
                .map(|region| (region.language(), region.text()))
                .collect::<Vec<_>>(),
            vec![("lua", "PRINT"), ("js", "RUN")]
        );
    }

    #[test]
    fn filters_injection_patterns_with_capture_predicates() {
        let raw = RawGrammarJson::from_tree_sitter_json_str(
            r#"{
              "name": "injection_predicate",
              "rules": {
                "document": {
                  "type": "REPEAT",
                  "content": { "type": "SYMBOL", "name": "block" }
                },
                "block": {
                  "type": "SEQ",
                  "members": [
                    { "type": "SYMBOL", "name": "tag" },
                    { "type": "STRING", "value": ":" },
                    { "type": "SYMBOL", "name": "code" },
                    { "type": "STRING", "value": ";" }
                  ]
                },
                "tag": {
                  "type": "TOKEN",
                  "content": { "type": "PATTERN", "value": "[a-z]+" }
                },
                "code": {
                  "type": "TOKEN",
                  "content": { "type": "PATTERN", "value": "[A-Z]+" }
                }
              },
              "extras": [],
              "conflicts": [],
              "precedences": [],
              "externals": [],
              "inline": [],
              "supertypes": []
            }"#,
        )
        .unwrap();
        let validated = ValidatedGrammar::from_raw(&raw).unwrap();
        let lexical = LexicalFacts::from_grammar(&validated);
        let parser = ParserGrammar::normalize_from_validated(&validated, &lexical)
            .unwrap()
            .prepare_productions_for_items()
            .unwrap();
        let table = ParseTable::from_grammar(&parser).unwrap();
        let input = "pwsh:PRINT;hbs:RUN;";
        let report = RuntimeParser::new(&validated, &parser, &table)
            .unwrap()
            .parse_compact_with_report(input)
            .unwrap();
        let query = QuerySource(
            r#"((block
                  (tag) @_name
                  (code) @injection.content)
                (#match? @_name ".*(powershell|pwsh|cmd).*")
                (#set! injection.language "powershell"))
               ((block
                  (tag) @_name
                  (code) @injection.content)
                (#any-of? @_name "hbs" "glimmer")
                (#set! injection.language "html"))
               ((block
                  (tag) @_name
                  (code) @injection.content)
                (#eq? @_name "sql")
                (#set! injection.language "sql"))"#
                .to_owned(),
        );

        let regions = query.execute_runtime_injections(&parser, &report, input);

        assert_eq!(
            regions
                .iter()
                .map(|region| (region.language(), region.text()))
                .collect::<Vec<_>>(),
            vec![("powershell", "PRINT"), ("html", "RUN")]
        );
    }
}
