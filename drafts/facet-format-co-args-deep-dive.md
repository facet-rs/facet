# Command-Line Arguments: Deep Dive for format-co

## Executive Summary

facet-args is a **sophisticated, feature-rich** argument parser that already implements many advanced features:
- ✅ Positional and named arguments
- ✅ Short and long flags (`-v`, `--verbose`)
- ✅ Flag chaining (`-abc` → `-a -b -c`)
- ✅ Subcommands with nested parsing
- ✅ Help generation and completions
- ✅ Detailed error messages with spans

**Assessment**: ⚠️ **Moderate Fit** for format-co

The format-co approach could benefit facet-args by:
1. **Unified evidence collection** for subcommand disambiguation
2. **Consistent ParseEvent abstraction** across formats
3. **Simplified parser logic** through event streaming

However, facet-args has unique requirements that make direct format-co adoption challenging:
- **Help generation** needs schema analysis before parsing
- **Error reporting** requires precise span tracking
- **Shell completions** need partial parse support
- **Performance** matters for CLI startup time

**Recommendation**: **Hybrid approach** - Use format-co patterns for core parsing logic while maintaining specialized features for CLI needs.

---

## Current Architecture Analysis

### High-Level Flow

```rust
std::env::args()
  → from_slice(&[&str])
  → Context::work_inner()
  → parse_struct(Partial)
  → HeapValue
  → materialize::<T>()
```

### Key Components

#### 1. Context State Machine

```rust
struct Context<'input> {
    shape: &'static Shape,          // Target type schema
    args: &'input [&'input str],    // Input arguments
    index: usize,                   // Current position
    positional_only: bool,          // After `--` flag
    arg_indices: Vec<usize>,        // For span tracking
    flattened_args: String,         // For error messages
}
```

**State tracking**:
- `index`: Current argument being parsed
- `positional_only`: Whether we've seen `--` (disables flag parsing)

#### 2. Argument Classification

```rust
enum ArgType {
    DoubleDash,             // `--`
    LongFlag(&str),         // `--verbose`, `--output=file.txt`
    ShortFlag(&str),        // `-v`, `-j4`, `-abc`
    Positional,             // `input.txt`
    None,
}
```

The parser scans each argument and classifies it, then dispatches to appropriate handlers.

#### 3. Parsing Strategies

**Struct parsing** (lines 249-414):
- Loop through arguments
- Classify each as flag or positional
- Match flags to struct fields via name or `args::short` attribute
- Handle `=` syntax (`--key=value`)
- Support flag chaining (`-abc`)
- Fill unset fields with defaults at end

**Subcommand parsing** (lines 570-643):
- Field marked with `#[facet(args::subcommand)]`
- Argument is variant name (kebab-case)
- Select variant and parse its fields recursively

**Variant field parsing** (lines 417-567):
- Similar to struct parsing but for enum variant data
- Supports nested subcommands

#### 4. Advanced Features

**Flag chaining** (lines 850-957):
```rust
// `-abc` → process `-a`, then `-bc`, then `-c`
fn process_short_flag(&mut self, p: Partial, flag: &str, ...) {
    let first_char = flag.chars().next();
    let rest = &flag[first_char.len()..];

    if is_bool {
        p = set_bool_field(p, first_char)?;
        if !rest.is_empty() {
            p = self.process_short_flag(p, rest, ...)?; // Recursive!
        }
    } else {
        // Non-bool with trailing chars = attached value
        p = set_field_with_value(p, first_char, rest)?;
    }
}
```

**Subcommand disambiguation** (lines 722-771):
```rust
fn find_variant_by_name(enum_type: EnumType, name: &str) -> Result<&Variant> {
    // 1. Check for explicit rename: #[facet(rename = "cmd")]
    // 2. Check kebab-case: VariantName → variant-name
    // 3. Check exact match
}
```

**Help integration** (lines 36-45):
- Check for `-h`, `--help`, `-help`, `/?` as first arg
- Generate help text from schema
- Return as error (to print and exit)

---

## format-co Mapping Analysis

### What Would a format-co Parser Look Like?

#### ParseEvent Stream Example

Input: `--verbose -j 4 input.txt --output=result.txt build`

ParseEvent sequence:
```
StructStart
  FieldKey("verbose", KeyValue)
    Scalar(Bool(true))           // Flag present = true
  FieldKey("concurrency", KeyValue)  // From `-j` short flag
    Scalar(Str("4"))             // Needs parsing to usize
  FieldKey("path", Argument)     // Positional
    Scalar(Str("input.txt"))
  FieldKey("output", KeyValue)
    Scalar(Str("result.txt"))
  FieldKey("command", Argument)  // Subcommand
    VariantTag("build")
    StructStart
      // ... variant fields
    StructEnd
StructEnd
```

#### Evidence Collection for Subcommands

Input: `build --release --target x86_64`

Top-level evidence:
```rust
[
  FieldEvidence { name: "build", location: Argument, type_hint: Some(String) }
]
```

When deserializing the `command` field (an enum), the solver queries:
- Available variants: `Build`, `Test`, `Run`, `Clean`
- Probe input: "build"
- Match: `Build` variant (via kebab-case or rename)

Then probe for variant-specific flags:
```rust
// After selecting Build variant, collect evidence for its fields
[
  FieldEvidence { name: "release", location: KeyValue, type_hint: Some(Bool) },
  FieldEvidence { name: "target", location: KeyValue, type_hint: Some(String) }
]
```

---

## Challenges for format-co Adoption

### 1. Stateful Parsing with Backtracking

**Problem**: Command-line parsing is inherently stateful and order-dependent.

```bash
prog -v file.txt --flag value
```

vs.

```bash
prog file.txt -v --flag value
```

These parse identically, but the order matters for error messages and help text.

**Current approach**: Single-pass state machine with lookahead (checking `self.index + 1`).

**format-co challenge**: ParseEvent streaming assumes a mostly linear structure. Args require:
- Peeking ahead for values after flags
- Backtracking for `--` handling
- State changes (positional_only mode)

### 2. Help Generation Before Parsing

**Problem**: Help must be generated without parsing arguments.

```bash
prog --help
```

This needs to:
1. Detect `--help` flag immediately
2. Generate help text from schema
3. Return without parsing other args

**Current approach**: Check first arg, generate help, return error.

**format-co challenge**: Evidence collection assumes we're trying to deserialize. Help generation is a "meta" operation that needs schema access but no parsing.

**Possible solution**: Separate "probe" mode that generates evidence without consuming input.

### 3. Span Tracking for Error Messages

**Problem**: CLI errors need precise location information.

```bash
prog --unknown-flag value
     ^^^^^^^^^^^^^ unknown flag
```

**Current approach**: Track `arg_indices` to map arguments to positions in flattened string.

```rust
struct Context {
    arg_indices: Vec<usize>,    // Start position of each arg
    flattened_args: String,     // "prog --unknown-flag value"
}
```

**format-co challenge**: ParseEvent doesn't carry span information by default. Would need to extend:

```rust
enum ParseEvent<'de> {
    FieldKey(Cow<'de, str>, FieldLocationHint, Option<Span>),
    Scalar(ScalarValue<'de>, Option<Span>),
    // ...
}
```

### 4. Flag Chaining and Abbreviations

**Problem**: `-abc` can mean:
- Three bool flags: `-a -b -c`
- Flag with value: `-a bc`
- Depends on field types!

**Current approach**: Recursive function with lookahead and type checking.

```rust
fn process_short_flag(...) {
    if field.is_bool() {
        set_bool();
        if rest.len() > 0 {
            process_short_flag(rest); // Recurse!
        }
    } else {
        set_field_with_value(rest); // Consume rest as value
    }
}
```

**format-co challenge**: Event streaming doesn't naturally support this kind of recursive multi-interpretation. Would need to:
1. Probe ahead to determine field types
2. Split or don't split based on types
3. Generate appropriate events

### 5. Performance and Startup Time

**Problem**: CLI tools need to start instantly. Extra layers hurt.

**Current approach**: Direct parsing with minimal allocations.

**format-co challenge**: Event streaming adds overhead:
- Allocate ParseEvent objects
- Buffer events for probing
- Two-phase parsing (evidence + deserialize)

**Mitigation**: Lazy probing - only collect evidence when needed (untagged enums, subcommands).

---

## Where format-co DOES Add Value

### 1. Subcommand Disambiguation

**Current code** (lines 722-771): Manual variant lookup with three strategies.

**With format-co**: Solver handles variant selection automatically:

```rust
// Input: "build"
// Enum variants: Build, Test, Run, Clean

let evidence = [
    FieldEvidence { name: "build", location: Argument, type_hint: Some(String) }
];

// Solver tries each variant:
// - Build: name matches "build" (kebab-case) ✅
// - Test: name doesn't match ❌
// - Run: name doesn't match ❌
// - Clean: name doesn't match ❌

// Result: Select Build variant
```

**Benefit**: Eliminates manual `find_variant_by_name` logic, supports rename attributes automatically.

### 2. Unified Format Abstraction

**Current situation**: facet-args uses custom parsing logic, different from JSON/XML/etc.

**With format-co**: Same ParseEvent model across all formats.

```rust
// Deserialize from args
let config: Config = facet_format_co::from_args(&["--port", "8080"])?;

// Deserialize from JSON
let config: Config = facet_format_co::from_json(r#"{"port": 8080}"#)?;

// Deserialize from env vars
let config: Config = facet_format_co::from_env()?;
```

**Benefit**: Consistent API, shared deserializer logic, easier testing.

### 3. Evidence-Based Optional Subcommands

**Current code** (lines 587-602): Complex logic for `Option<Enum>` subcommands.

**With format-co**: Probe first, then decide.

```rust
#[derive(Facet)]
struct Args {
    #[facet(args::subcommand)]
    command: Option<Command>, // Optional!
}

// Input: "--verbose" (no subcommand)
// Probe: No evidence for subcommand variants
// Result: Set to None, parse other flags

// Input: "build --release" (with subcommand)
// Probe: Evidence for "build"
// Result: Set to Some(Command::Build), parse variant fields
```

**Benefit**: Cleaner logic, solver handles the decision.

### 4. Testing and Debugging

**Current situation**: Hard to unit test parsing logic in isolation.

**With format-co**: Test parser independently from deserializer.

```rust
#[test]
fn test_args_parser_events() {
    let mut parser = ArgsParser::new(&["--verbose", "-j", "4", "file.txt"]);

    assert_eq!(parser.next_event(), Ok(ParseEvent::StructStart));
    assert_eq!(parser.next_event(), Ok(ParseEvent::FieldKey("verbose", KeyValue)));
    assert_eq!(parser.next_event(), Ok(ParseEvent::Scalar(Bool(true))));
    // ...
}
```

**Benefit**: Better test coverage, easier to debug event generation vs. deserialization.

---

## Proposed Hybrid Approach

### Architecture

```
CLI args → ArgsParser → ParseEvent stream → FormatDeserializer → Value
              ↓
         Evidence collection
              ↓
           Solver
```

### ArgsParser Implementation

```rust
pub struct ArgsParser<'input> {
    args: &'input [&'input str],
    index: usize,
    positional_only: bool,

    // Schema information for disambiguation
    shape: &'static Shape,

    // Event buffer for probing
    event_buffer: Vec<ParseEvent<'input>>,
}

impl<'input> FormatParser<'input> for ArgsParser<'input> {
    type Error = ArgsError;
    type Probe<'a> = ArgsProbe<'input> where Self: 'a;

    fn next_event(&mut self) -> Result<ParseEvent<'input>, Self::Error> {
        // Emit buffered events first
        if let Some(event) = self.event_buffer.pop() {
            return Ok(event);
        }

        // Parse next argument
        if self.index >= self.args.len() {
            return Ok(ParseEvent::StructEnd);
        }

        let arg = self.args[self.index];
        let arg_type = ArgType::parse(arg);

        match arg_type {
            ArgType::LongFlag(flag) => {
                // Check for `--key=value` syntax
                if let Some((key, value)) = flag.split_once('=') {
                    self.index += 1;
                    self.event_buffer.push(ParseEvent::Scalar(
                        ScalarValue::Str(Cow::Borrowed(value))
                    ));
                    Ok(ParseEvent::FieldKey(
                        Cow::Borrowed(key),
                        FieldLocationHint::KeyValue
                    ))
                } else {
                    // Look ahead for value
                    self.index += 1;
                    Ok(ParseEvent::FieldKey(
                        Cow::Borrowed(flag),
                        FieldLocationHint::KeyValue
                    ))
                }
            }
            ArgType::ShortFlag(flag) => {
                // Handle chaining, attached values, etc.
                self.handle_short_flag(flag)
            }
            ArgType::Positional => {
                self.index += 1;
                // Is this a subcommand or positional arg?
                // Need schema to decide!
                self.classify_positional(arg)
            }
            ArgType::DoubleDash => {
                self.positional_only = true;
                self.index += 1;
                self.next_event() // Recurse to get next real event
            }
            ArgType::None => Ok(ParseEvent::StructEnd),
        }
    }

    fn peek_event(&mut self) -> Result<ParseEvent<'input>, Self::Error> {
        // Save state
        let saved_index = self.index;
        let saved_buffer_len = self.event_buffer.len();

        // Get next event
        let event = self.next_event()?;

        // Restore state
        self.index = saved_index;
        self.event_buffer.truncate(saved_buffer_len);

        Ok(event)
    }

    fn skip_value(&mut self) -> Result<(), Self::Error> {
        // Skip current field and its value
        self.index += 1;
        Ok(())
    }

    fn begin_probe(&mut self) -> Result<Self::Probe<'_>, Self::Error> {
        // Scan all arguments without consuming
        let saved_index = self.index;
        let mut evidence = Vec::new();

        while self.index < self.args.len() {
            let arg = self.args[self.index];
            let arg_type = ArgType::parse(arg);

            match arg_type {
                ArgType::LongFlag(flag) => {
                    let (key, _) = flag.split_once('=').unwrap_or((flag, ""));
                    evidence.push(FieldEvidence::new(
                        key,
                        FieldLocationHint::KeyValue,
                        Some(ValueTypeHint::String)
                    ));
                    self.index += 1;
                }
                ArgType::ShortFlag(flag) => {
                    // Resolve short flag to field name via schema
                    if let Some(field_name) = self.resolve_short_flag(flag) {
                        evidence.push(FieldEvidence::new(
                            field_name,
                            FieldLocationHint::KeyValue,
                            Some(ValueTypeHint::Bool) // Usually bool
                        ));
                    }
                    self.index += 1;
                }
                ArgType::Positional => {
                    // Could be subcommand or positional arg
                    evidence.push(FieldEvidence::new(
                        arg,
                        FieldLocationHint::Argument,
                        Some(ValueTypeHint::String)
                    ));
                    self.index += 1;
                }
                ArgType::DoubleDash => {
                    self.positional_only = true;
                    self.index += 1;
                }
                ArgType::None => break,
            }
        }

        // Restore index
        self.index = saved_index;

        Ok(ArgsProbe { evidence, idx: 0 })
    }
}
```

### Keep Specialized Features

**Help generation**:
```rust
pub fn generate_help<T: Facet<'static>>() -> String {
    generate_help_for_shape(T::SHAPE, &HelpConfig::default())
}
```

This stays **outside** the format-co pipeline. It's a schema operation, not a parsing operation.

**Shell completions**:
```rust
pub fn generate_completions<T: Facet<'static>>(shell: Shell) -> String {
    generate_completions_for_shape(T::SHAPE, shell)
}
```

Also stays outside. Uses schema to generate shell scripts.

**Error messages with spans**:
```rust
pub struct ArgsError {
    kind: ArgsErrorKind,
    span: Span,  // Position in flattened args
}

impl Display for ArgsError {
    fn fmt(&self, f: &mut Formatter) -> Result {
        // Pretty-print with caret pointing to error location
        write!(f, "Error at position {}: {}", self.span, self.kind)
    }
}
```

Extend ParseEvent to carry spans:
```rust
pub struct SpannedParseEvent<'de> {
    event: ParseEvent<'de>,
    span: Span,
}
```

---

## Benefits of Hybrid Approach

### 1. Consistent Abstraction

**Before**:
- facet-json: Custom parsing logic
- facet-xml: Custom parsing logic
- facet-args: Completely different custom logic

**After**:
- All formats: `impl FormatParser`
- Shared deserializer logic
- Consistent error handling

### 2. Easier Testing

**Parser tests**:
```rust
#[test]
fn test_parse_long_flag_with_value() {
    let mut parser = ArgsParser::new(&["--output=file.txt"]);
    assert_eq!(parser.next_event(), ParseEvent::StructStart);
    assert_eq!(parser.next_event(), ParseEvent::FieldKey("output", KeyValue));
    assert_eq!(parser.next_event(), ParseEvent::Scalar(Str("file.txt")));
}
```

**Deserializer tests**:
```rust
#[test]
fn test_deserialize_args() {
    #[derive(Facet)]
    struct Args {
        output: String,
    }

    let args: Args = from_args(&["--output=file.txt"]).unwrap();
    assert_eq!(args.output, "file.txt");
}
```

Separation of concerns!

### 3. Reusable Solver Logic

**Subcommand selection** uses the same solver as:
- Untagged enum variants in JSON
- XML element name disambiguation
- KDL node type inference

### 4. Future Extensions

**Environment variables**:
```rust
// Similar parsing logic, different input source
let mut parser = EnvParser::new(std::env::vars());
let config: Config = FormatDeserializer::new(&mut parser).deserialize()?;
```

**Config files with overrides**:
```rust
// Layer multiple sources
let base: Config = from_json("config.json")?;
let overrides: Config = from_args(&std::env::args())?;
let final_config = base.merge(overrides);
```

---

## Implementation Roadmap

### Phase 1: ArgsParser (format-co integration)

Create `facet-format-co-args`:

```rust
pub struct ArgsParser<'input> {
    args: &'input [&'input str],
    index: usize,
    positional_only: bool,
    shape: &'static Shape,
}

impl<'input> FormatParser<'input> for ArgsParser<'input> {
    // Implement next_event, peek_event, skip_value, begin_probe
}
```

Focus on core parsing logic, emit ParseEvent stream.

### Phase 2: Maintain Specialized Features

Keep in `facet-args`:
- `generate_help` and `generate_help_for_shape`
- `generate_completions` and `generate_completions_for_shape`
- `ArgsError` with span tracking
- Custom attributes (`args::positional`, `args::short`, etc.)

### Phase 3: Gradual Migration

**New API** (opt-in):
```rust
use facet_args::from_args_codex;

let args: MyArgs = from_args_codex(&["--verbose", "file.txt"])?;
```

**Old API** (unchanged):
```rust
use facet_args::from_slice;

let args: MyArgs = from_slice(&["--verbose", "file.txt"])?;
```

Run both in parallel, compare results, ensure compatibility.

### Phase 4: Deprecation

Once codex approach is stable and tested:
- Deprecate old `from_slice`
- Make `from_args_codex` the default
- Remove old implementation in next major version

---

## Evidence Quality Assessment

### Revised Assessment: **Medium** ⚠️

**What Evidence Provides**:

1. **Field names**: ✅ Present (flag names, positional args)
2. **Field types**: ⚠️ Must infer from schema (bool flags, etc.)
3. **Subcommand names**: ✅ Clear from positional args
4. **Flag presence**: ✅ Bool flags present = true
5. **Values**: ✅ Strings that need parsing

**Evidence Quality by Scenario**:

| Scenario | Evidence Quality | Solver Benefit |
|----------|------------------|----------------|
| Subcommand selection | High | ✅ Strong - variant name matching |
| Optional subcommands | High | ✅ Strong - presence/absence clear |
| Flag disambiguation | Medium | ⚠️ Moderate - need schema for type info |
| Positional arg types | Low | 🔴 Weak - everything is string |
| Flag chaining | Low | 🔴 Weak - requires schema + lookahead |

**Overall**: Evidence helps with high-level structure (which subcommand?) but less with low-level details (is `-abc` three flags or one flag with value?).

---

## Comparison: Current vs format-co Approach

### Lines of Code

**Current facet-args/src/format.rs**: ~980 lines
- Argument parsing: ~400 lines
- Field handling: ~200 lines
- Subcommand handling: ~150 lines
- Finalization: ~100 lines
- Helpers: ~130 lines

**Estimated format-co version**:
- ArgsParser (ParseEvent generation): ~300 lines
- Evidence collection: ~100 lines
- Specialized features (help, completions): ~200 lines (unchanged)
- FormatDeserializer: ~0 lines (shared!)

**Total reduction**: ~380 lines eliminated through shared deserializer logic.

### Performance

**Current**: Single-pass, minimal allocations
**format-co**: Two-phase (evidence + deserialize) when needed

**Optimization**: Lazy probing
- Only probe for untagged enums and subcommands
- Skip probing for simple structs with known fields
- Benchmark to measure overhead

---

## Conclusion

### Should facet-args Use format-co?

**Answer**: **Yes, but with a hybrid approach.**

### Why Hybrid?

1. **Keep what works**: Help generation, completions, error messages are facet-args-specific and should stay.
2. **Share what's common**: Deserializer logic, variant selection, field mapping can use format-co.
3. **Minimize disruption**: Gradual migration preserves compatibility.
4. **Maximize reuse**: Solver logic shared across all formats.

### Primary Benefits

1. ✅ **Unified abstraction**: Same ParseEvent model as JSON/XML/KDL
2. ✅ **Subcommand disambiguation**: Solver handles variant selection
3. ✅ **Testing**: Parser and deserializer can be tested independently
4. ✅ **Code reduction**: ~380 lines eliminated through sharing

### Primary Challenges

1. ⚠️ **Span tracking**: Need to extend ParseEvent with span info
2. ⚠️ **Performance**: Two-phase parsing adds overhead (mitigate with lazy probing)
3. ⚠️ **Complexity**: More abstraction layers to understand
4. ⚠️ **Flag chaining**: Recursive logic doesn't fit pure event streaming

### Recommendation

**Implement facet-format-co-args** as a **new optional backend** for facet-args:
- Prove the concept with tests and benchmarks
- Maintain backward compatibility
- Migrate gradually based on real-world feedback

Priority: **Medium** - Good conceptual fit, but existing implementation works well. Do this after XML, KDL, and urlencoded.

---

## Open Questions

1. **How to handle span information in ParseEvent?**
   - Extend ParseEvent with optional Span field?
   - Use a wrapper type `Spanned<ParseEvent>`?
   - Store spans separately in parser state?

2. **How to optimize probing performance?**
   - Cache evidence collection results?
   - Only probe when solver needs it?
   - Stream-based probing (don't buffer all args)?

3. **How to handle flag chaining in event model?**
   - Emit multiple FieldKey events for `-abc`?
   - Emit single event with metadata about chaining?
   - Let deserializer handle expansion?

4. **Should help/completions use evidence collection?**
   - Could help be generated from evidence schema?
   - Could completions probe available fields?
   - Or keep them as pure schema operations?

---

## References

- Current implementation: `facet-args/src/format.rs`
- Help generation: `facet-args/src/help.rs`
- Completions: `facet-args/src/completions.rs`
- Tests: `facet-args/tests/`
