# URL-Encoded Format: Deep Dive for format-co

## Executive Summary

URL-encoded (application/x-www-form-urlencoded) is **more powerful than initially assessed**. While fundamentally text-based and flat, the widespread adoption of bracket notation conventions enables:

1. **Nested structures** via `field[nested]=value`
2. **Arrays** via `field[]=value1&field[]=value2`
3. **Deep nesting** via `field[a][b][c]=value`
4. **Mixed flat and nested** data in the same payload

**Revised Assessment**: ✅ **Good Fit** (upgraded from ⚠️ Marginal)

The format-co approach provides **significant value** for URL-encoded data by enabling:
- Evidence-based untagged enum disambiguation
- Natural handling of nested structures
- Consistent ParseEvent stream for complex forms

---

## Format Overview

### Basic Syntax

```
key1=value1&key2=value2&key3=value3
```

Features:
- `&` separates key-value pairs
- `=` separates key from value
- Values are percent-encoded (e.g., space → `%20` or `+`)
- Everything is transmitted as text

### Extended Syntax (Bracket Notation)

The bracket notation conventions originated from PHP and were popularized by Rails/Rack. Now widely supported across web frameworks:

#### 1. Nested Objects
```
user[name]=Alice&user[age]=30
```

Represents:
```json
{
  "user": {
    "name": "Alice",
    "age": "30"
  }
}
```

#### 2. Arrays (Empty Brackets)
```
tags[]=rust&tags[]=web&tags[]=json
```

Represents:
```json
{
  "tags": ["rust", "web", "json"]
}
```

According to the [Rails convention](https://guides.rubyonrails.org/form_helpers.html), "to send an array of values, append an empty pair of square brackets [] to the key name."

#### 3. Indexed Arrays
```
items[0]=apple&items[1]=banana&items[2]=cherry
```

Represents:
```json
{
  "items": ["apple", "banana", "cherry"]
}
```

#### 4. Deep Nesting
```
order[user][address][street]=123+Main+St&order[user][address][city]=NYC
```

Represents:
```json
{
  "order": {
    "user": {
      "address": {
        "street": "123 Main St",
        "city": "NYC"
      }
    }
  }
}
```

#### 5. Arrays of Objects
```
users[0][name]=Alice&users[0][age]=30&users[1][name]=Bob&users[1][age]=25
```

Represents:
```json
{
  "users": [
    {"name": "Alice", "age": "30"},
    {"name": "Bob", "age": "25"}
  ]
}
```

---

## Current facet-urlencoded Implementation

### What's Supported ✅

Looking at `facet-urlencoded/src/lib.rs`:

1. **Nested objects** via bracket notation (lines 171-194)
   ```rust
   // Parses: user[name]=Alice
   if let Some(open_bracket) = key.find('[')
       && let Some(close_bracket) = key.find(']')
   {
       let parent_key = &key[0..open_bracket];
       let nested_key = &key[(open_bracket + 1)..close_bracket];
       // Creates nested structure
   }
   ```

2. **Deep nesting** recursively handled (lines 189-193)
   ```rust
   // Handle deeply nested case like user[address][city]=value
   let new_key = format!("{nested_key}{remainder}");
   nested.insert(&new_key, value);
   ```

3. **Arbitrary depth** - Test shows 5 levels deep work (lines 170-216 of tests.rs)
   ```
   very[very][deeply][nested][field]=value
   ```

### What's NOT Supported 🔴

1. **Arrays** - No handling for `field[]=value` syntax
2. **Indexed arrays** - No handling for `field[0]=value`
3. **Arrays of objects** - No `items[0][name]=Alice` support
4. **Duplicate keys** - Last-wins or array behavior undefined
5. **Type hints** - Everything parsed as string, no bool/number detection

### Current Architecture

The implementation uses a two-phase approach:

**Phase 1: Parse into nested structure** (lines 128-135)
```rust
let mut nested_values = NestedValues::new();
for (key, value) in pairs {
    nested_values.insert(&key, value.to_string());
}
```

**Phase 2: Deserialize using reflection** (lines 221-264)
```rust
fn deserialize_value<'mem>(
    mut wip: Partial<'mem>,
    values: &NestedValues,
) -> Result<Partial<'mem>, UrlEncodedError>
```

The `NestedValues` struct (lines 156-218):
```rust
struct NestedValues {
    flat: HashMap<String, String>,      // Simple key-value pairs
    nested: HashMap<String, NestedValues>,  // Nested structures
}
```

**Problem**: No support for sequences/arrays - only maps!

---

## format-co Mapping Strategy

### ParseEvent Stream Examples

#### Example 1: Flat Form
```
name=Alice&age=30&email=alice@example.com
```

ParseEvent stream:
```
StructStart
  FieldKey("name", KeyValue)
    Scalar(Str("Alice"))
  FieldKey("age", KeyValue)
    Scalar(Str("30"))       // Note: string! Solver must try parsing
  FieldKey("email", KeyValue)
    Scalar(Str("alice@example.com"))
StructEnd
```

#### Example 2: Nested Object
```
user[name]=Alice&user[age]=30&user[active]=true
```

ParseEvent stream:
```
StructStart
  FieldKey("user", KeyValue)
    StructStart
      FieldKey("name", KeyValue)
        Scalar(Str("Alice"))
      FieldKey("age", KeyValue)
        Scalar(Str("30"))
      FieldKey("active", KeyValue)
        Scalar(Str("true"))  // String "true", not bool
    StructEnd
StructEnd
```

#### Example 3: Array (Empty Bracket Notation)
```
tags[]=rust&tags[]=web&tags[]=api
```

ParseEvent stream:
```
StructStart
  FieldKey("tags", KeyValue)
    SequenceStart
      Scalar(Str("rust"))
      Scalar(Str("web"))
      Scalar(Str("api"))
    SequenceEnd
StructEnd
```

#### Example 4: Indexed Array
```
colors[0]=red&colors[1]=green&colors[2]=blue
```

ParseEvent stream:
```
StructStart
  FieldKey("colors", KeyValue)
    SequenceStart
      Scalar(Str("red"))
      Scalar(Str("green"))
      Scalar(Str("blue"))
    SequenceEnd
StructEnd
```

#### Example 5: Array of Objects
```
users[0][name]=Alice&users[0][age]=30&users[1][name]=Bob&users[1][age]=25
```

ParseEvent stream:
```
StructStart
  FieldKey("users", KeyValue)
    SequenceStart
      StructStart
        FieldKey("name", KeyValue)
          Scalar(Str("Alice"))
        FieldKey("age", KeyValue)
          Scalar(Str("30"))
      StructEnd
      StructStart
        FieldKey("name", KeyValue)
          Scalar(Str("Bob"))
        FieldKey("age", KeyValue)
          Scalar(Str("25"))
      StructEnd
    SequenceEnd
StructEnd
```

#### Example 6: Mixed Structure
```
product_id=ABC123&quantity=2&user[name]=Alice&user[tags][]=rust&user[tags][]=web
```

ParseEvent stream:
```
StructStart
  FieldKey("product_id", KeyValue)
    Scalar(Str("ABC123"))
  FieldKey("quantity", KeyValue)
    Scalar(Str("2"))
  FieldKey("user", KeyValue)
    StructStart
      FieldKey("name", KeyValue)
        Scalar(Str("Alice"))
      FieldKey("tags", KeyValue)
        SequenceStart
          Scalar(Str("rust"))
          Scalar(Str("web"))
        SequenceEnd
    StructEnd
StructEnd
```

---

## Evidence Collection Strategy

### Key Insight: Two-Phase Parsing Required

URL-encoded data **cannot be streamed** in the traditional sense. The structure is only known after parsing ALL key-value pairs because:

1. **Duplicate keys determine type**: Is `tag=a&tag=b` an array or last-wins?
2. **Indices determine array length**: `arr[5]=x` means array has at least 6 elements
3. **Nested structure emerges from keys**: `a[b][c]=1` vs `a[d]=2` requires seeing all keys

**Solution**: Parse entire input first, then generate ParseEvent stream.

### Evidence Collection Algorithm

**Step 1: Parse all keys into structural tree**
```rust
struct ParsedStructure {
    scalars: HashMap<String, String>,
    objects: HashMap<String, ParsedStructure>,
    arrays: HashMap<String, Vec<ParsedValue>>,
}

enum ParsedValue {
    Scalar(String),
    Object(ParsedStructure),
}
```

**Step 2: Analyze structure to generate evidence**
```rust
fn collect_evidence<'de>(
    structure: &ParsedStructure
) -> Vec<FieldEvidence<'de>> {
    let mut evidence = Vec::new();

    // Scalar fields
    for (key, value) in &structure.scalars {
        let type_hint = infer_type_hint(value);
        evidence.push(FieldEvidence {
            name: key,
            location: KeyValue,
            type_hint: Some(type_hint),
        });
    }

    // Object fields
    for key in structure.objects.keys() {
        evidence.push(FieldEvidence {
            name: key,
            location: KeyValue,
            type_hint: Some(ValueTypeHint::Map),
        });
    }

    // Array fields
    for key in structure.arrays.keys() {
        evidence.push(FieldEvidence {
            name: key,
            location: KeyValue,
            type_hint: Some(ValueTypeHint::Sequence),
        });
    }

    evidence
}
```

**Step 3: Type inference heuristics**
```rust
fn infer_type_hint(value: &str) -> ValueTypeHint {
    // Try bool
    if value == "true" || value == "false" {
        return ValueTypeHint::Bool;
    }

    // Try integer
    if value.parse::<i64>().is_ok() {
        return ValueTypeHint::Number;
    }

    // Try float
    if value.parse::<f64>().is_ok() {
        return ValueTypeHint::Number;
    }

    // Default to string
    ValueTypeHint::String
}
```

### Evidence Examples

#### Example 1: Untagged Enum Disambiguation

Given this Rust type:
```rust
#[derive(Facet)]
enum Action {
    #[facet(untagged)]
    Login { username: String, password: String },
    #[facet(untagged)]
    Logout { session_id: String },
    #[facet(untagged)]
    RefreshToken { refresh_token: String },
}
```

Input: `username=alice&password=secret123`

Evidence collected:
```rust
[
  FieldEvidence { name: "username", location: KeyValue, type_hint: Some(String) },
  FieldEvidence { name: "password", location: KeyValue, type_hint: Some(String) }
]
```

Solver queries each variant:
- `Login`: requires `username` ✅ and `password` ✅ → **Match!**
- `Logout`: requires `session_id` ❌ → No match
- `RefreshToken`: requires `refresh_token` ❌ → No match

Result: Deserialize as `Action::Login`

#### Example 2: Nested Untagged Enum

```rust
#[derive(Facet)]
struct Order {
    product: String,
    payment: PaymentMethod,
}

#[derive(Facet)]
enum PaymentMethod {
    #[facet(untagged)]
    CreditCard { number: String, cvv: String },
    #[facet(untagged)]
    PayPal { email: String },
    #[facet(untagged)]
    BankTransfer { account: String, routing: String },
}
```

Input: `product=Widget&payment[email]=alice@example.com`

Top-level evidence:
```rust
[
  FieldEvidence { name: "product", location: KeyValue, type_hint: Some(String) },
  FieldEvidence { name: "payment", location: KeyValue, type_hint: Some(Map) }
]
```

When deserializing `payment` field, probe yields:
```rust
[
  FieldEvidence { name: "email", location: KeyValue, type_hint: Some(String) }
]
```

Solver determines: `PaymentMethod::PayPal`

---

## Key Challenges for format-co

### 1. Type Ambiguity (Everything is Text)

**Challenge**: All values are strings. The type must be inferred.

```
age=30          // "30" as string, but Rust type is u32
active=true     // "true" as string, but Rust type is bool
price=19.99     // "19.99" as string, but Rust type is f64
```

**Solution**:
- Provide `ValueTypeHint` based on parsing attempt
- Let deserializer try multiple parses (string → number → bool)
- Leverage Rust type information to guide parsing

**format-co Advantage**: The solver can try different interpretations when disambiguating untagged enums.

### 2. Array Syntax Variations

**Challenge**: Multiple conventions exist for arrays:

```
tags[]=a&tags[]=b           // Empty bracket (most common)
tags[0]=a&tags[1]=b         // Indexed
tags=a&tags=b               // Duplicate keys (some frameworks)
```

**Solution**:
- Parse all three styles
- Empty brackets: `field[]` → array indicator
- Indexed: `field[N]` where N is number → array with index
- Duplicate keys: configurable behavior (array or last-wins)

**format-co Advantage**: ParseEvent abstraction hides the syntax variation.

### 3. Sparse Arrays

**Challenge**: What if indices have gaps?

```
items[0]=a&items[2]=c&items[5]=f
```

Options:
- Reject as error
- Fill gaps with None/null
- Compact array (ignore indices, use order)

**Recommendation**: **Compact array** approach matches most framework behavior.

### 4. Mixed Notation Ambiguity

**Challenge**: What does this mean?

```
field[0]=value1&field[]=value2
```

Is it:
- Invalid (mixing indexed and empty bracket)?
- Array with two elements?
- Error case?

**Recommendation**: **Treat as error** - mixed notation is ambiguous and rare.

### 5. Duplicate Keys (Non-Array)

**Challenge**: What does this mean?

```
name=Alice&name=Bob
```

Options:
- Last-wins (Bob)
- First-wins (Alice)
- Array (["Alice", "Bob"])
- Error

**Recommendation**: **Configurable behavior**:
- Default: last-wins (matches HTML form behavior)
- Array mode: collect duplicates
- Strict mode: error on duplicates

### 6. Empty Values

**Challenge**: What does this represent?

```
field=&name=Alice
```

Is `field` an empty string or None/null?

**Recommendation**:
- Empty string `""` by default
- Use `Option<T>` in Rust type for None behavior
- Missing key = None, present but empty = Some("")

### 7. Nested Array Notation

**Challenge**: Arrays inside objects:

```
user[tags][]=rust&user[tags][]=web
```

**Solution**: Parse as:
```
user → object
  tags → array
    [0] → "rust"
    [1] → "web"
```

This works naturally with recursive parsing.

---

## Comparison: Current vs format-co Approach

### Current facet-urlencoded

**Architecture**:
```
URL string → NestedValues tree → Reflection-based deserialize
```

**Strengths**:
- Simple, direct
- Works for flat and nested objects
- Good error messages for missing fields

**Limitations**:
- No array support (critical missing feature!)
- No untagged enum support
- Tightly coupled to reflection API
- Can't reuse parser for different serialization systems

### Proposed format-co Approach

**Architecture**:
```
URL string → ParsedStructure → ParseEvent stream → Deserializer → Value
                                        ↓
                                 Evidence collection → Solver
```

**Advantages**:
1. ✅ **Array support** - Handles `field[]`, `field[N]`, duplicates
2. ✅ **Untagged enums** - Evidence-based solver disambiguation
3. ✅ **Consistent abstraction** - Same ParseEvent model as JSON/XML/KDL
4. ✅ **Reusable parser** - Can plug into different deserializers
5. ✅ **Type hints** - Infer bool/number from string values
6. ✅ **Probing** - Can peek at structure before committing to parse

**Trade-offs**:
- More complex implementation
- Two-phase parsing (can't stream)
- Additional memory for ParsedStructure

**Verdict**: The advantages strongly outweigh the complexity cost, especially for:
- Applications using untagged enums
- Complex forms with arrays
- API endpoints accepting flexible input

---

## Implementation Roadmap

### Phase 1: Enhanced Parser (format-co-urlencoded)

Create `facet-format-co-urlencoded` with:

```rust
pub struct UrlEncodedParser {
    structure: ParsedStructure,
    position: Position,
}

impl FormatParser for UrlEncodedParser {
    type Error = UrlEncodedError;
    type Probe<'a> = UrlEncodedProbe where Self: 'a;

    fn next_event(&mut self) -> Result<ParseEvent, Self::Error>;
    fn peek_event(&mut self) -> Result<ParseEvent, Self::Error>;
    fn skip_value(&mut self) -> Result<(), Self::Error>;
    fn begin_probe(&mut self) -> Result<Self::Probe<'_>, Self::Error>;
}
```

**Key features**:
1. Parse all three array notations: `[]`, `[N]`, duplicate keys
2. Recursive nested structure handling
3. Type inference for ValueTypeHint
4. Evidence collection for probing

### Phase 2: Integration with FormatDeserializer

Wire up the parser:
```rust
let input = "user[name]=Alice&user[tags][]=rust&user[tags][]=web";
let mut parser = UrlEncodedParser::new(input)?;
let deserializer = FormatDeserializer::new(&mut parser);
let value: User = deserializer.deserialize()?;
```

### Phase 3: Migration Path

Keep `facet-urlencoded` for backward compatibility:
- Stable API for existing users
- No breaking changes

Add opt-in for format-co:
```rust
// Old API (unchanged)
let value: User = facet_urlencoded::from_str(input)?;

// New API (opt-in for advanced features)
let value: User = facet_urlencoded::from_str_codex(input)?;
```

When mature, deprecate old API and make codex default.

---

## Evidence Quality Assessment

### Revised Assessment: **Medium-High** ✅

**What Evidence Provides**:

1. **Field names**: ✅ Always present (key names)
2. **Field presence**: ✅ All fields known upfront (after parsing)
3. **Nesting structure**: ✅ Clear from bracket notation
4. **Array vs Object**: ✅ Detectable from `[]` or indexed `[N]`
5. **Type hints**: ⚠️ Inferred from string values (not perfect but useful)

**Evidence Quality by Scenario**:

| Scenario | Evidence Quality | Solver Benefit |
|----------|------------------|----------------|
| Untagged enums (flat) | High | ✅ Strong - field names disambiguate variants |
| Untagged enums (nested) | High | ✅ Strong - nested field names work too |
| Flatten support | Medium | ⚠️ Moderate - all fields at one level, hard to separate sources |
| Type disambiguation | Low | 🔴 Weak - everything is text, limited type info |
| Array detection | High | ✅ Strong - bracket notation explicit |

**Overall**: Evidence collection for URL-encoded is **significantly better than initially assessed**. The bracket notation provides rich structural information that the solver can leverage effectively.

---

## Performance Considerations

### Memory Usage

**Two-phase parsing requires storing entire structure**:
```rust
// Phase 1: Parse everything
let structure = parse_to_structure(input)?;  // Memory allocation

// Phase 2: Generate events
let mut parser = UrlEncodedParser::new(structure);
```

**Memory cost**: O(n) where n = input size
- Acceptable for typical form sizes (< 1MB)
- Large file uploads should use multipart, not urlencoded

### Parsing Cost

**Current approach**: O(n) single pass
**format-co approach**: O(n) + O(m) where m = number of fields
- Parse input: O(n)
- Build structure tree: O(m)
- Generate events: O(m)

**Total**: Still O(n + m), reasonable overhead

### Optimization Opportunities

1. **Lazy evidence collection**: Only probe when solver needs it
2. **Incremental parsing**: If input structure known upfront (schema), skip full parse
3. **String interning**: Reuse common field names across requests

---

## Real-World Examples

### Example 1: Login Form

Input:
```
username=alice@example.com&password=secret123&remember_me=true
```

Rust types:
```rust
#[derive(Facet)]
struct LoginForm {
    username: String,
    password: String,
    remember_me: bool,
}
```

Evidence collected:
```rust
[
  FieldEvidence { name: "username", location: KeyValue, type_hint: Some(String) },
  FieldEvidence { name: "password", location: KeyValue, type_hint: Some(String) },
  FieldEvidence { name: "remember_me", location: KeyValue, type_hint: Some(Bool) }
]
```

**Benefit**: Type hint for `remember_me` guides bool parsing.

### Example 2: Search Form with Filters

Input:
```
q=rust+web&categories[]=programming&categories[]=tutorial&sort=date&order=desc
```

Rust types:
```rust
#[derive(Facet)]
struct SearchQuery {
    q: String,
    categories: Vec<String>,
    sort: String,
    order: String,
}
```

Evidence collected:
```rust
[
  FieldEvidence { name: "q", location: KeyValue, type_hint: Some(String) },
  FieldEvidence { name: "categories", location: KeyValue, type_hint: Some(Sequence) },
  FieldEvidence { name: "sort", location: KeyValue, type_hint: Some(String) },
  FieldEvidence { name: "order", location: KeyValue, type_hint: Some(String) }
]
```

**Benefit**: Array detection automatic from `[]` syntax.

### Example 3: E-Commerce Checkout

Input:
```
cart[items][0][product_id]=ABC123&cart[items][0][quantity]=2&
cart[items][1][product_id]=XYZ789&cart[items][1][quantity]=1&
shipping[name]=Alice+Doe&shipping[address][street]=123+Main+St&
shipping[address][city]=NYC&shipping[address][zip]=10001&
payment_method=credit_card&payment[card_number]=4111111111111111
```

Rust types:
```rust
#[derive(Facet)]
struct CheckoutForm {
    cart: Cart,
    shipping: ShippingInfo,
    payment_method: String,
    payment: PaymentDetails,
}

#[derive(Facet)]
struct Cart {
    items: Vec<CartItem>,
}

#[derive(Facet)]
struct CartItem {
    product_id: String,
    quantity: u32,
}

#[derive(Facet)]
struct ShippingInfo {
    name: String,
    address: Address,
}

#[derive(Facet)]
struct Address {
    street: String,
    city: String,
    zip: String,
}

#[derive(Facet)]
struct PaymentDetails {
    card_number: String,
}
```

Evidence collected (top-level):
```rust
[
  FieldEvidence { name: "cart", location: KeyValue, type_hint: Some(Map) },
  FieldEvidence { name: "shipping", location: KeyValue, type_hint: Some(Map) },
  FieldEvidence { name: "payment_method", location: KeyValue, type_hint: Some(String) },
  FieldEvidence { name: "payment", location: KeyValue, type_hint: Some(Map) }
]
```

**Benefit**: Deep nesting handled naturally through recursive evidence collection.

### Example 4: Untagged Enum API Endpoint

Input (Login):
```
action=login&username=alice&password=secret
```

Input (Create Post):
```
action=create_post&title=Hello+World&body=Post+content
```

Rust types:
```rust
#[derive(Facet)]
enum ApiAction {
    #[facet(untagged)]
    Login { username: String, password: String },
    #[facet(untagged)]
    CreatePost { title: String, body: String },
    #[facet(untagged)]
    DeletePost { post_id: u64 },
}
```

Evidence for login input:
```rust
[
  FieldEvidence { name: "action", location: KeyValue, type_hint: Some(String) },
  FieldEvidence { name: "username", location: KeyValue, type_hint: Some(String) },
  FieldEvidence { name: "password", location: KeyValue, type_hint: Some(String) }
]
```

**Solver analysis**:
- `Login` variant: requires `username` ✅, `password` ✅ → **Match!**
- `CreatePost` variant: requires `title` ❌ → No match
- `DeletePost` variant: requires `post_id` ❌ → No match

Result: Deserialize as `ApiAction::Login`

**Benefit**: No need for explicit `type` field or external tagging!

---

## Convention Support Matrix

| Convention | Syntax Example | Support in facet-urlencoded | Support in format-co | Notes |
|------------|----------------|----------------------------|---------------------|-------|
| Flat fields | `a=1&b=2` | ✅ Yes | ✅ Yes | Basic case |
| Nested objects | `a[b]=1` | ✅ Yes | ✅ Yes | Current impl works |
| Deep nesting | `a[b][c][d]=1` | ✅ Yes | ✅ Yes | Recursive parsing |
| Empty bracket arrays | `a[]=1&a[]=2` | 🔴 No | ✅ Yes | **Major gap in current impl** |
| Indexed arrays | `a[0]=1&a[1]=2` | 🔴 No | ✅ Yes | Required for complex forms |
| Duplicate keys | `a=1&a=2` | ⚠️ Last-wins | ✅ Configurable | Should be explicit |
| Arrays of objects | `a[0][b]=1&a[1][b]=2` | 🔴 No | ✅ Yes | Composition of above |
| Mixed types | `a=1&b[]=2&c[d]=3` | ⚠️ Partial | ✅ Yes | Full support needed |

**Key Takeaway**: format-co fills critical gaps in current implementation, especially around arrays.

---

## Conclusion

### Revised Format-Co Fit Assessment

**Previous**: ⚠️ Marginal - "Limited structure, existing approach sufficient"

**Revised**: ✅ **Good to Excellent** - "Rich structural information, strong solver support"

### Why the Upgrade?

1. **Bracket notation is powerful**: Enables nested objects, arrays, and mixed structures
2. **Evidence quality is high**: Field names + structure + type hints enable solver
3. **Critical gaps in current impl**: Arrays not supported, untagged enums impossible
4. **Real-world value**: Complex forms, API endpoints, untagged enums all benefit
5. **Consistent abstraction**: Same ParseEvent model as JSON/XML/KDL/YAML

### Recommendation

**Implement facet-format-co-urlencoded** as high-priority format:
- Fills critical feature gaps (array support)
- Enables untagged enum disambiguation
- Provides consistent API across formats
- Real-world use case: web forms, REST APIs, search queries

### Priority Tier

Move from **Tier 3** (Low Value) → **Tier 2** (Medium-High Value)

Rationale: URL-encoded is ubiquitous in web development, and the format-co approach enables features that are currently impossible with facet-urlencoded.

---

## References

- [Rails Form Helpers - Array Conventions](https://guides.rubyonrails.org/form_helpers.html)
- [Rails Action Controller - Parameter Parsing](https://edgeguides.rubyonrails.org/action_controller_overview.html)
- [PHP Array Query String Handling](https://www.uptimia.com/questions/how-to-pass-an-array-in-a-query-string)
- [Rack Parameter Conventions](https://github.com/node-formidable/formidable/issues/33)
- Current facet-urlencoded implementation: `facet-urlencoded/src/lib.rs`
- Test suite: `facet-urlencoded/src/tests.rs`
