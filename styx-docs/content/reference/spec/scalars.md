+++
title = "Scalar Interpretation"
weight = 4
slug = "scalars"
insert_anchor_links = "heading"
+++

How opaque scalars are interpreted as typed values during deserialization.

Styx scalars are opaque text at parse time — the parser assigns no type. Interpretation happens during deserialization, when a schema or target type requests a specific type. This page specifies the standard interpretation rules that all conforming implementations MUST follow.

## Strings

> r[interp.string]
> Any scalar MAY be interpreted as a string. The scalar's text content becomes the string value.
>
> ```styx
> name Alice        // bare scalar → "Alice"
> name "Alice"      // quoted scalar → "Alice"
> name r#"Alice"#   // raw scalar → "Alice"
> ```

## Booleans

> r[interp.bool.true]
> A scalar is interpreted as boolean `true` if its text content is exactly `true` (case-sensitive).

> r[interp.bool.false]
> A scalar is interpreted as boolean `false` if its text content is exactly `false` (case-sensitive).

> r[interp.bool.error]
> Any other scalar text MUST produce an error when boolean interpretation is requested.
>
> ```styx
> enabled true      // → true
> enabled false     // → false
> enabled yes       // ERROR: not a valid boolean
> enabled TRUE      // ERROR: case-sensitive
> ```

## Integers

> r[interp.int.decimal]
> A decimal integer is an optional sign (`+` or `-`) followed by one or more digits `0-9`.
> Leading zeros are permitted. Underscores MAY appear between digits for readability and are ignored.
>
> ```styx
> port 8080         // → 8080
> offset -42        // → -42
> big 1_000_000     // → 1000000
> ```

> r[interp.int.hex]
> A hexadecimal integer starts with `0x` or `0X` followed by one or more hex digits `0-9`, `a-f`, `A-F`.
> Underscores MAY appear between digits.
>
> ```styx
> color 0xff5500    // → 16733440
> mask 0xFF_FF      // → 65535
> ```

> r[interp.int.octal]
> An octal integer starts with `0o` or `0O` followed by one or more digits `0-7`.
> Underscores MAY appear between digits.
>
> ```styx
> mode 0o755        // → 493
> ```

> r[interp.int.binary]
> A binary integer starts with `0b` or `0B` followed by one or more digits `0-1`.
> Underscores MAY appear between digits.
>
> ```styx
> flags 0b1010      // → 10
> mask 0b1111_0000  // → 240
> ```

> r[interp.int.range]
> Implementations MUST reject integers that overflow the target type's range.
> The error message SHOULD include the valid range.

## Floating-point

> r[interp.float.syntax]
> A floating-point number consists of:
> - An optional sign (`+` or `-`)
> - An integer part (one or more digits)
> - An optional fractional part (`.` followed by one or more digits)
> - An optional exponent (`e` or `E`, optional sign, one or more digits)
>
> At least one of fractional part or exponent MUST be present to distinguish from integer.
> Underscores MAY appear between digits.
>
> ```styx
> pi 3.14159
> avogadro 6.022e23
> small 1.5e-10
> precise 3.141_592_653
> ```

> r[interp.float.special]
> The following case-sensitive literals represent special floating-point values:
> - `inf` or `+inf` — positive infinity
> - `-inf` — negative infinity
> - `nan` — not a number (quiet NaN)
>
> ```styx
> max inf
> min -inf
> undefined nan
> ```

## Durations

> r[interp.duration.syntax]
> A duration is a sequence of one or more `<number><unit>` pairs, where:
> - `<number>` is a non-negative integer or floating-point number
> - `<unit>` is one of the following (case-sensitive):
>
> | Unit | Meaning |
> |------|---------|
> | `ns` | nanoseconds |
> | `us` or `µs` | microseconds |
> | `ms` | milliseconds |
> | `s` | seconds |
> | `m` | minutes |
> | `h` | hours |
> | `d` | days (24 hours) |
>
> Multiple pairs are summed. No whitespace is allowed between pairs.
>
> ```styx
> timeout 30s           // 30 seconds
> interval 1h30m        // 1 hour + 30 minutes = 5400 seconds
> precise 1.5s          // 1500 milliseconds
> delay 500ms           // 500 milliseconds
> ttl 7d                // 7 days
> ```

> r[interp.duration.order]
> Units MAY appear in any order, but for readability SHOULD appear largest to smallest.
> The same unit MAY appear multiple times; values are summed.
>
> ```styx
> weird 30s1h           // valid: 1 hour 30 seconds (not recommended)
> also 1h1h             // valid: 2 hours (not recommended)
> ```

## Dates and times

> r[interp.datetime.iso8601]
> Date and time values follow ISO 8601 format. Implementations MUST support at minimum:
>
> | Format | Example | Meaning |
> |--------|---------|---------|
> | `YYYY-MM-DD` | `2024-03-15` | Date only |
> | `YYYY-MM-DDTHH:MM:SS` | `2024-03-15T14:30:00` | Local datetime |
> | `YYYY-MM-DDTHH:MM:SSZ` | `2024-03-15T14:30:00Z` | UTC datetime |
> | `YYYY-MM-DDTHH:MM:SS±HH:MM` | `2024-03-15T14:30:00+01:00` | Datetime with offset |
>
> The `T` separator MAY be replaced with a space when the scalar is quoted.
>
> ```styx
> created 2024-03-15
> updated 2024-03-15T14:30:00Z
> local "2024-03-15 14:30:00"
> ```

> r[interp.datetime.subsec]
> Fractional seconds MAY be included with up to nanosecond precision:
>
> ```styx
> precise 2024-03-15T14:30:00.123456789Z
> ```

## Bytes

> r[interp.bytes.hex]
> A byte sequence MAY be represented as a hexadecimal string: an even number of hex digits `0-9`, `a-f`, `A-F`.
> Each pair of digits represents one byte. Underscores MAY appear between byte pairs for readability.
>
> ```styx
> hash deadbeef
> key 00_11_22_33
> empty ""              // zero bytes
> ```

> r[interp.bytes.base64]
> Implementations MAY support base64-encoded bytes, indicated by a `base64:` prefix or schema annotation.
> Standard base64 alphabet with `+/` and `=` padding MUST be supported.
> URL-safe alphabet with `-_` SHOULD be supported.
>
> ```styx
> data base64:SGVsbG8gV29ybGQ=
> ```

## Null and unit

> r[interp.null]
> The unit value `@` MAY be interpreted as null/nil/None in languages that support it.
> When deserializing to an optional type, unit indicates absence.
>
> ```styx
> value @              // null / None / nil
> ```

> r[interp.unit.field]
> When a field's value is unit and the target type is not optional, implementations MUST produce an error.

## Type coercion

> r[interp.coerce.none]
> Implementations MUST NOT perform implicit type coercion.
> A scalar that matches integer syntax MUST NOT automatically become a string, or vice versa.
> The target type determines interpretation.
>
> ```styx
> // If schema says port is @int:
> port 8080            // OK: interpreted as integer
>
> // If schema says port is @string:
> port 8080            // OK: interpreted as string "8080"
>
> // If schema says port is @int:
> port localhost       // ERROR: not a valid integer
> ```

> r[interp.error.context]
> When interpretation fails, the error MUST include:
> - The scalar's source location (file, line, column)
> - The scalar's text content (or a prefix if very long)
> - The expected type
> - Why the interpretation failed
