# Phase 006: styx-cst (Rowan CST)

Lossless Concrete Syntax Tree using the rowan library. Preserves all whitespace, comments, and exact source representation for tooling.

## Deliverables

- `crates/styx-cst/src/lib.rs` - Crate root
- `crates/styx-cst/src/syntax_kind.rs` - Syntax node/token kinds
- `crates/styx-cst/src/parser.rs` - CST parser (may reuse styx-parse lexer)
- `crates/styx-cst/src/ast.rs` - Typed AST wrappers over CST nodes
- `crates/styx-cst/src/validation.rs` - Semantic validation

## Why Rowan?

Rowan provides:
- Lossless representation (source can be exactly reconstructed)
- Cheap cloning via reference counting
- Parent pointers for navigation
- Incremental reparsing support
- Used by rust-analyzer, proven at scale

## Syntax Kinds

```rust
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[repr(u16)]
pub enum SyntaxKind {
    // Tokens (terminals)
    L_BRACE,        // {
    R_BRACE,        // }
    L_PAREN,        // (
    R_PAREN,        // )
    COMMA,          // ,
    EQ,             // =
    AT,             // @
    
    BARE_SCALAR,
    QUOTED_SCALAR,
    RAW_SCALAR,
    HEREDOC_MARKER, // <<DELIM
    HEREDOC_CONTENT,
    HEREDOC_END,
    
    WHITESPACE,
    NEWLINE,
    LINE_COMMENT,
    DOC_COMMENT,
    
    ERROR,
    
    // Nodes (non-terminals)
    DOCUMENT,
    ENTRY,
    OBJECT,
    SEQUENCE,
    SCALAR,
    UNIT,
    TAG,
    TAG_NAME,
    TAG_PAYLOAD,
    ATTRIBUTES,
    ATTRIBUTE,
    KEY,
    VALUE,
    HEREDOC,
}

impl From<SyntaxKind> for rowan::SyntaxKind {
    fn from(kind: SyntaxKind) -> Self {
        rowan::SyntaxKind(kind as u16)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum StyxLanguage {}

impl rowan::Language for StyxLanguage {
    type Kind = SyntaxKind;
    
    fn kind_from_raw(raw: rowan::SyntaxKind) -> Self::Kind {
        unsafe { std::mem::transmute(raw.0) }
    }
    
    fn kind_to_raw(kind: Self::Kind) -> rowan::SyntaxKind {
        rowan::SyntaxKind(kind as u16)
    }
}

pub type SyntaxNode = rowan::SyntaxNode<StyxLanguage>;
pub type SyntaxToken = rowan::SyntaxToken<StyxLanguage>;
pub type SyntaxElement = rowan::SyntaxElement<StyxLanguage>;
```

## CST Parser

The CST parser builds a green tree using rowan's GreenNodeBuilder:

```rust
pub struct CstParser<'src> {
    lexer: Lexer<'src>,
    builder: GreenNodeBuilder<'static>,
    errors: Vec<ParseError>,
}

impl<'src> CstParser<'src> {
    pub fn new(source: &'src str) -> Self;
    
    pub fn parse(mut self) -> Parse {
        self.builder.start_node(SyntaxKind::DOCUMENT.into());
        
        while !self.at_eof() {
            self.parse_entry();
        }
        
        self.builder.finish_node();
        
        Parse {
            green: self.builder.finish(),
            errors: self.errors,
        }
    }
}

pub struct Parse {
    green: GreenNode,
    errors: Vec<ParseError>,
}

impl Parse {
    pub fn syntax(&self) -> SyntaxNode {
        SyntaxNode::new_root(self.green.clone())
    }
    
    pub fn errors(&self) -> &[ParseError] {
        &self.errors
    }
    
    pub fn ok(self) -> Result<SyntaxNode, Vec<ParseError>> {
        if self.errors.is_empty() {
            Ok(self.syntax())
        } else {
            Err(self.errors)
        }
    }
}
```

## Typed AST Layer

Wrap raw CST nodes with typed accessors:

```rust
// Macro for defining AST nodes
macro_rules! ast_node {
    ($name:ident, $kind:expr) => {
        #[derive(Debug, Clone, PartialEq, Eq, Hash)]
        pub struct $name(SyntaxNode);
        
        impl $name {
            pub fn cast(node: SyntaxNode) -> Option<Self> {
                if node.kind() == $kind {
                    Some(Self(node))
                } else {
                    None
                }
            }
            
            pub fn syntax(&self) -> &SyntaxNode {
                &self.0
            }
        }
    };
}

ast_node!(Document, SyntaxKind::DOCUMENT);
ast_node!(Entry, SyntaxKind::ENTRY);
ast_node!(Object, SyntaxKind::OBJECT);
ast_node!(Sequence, SyntaxKind::SEQUENCE);
ast_node!(Scalar, SyntaxKind::SCALAR);
ast_node!(Tag, SyntaxKind::TAG);

impl Document {
    pub fn entries(&self) -> impl Iterator<Item = Entry> {
        self.0.children().filter_map(Entry::cast)
    }
}

impl Object {
    pub fn entries(&self) -> impl Iterator<Item = Entry> {
        self.0.children().filter_map(Entry::cast)
    }
    
    pub fn separator(&self) -> Separator {
        // Detect from tokens
    }
}

impl Entry {
    pub fn key(&self) -> Option<SyntaxNode> {
        self.0.children()
            .find(|n| n.kind() == SyntaxKind::KEY)
    }
    
    pub fn value(&self) -> Option<SyntaxNode> {
        self.0.children()
            .find(|n| n.kind() == SyntaxKind::VALUE)
    }
    
    pub fn doc_comment(&self) -> Option<String> {
        // Find preceding DOC_COMMENT tokens
    }
}

impl Scalar {
    pub fn text(&self) -> &str {
        // Get text, handling escapes
    }
    
    pub fn raw_text(&self) -> &str {
        self.0.text()
    }
    
    pub fn kind(&self) -> ScalarKind {
        // Determine from first token
    }
}

impl Tag {
    pub fn name(&self) -> Option<&str> {
        self.0.children()
            .find(|n| n.kind() == SyntaxKind::TAG_NAME)
            .map(|n| n.text())
    }
    
    pub fn payload(&self) -> Option<SyntaxNode> {
        self.0.children()
            .find(|n| n.kind() == SyntaxKind::TAG_PAYLOAD)
    }
}
```

## Validation

Semantic validation on top of CST:

```rust
pub fn validate(root: &SyntaxNode) -> Vec<Diagnostic> {
    let mut diagnostics = Vec::new();
    
    validate_node(root, &mut diagnostics);
    
    diagnostics
}

fn validate_node(node: &SyntaxNode, diagnostics: &mut Vec<Diagnostic>) {
    match node.kind() {
        SyntaxKind::OBJECT => validate_object(node, diagnostics),
        SyntaxKind::ENTRY => validate_entry(node, diagnostics),
        _ => {}
    }
    
    for child in node.children() {
        validate_node(&child, diagnostics);
    }
}

fn validate_object(node: &SyntaxNode, diagnostics: &mut Vec<Diagnostic>) {
    // Check for mixed separators
    // Check for duplicate keys
}
```

## Source Reconstruction

```rust
impl SyntaxNode {
    /// Reconstruct exact source text
    pub fn text(&self) -> String {
        self.to_string()  // rowan provides this
    }
}
```

## Use Cases

1. **Formatter**: Parse → modify tree → emit
2. **LSP**: Parse → navigate → provide completions/diagnostics
3. **Refactoring**: Find references, rename, restructure
4. **Schema-aware highlighting**: Parse → validate against schema → annotate

## Testing

- Round-trip: source → CST → text == source
- AST accessor correctness
- Validation catches expected errors
- Error recovery produces usable tree
