# MIME Type Registration

**Status:** TODO  
**Priority:** Low

## Goal

Register an official MIME type for styx files.

## Proposed MIME Type

```
application/vnd.styx
```

Or possibly:
```
text/x-styx
application/x-styx
```

## Registration Process

### IANA Registration

For `application/vnd.*`:
1. Fill out IANA media type registration form
2. Provide specification reference (styx.bearcove.eu)
3. Define file extension: `.styx`
4. Define magic bytes (if any): none (text format)

### Practical Integration

Even without IANA registration:
- GitHub linguist (for syntax highlighting)
- VS Code language associations
- HTTP Content-Type headers
- File managers / desktop environments

## File Extension

Primary: `.styx`
Schema files: `.schema.styx` (convention, not separate type)

## GitHub Linguist

Add to [linguist](https://github.com/github/linguist):
- Language name: Styx
- File extensions: `.styx`
- Type: data
- Grammar: tree-sitter-styx or TextMate
