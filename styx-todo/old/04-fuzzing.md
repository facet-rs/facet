# Fuzzing & Property Testing

**Status:** TODO  
**Priority:** Low

## Goal

Find edge cases and bugs through automated testing.

## Approach

### proptest for Property Testing

Properties to test:

**Parser roundtrip**
- Parse any valid input → serialize → parse again → same tree

**Formatter idempotence**
- Format once → format again → identical output

**Schema validation consistency**
- Valid document → modify to invalid → catches error
- Invalid document → fix error → validates

### cargo-fuzz for Fuzzing

Fuzz targets:

**Parser**
- Feed random bytes to parser
- Should never panic, always return Result

**Deserializer**
- Feed random valid styx to deserializer
- Should never panic

**Schema validator**
- Random document + random schema
- Should never panic

## Previous Findings

Fuzzing already found an infinite loop in heredoc parsing (fixed).

## Implementation

1. Add proptest dev-dependency
2. Create property tests in each crate
3. Set up cargo-fuzz targets
4. Add to CI (limited iterations)
