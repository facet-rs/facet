use facet_core::{Field, FieldFlags};

/// Phase used when checking whether struct fields declare how they map to XML.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum XmlAnnotationPhase {
    /// Validation performed before serialization.
    Serialize,
    /// Validation performed before deserialization.
    Deserialize,
}

/// Return all fields that require an XML annotation but don't have one.
pub(crate) fn fields_missing_xml_annotations(
    fields: &[Field],
    phase: XmlAnnotationPhase,
) -> Vec<&Field> {
    fields
        .iter()
        .filter(|field| field_requires_xml_annotation(field, phase))
        .filter(|field| !field_has_xml_mapping(field))
        .collect()
}

fn field_requires_xml_annotation(field: &Field, phase: XmlAnnotationPhase) -> bool {
    if field.is_metadata() || field.is_flattened() {
        return false;
    }

    if field.flags.contains(FieldFlags::SKIP) {
        return false;
    }

    match phase {
        XmlAnnotationPhase::Serialize => {
            if field.flags.contains(FieldFlags::SKIP_SERIALIZING) {
                return false;
            }
        }
        XmlAnnotationPhase::Deserialize => {
            if field.flags.contains(FieldFlags::SKIP_DESERIALIZING) {
                return false;
            }
        }
    }

    true
}

fn field_has_xml_mapping(field: &Field) -> bool {
    field.has_attr(Some("xml"), "attribute")
        || field.has_attr(Some("xml"), "text")
        || field.has_attr(Some("xml"), "elements")
        || field.has_attr(Some("xml"), "element")
        || field.has_attr(Some("xml"), "element_name")
        || field.is_child()
}
