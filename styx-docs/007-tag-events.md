# STYX Tag Events

This document defines the canonical mapping from STYX tag syntax to facet-format `ParseEvent` sequences.

## 1. Bare `@` (null/unit)

```styx
@
```

```
Scalar(Unit)
```

## 2. Unit tag `@Foo`

```styx
@Foo
```

```
VariantTag("Foo")
  Scalar(Unit)
```

## 3. Tag with explicit null payload `@Foo@`

```styx
@Foo@
```

```
VariantTag("Foo")
  Scalar(Unit)
```

## 4. Tag with sequence payload `@Foo(a b)`

```styx
@Foo(a b)
```

```
VariantTag("Foo")
  SequenceStart
    Scalar("a")
    Scalar("b")
  SequenceEnd
```

## 5. Tag with struct payload `@Foo{x 1}`

```styx
@Foo{x 1}
```

```
VariantTag("Foo")
  StructStart
    FieldKey("x")
    Scalar(1)
  StructEnd
```

## 6. Nested unit tags `@Foo(@Bar)`

```styx
@Foo(@Bar)
```

```
VariantTag("Foo")
  SequenceStart
    VariantTag("Bar")
      Scalar(Unit)
  SequenceEnd
```

## 7. Tag with struct containing tag `@Foo{x @Bar}`

```styx
@Foo{x @Bar}
```

```
VariantTag("Foo")
  StructStart
    FieldKey("x")
    VariantTag("Bar")
      Scalar(Unit)
  StructEnd
```

## 8. Field with unit tag value `x @Foo`

```styx
x @Foo
```

```
FieldKey("x")
VariantTag("Foo")
  Scalar(Unit)
```

## 9. Field with struct tag value `x @Foo{y 1}`

```styx
x @Foo{y 1}
```

```
FieldKey("x")
VariantTag("Foo")
  StructStart
    FieldKey("y")
    Scalar(1)
  StructEnd
```

## 10. Sequence of unit tags `(@Foo @Bar)`

```styx
(@Foo @Bar)
```

```
SequenceStart
  VariantTag("Foo")
    Scalar(Unit)
  VariantTag("Bar")
    Scalar(Unit)
SequenceEnd
```

## 11. Deeply nested `@Foo(@Bar{x 1})`

```styx
@Foo(@Bar{x 1})
```

```
VariantTag("Foo")
  SequenceStart
    VariantTag("Bar")
      StructStart
        FieldKey("x")
        Scalar(1)
      StructEnd
  SequenceEnd
```

## 12. `@` as a key `@ {x 1}`

```styx
@ {x 1}
```

```
FieldKey("@")
StructStart
  FieldKey("x")
  Scalar(1)
StructEnd
```

## Notes

- `@` alone is a null/unit value
- `@Foo` is a unit variant tag - emits `VariantTag` followed by `Scalar(Unit)`
- `@Foo@` is equivalent to `@Foo` (explicit null payload)
- `@Foo(...)` is a variant tag with sequence payload
- `@Foo{...}` is a variant tag with struct payload
- `@ ` (with space) used as a key represents the "default" or "additional fields" slot

## Implementation

The `VariantTag` event is handled by facet-format's `deserialize_enum_externally_tagged` function.
Unit variants are followed by `Scalar(Unit)`.
Tuple/struct variants have their payload events follow immediately after `VariantTag`.
