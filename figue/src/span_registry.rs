//! Virtual span registry for error reporting across multiple source documents.
//!
//! When deserializing a merged ConfigValue tree, we need to report errors with
//! spans that point back to the original source (CLI args, env vars, or config files).
//! Since facet-format expects a single linear span space, we assign virtual spans
//! to each value and maintain a registry that maps virtual spans back to their
//! real source locations.

use crate::config_value::{ConfigValue, ConfigValueVisitorMut};
use crate::path::Path;
use crate::provenance::Provenance;
use facet_reflect::Span;

/// Entry in the span registry mapping virtual span to real location.
#[derive(Debug, Clone)]
pub struct SpanEntry {
    /// The real span within the source document.
    pub real_span: Span,
    /// Which source document this span refers to.
    pub provenance: Provenance,
}

/// Registry that maps virtual spans to real source locations.
#[derive(Debug, Default)]
pub struct SpanRegistry {
    /// Entries indexed by virtual span offset.
    /// Each entry represents one value in the ConfigValue tree.
    entries: Vec<SpanEntry>,
    /// Next virtual offset to assign.
    next_offset: usize,
}

impl SpanRegistry {
    /// Create a new empty registry.
    pub fn new() -> Self {
        Self::default()
    }

    /// Register a span and return its virtual span.
    ///
    /// The virtual span has a monotonically increasing offset that can be
    /// used to look up the real span later.
    pub fn register(&mut self, real_span: Span, provenance: Provenance) -> Span {
        let virtual_offset = self.next_offset;
        // Use the real length so error highlighting works correctly
        let virtual_span = Span::new(virtual_offset, real_span.len);

        self.entries.push(SpanEntry {
            real_span,
            provenance,
        });

        // Increment by at least 1 to ensure unique offsets
        self.next_offset += real_span.len.max(1);

        virtual_span
    }

    /// Look up a virtual span to get the real span and provenance.
    ///
    /// Returns None if the virtual span doesn't match any registered entry.
    #[allow(dead_code)]
    pub fn lookup(&self, virtual_span: Span) -> Option<&SpanEntry> {
        // Find the entry whose virtual offset range contains this span
        let mut current_offset = 0;
        for entry in &self.entries {
            let entry_len = entry.real_span.len.max(1);
            if virtual_span.offset >= current_offset
                && virtual_span.offset < current_offset + entry_len
            {
                return Some(entry);
            }
            current_offset += entry_len;
        }
        None
    }

    /// Look up by just the offset (for facet-format errors that only give offset).
    pub fn lookup_by_offset(&self, offset: usize) -> Option<&SpanEntry> {
        let mut current_offset = 0;
        for entry in &self.entries {
            let entry_len = entry.real_span.len.max(1);
            if offset >= current_offset && offset < current_offset + entry_len {
                return Some(entry);
            }
            current_offset += entry_len;
        }
        None
    }
}

/// Walk a ConfigValue tree and assign virtual spans, mutating the tree in place.
/// Returns the registry for looking up real spans.
pub fn assign_virtual_spans(value: &mut ConfigValue) -> SpanRegistry {
    let mut visitor = VirtualSpanVisitor {
        registry: SpanRegistry::new(),
    };
    let mut path = Path::new();
    value.visit_mut(&mut visitor, &mut path);
    visitor.registry
}

/// Visitor that assigns virtual spans to all values with real spans.
struct VirtualSpanVisitor {
    registry: SpanRegistry,
}

impl ConfigValueVisitorMut for VirtualSpanVisitor {
    fn visit_value(&mut self, _path: &Path, value: &mut ConfigValue) {
        if let Some(real_span) = value.span() {
            let prov = value.provenance().cloned().unwrap_or(Provenance::Default);
            let virtual_span = self.registry.register(real_span, prov);
            *value.span_mut() = Some(virtual_span);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_registry_register_and_lookup() {
        let mut registry = SpanRegistry::new();

        let real1 = Span::new(10, 5);
        let prov1 = Provenance::cli("--foo", "bar");
        let virtual1 = registry.register(real1, prov1.clone());

        let real2 = Span::new(100, 10);
        let prov2 = Provenance::env("FOO", "baz");
        let virtual2 = registry.register(real2, prov2.clone());

        // Virtual spans should be distinct
        assert_ne!(virtual1.offset, virtual2.offset);

        // Lookup should return correct entries
        let entry1 = registry.lookup(virtual1).unwrap();
        assert_eq!(entry1.real_span.offset, 10);
        assert_eq!(entry1.real_span.len, 5);

        let entry2 = registry.lookup(virtual2).unwrap();
        assert_eq!(entry2.real_span.offset, 100);
        assert_eq!(entry2.real_span.len, 10);
    }

    #[test]
    fn test_lookup_by_offset() {
        let mut registry = SpanRegistry::new();

        let real = Span::new(50, 8);
        let prov = Provenance::cli("--test", "value");
        let virtual_span = registry.register(real, prov);

        // Should find by the start offset
        let entry = registry.lookup_by_offset(virtual_span.offset).unwrap();
        assert_eq!(entry.real_span.offset, 50);
    }
}
