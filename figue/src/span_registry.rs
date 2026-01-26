//! Virtual span registry for error reporting across multiple source documents.
//!
//! When deserializing a merged ConfigValue tree, we need to report errors with
//! spans that point back to the original source (CLI args, env vars, or config files).
//! Since facet-format expects a single linear span space, we assign virtual spans
//! to each value and maintain a registry that maps virtual spans back to their
//! real source locations.

use crate::config_value::ConfigValue;
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

/// Walk a ConfigValue tree and assign virtual spans, returning a modified tree
/// and a registry for looking up real spans.
pub fn assign_virtual_spans(value: &ConfigValue) -> (ConfigValue, SpanRegistry) {
    let mut registry = SpanRegistry::new();
    let new_value = assign_virtual_spans_recursive(value, &mut registry);
    (new_value, registry)
}

fn assign_virtual_spans_recursive(value: &ConfigValue, registry: &mut SpanRegistry) -> ConfigValue {
    match value {
        ConfigValue::Null(sourced) => {
            let virtual_span = sourced.span.map(|real_span| {
                let prov = sourced.provenance.clone().unwrap_or(Provenance::Default);
                registry.register(real_span, prov)
            });
            ConfigValue::Null(crate::config_value::Sourced {
                value: sourced.value,
                span: virtual_span,
                provenance: sourced.provenance.clone(),
            })
        }
        ConfigValue::Bool(sourced) => {
            let virtual_span = sourced.span.map(|real_span| {
                let prov = sourced.provenance.clone().unwrap_or(Provenance::Default);
                registry.register(real_span, prov)
            });
            ConfigValue::Bool(crate::config_value::Sourced {
                value: sourced.value,
                span: virtual_span,
                provenance: sourced.provenance.clone(),
            })
        }
        ConfigValue::Integer(sourced) => {
            let virtual_span = sourced.span.map(|real_span| {
                let prov = sourced.provenance.clone().unwrap_or(Provenance::Default);
                registry.register(real_span, prov)
            });
            ConfigValue::Integer(crate::config_value::Sourced {
                value: sourced.value,
                span: virtual_span,
                provenance: sourced.provenance.clone(),
            })
        }
        ConfigValue::Float(sourced) => {
            let virtual_span = sourced.span.map(|real_span| {
                let prov = sourced.provenance.clone().unwrap_or(Provenance::Default);
                registry.register(real_span, prov)
            });
            ConfigValue::Float(crate::config_value::Sourced {
                value: sourced.value,
                span: virtual_span,
                provenance: sourced.provenance.clone(),
            })
        }
        ConfigValue::String(sourced) => {
            let virtual_span = sourced.span.map(|real_span| {
                let prov = sourced.provenance.clone().unwrap_or(Provenance::Default);
                registry.register(real_span, prov)
            });
            ConfigValue::String(crate::config_value::Sourced {
                value: sourced.value.clone(),
                span: virtual_span,
                provenance: sourced.provenance.clone(),
            })
        }
        ConfigValue::Array(sourced) => {
            let virtual_span = sourced.span.map(|real_span| {
                let prov = sourced.provenance.clone().unwrap_or(Provenance::Default);
                registry.register(real_span, prov)
            });
            let new_items: Vec<ConfigValue> = sourced
                .value
                .iter()
                .map(|item| assign_virtual_spans_recursive(item, registry))
                .collect();
            ConfigValue::Array(crate::config_value::Sourced {
                value: new_items,
                span: virtual_span,
                provenance: sourced.provenance.clone(),
            })
        }
        ConfigValue::Object(sourced) => {
            let virtual_span = sourced.span.map(|real_span| {
                let prov = sourced.provenance.clone().unwrap_or(Provenance::Default);
                registry.register(real_span, prov)
            });
            let new_map: indexmap::IndexMap<String, ConfigValue, std::hash::RandomState> = sourced
                .value
                .iter()
                .map(|(k, v)| (k.clone(), assign_virtual_spans_recursive(v, registry)))
                .collect();
            ConfigValue::Object(crate::config_value::Sourced {
                value: new_map,
                span: virtual_span,
                provenance: sourced.provenance.clone(),
            })
        }
        ConfigValue::Enum(sourced) => {
            let virtual_span = sourced.span.map(|real_span| {
                let prov = sourced.provenance.clone().unwrap_or(Provenance::Default);
                registry.register(real_span, prov)
            });
            let new_fields: indexmap::IndexMap<String, ConfigValue, std::hash::RandomState> =
                sourced
                    .value
                    .fields
                    .iter()
                    .map(|(k, v)| (k.clone(), assign_virtual_spans_recursive(v, registry)))
                    .collect();
            ConfigValue::Enum(crate::config_value::Sourced {
                value: crate::config_value::EnumValue {
                    variant: sourced.value.variant.clone(),
                    fields: new_fields,
                },
                span: virtual_span,
                provenance: sourced.provenance.clone(),
            })
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
