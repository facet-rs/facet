# Extension Attributes Architecture Diagrams

Render with: https://mermaid.live or VS Code Mermaid extension

---

## Diagram 1: Grammar Definition Time

```mermaid
flowchart TD
    A[Extension crate calls define_attr_grammar!] --> B[__make_parse_attr! proc-macro]
    B --> C[enum Attr + structs]
    B --> D[__parse_attr! macro]
    B --> E[proc-macro re-exports]
    C --> F[All live in extension crate]
    D --> F
    E --> F
```

**What each box means:**
- `define_attr_grammar!` — user-facing macro in extension crate (e.g., facet-kdl)
- `__make_parse_attr!` — proc-macro in facet-macros that compiles the grammar
- `enum Attr + structs` — the type definitions for parsed attributes
- `__parse_attr!` — declarative macro containing the compiled grammar patterns
- `proc-macro re-exports` — `__dispatch_attr`, `__build_struct_fields`, `__attr_error`

---

## Diagram 2: Attribute Usage Flow

```mermaid
flowchart TD
    A["User: #[facet(orm::column(primary_key))]"] --> B[Facet derive macro]
    B --> C["Generates: orm::__parse_attr!(column(primary_key))"]
    C --> D[__parse_attr! in orm crate]
    D --> E[__dispatch_attr! proc-macro]
    E --> F{variant type?}
    F -->|unit| G["orm::Attr::Skip"]
    F -->|newtype| H["orm::Attr::Rename(value)"]
    F -->|struct| I[__build_struct_fields!]
    I --> J["orm::Attr::Column(Column{...})"]
```

---

## Diagram 3: Error Flow

```mermaid
flowchart TD
    A["User typo: #[facet(orm::colum(...))]"] --> B[__parse_attr!]
    B --> C[__dispatch_attr!]
    C --> D{colum in known variants?}
    D -->|no| E[__attr_error! proc-macro]
    E --> F[strsim finds closest: column]
    F --> G["compile_error! with suggestion"]
    G --> H[Error points to user's typo]
```

---

## Diagram 4: Storage Model

```mermaid
flowchart LR
    subgraph Input
        A["#[facet(skip)]"]
        B["#[facet(kdl::child)]"]
    end

    subgraph Parsing
        C[facet __parse_attr!]
        D[kdl __parse_attr!]
    end

    subgraph Output
        E["ExtensionAttr{ns:'', key:'skip'}"]
        F["ExtensionAttr{ns:'kdl', key:'child'}"]
    end

    A --> C --> E
    B --> D --> F

    E --> G[Field.attributes]
    F --> G
```

**Key insight:** Both built-in (`ns: ""`) and extension (`ns: "kdl"`) attrs become `ExtensionAttr`.

---

## Diagram 5: Crate Dependencies

```mermaid
flowchart BT
    A[facet-core] --> B[facet-macros-impl]
    B --> C[facet-macros]
    A --> D[facet]
    C --> D
    D --> E[facet-kdl / facet-args / etc]
    E --> F[user crate]
    D --> F
```

---

## Diagram 6: Sequence

```mermaid
sequenceDiagram
    participant Ext as Extension Crate
    participant Compiler as __make_parse_attr!
    participant User as User Code
    participant Derive as derive(Facet)
    participant Parse as __parse_attr!
    participant Dispatch as __dispatch_attr!

    Note over Ext,Compiler: Phase 1: Grammar Definition
    Ext->>Compiler: define_attr_grammar!{...}
    Compiler-->>Ext: enum Attr, __parse_attr! macro

    Note over User,Dispatch: Phase 2: Attribute Usage
    User->>Derive: #[facet(orm::column(pk))]
    Derive-->>User: orm::__parse_attr!(column(pk))
    User->>Parse: expand
    Parse->>Dispatch: @name{column} @rest{(pk)}
    Dispatch-->>User: ExtensionAttr{ns:"orm", key:"column", ...}
```
