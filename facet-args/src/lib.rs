use facet_core::{Facet, FieldAttribute};
use facet_reflect::{PokeStruct, PokeUninit};

fn parse_field(field: PokeUninit, value: &str, field_index: usize, ps: &mut PokeStruct<'_>) {
    let field_shape = field.shape();
    log::trace!("Field shape: {}", field_shape);

    let field = match field {
        PokeUninit::Scalar(pv) => {
            let pv = match pv.typed::<String>() {
                Ok(pv) => {
                    pv.put(value.to_string());
                    unsafe { ps.mark_initialized(field_index) }
                    return;
                }
                Err(pv) => pv,
            };
            let pv = match pv.typed::<bool>() {
                Ok(pv) => {
                    log::trace!("Boolean field detected, setting to true");
                    pv.put(value.to_lowercase() == "true");
                    unsafe { ps.mark_initialized(field_index) }
                    return;
                }
                Err(pv) => pv,
            };
            PokeUninit::Scalar(pv)
        }
        field => field,
    };

    let parse = field_shape.vtable.parse.unwrap_or_else(|| {
        log::trace!("No parse function found for shape {}", field.shape());
        panic!("shape {} does not support parse", field.shape())
    });
    log::trace!("Parsing field value");
    unsafe { (parse)(value, field.into_value().data()) }.unwrap_or_else(|e| {
        log::trace!("Failed to parse field: {}", e);
        panic!("Failed to parse field of shape {}: {}", field_shape, e)
    });
    unsafe { ps.mark_initialized(field_index) }
}

pub fn from_slice<T: Facet>(s: &[&str]) -> T {
    log::trace!("Entering from_slice function");
    let mut s = s;
    let (poke, guard) = PokeUninit::alloc::<T>();
    log::trace!("Allocated Poke for type T");
    let mut ps = poke.into_struct();
    log::trace!("Converted Poke into struct");

    while let Some(token) = s.first() {
        log::trace!("Processing token: {}", token);
        s = &s[1..];

        if let Some(key) = token.strip_prefix("--") {
            log::trace!("Found named argument: {}", key);
            let (field_index, field) = ps.field_by_name(key).unwrap();
            if field.shape().is_type::<bool>() {
                parse_field(field, "true", field_index, &mut ps);
            } else {
                let value = s.first().expect("expected value after argument");
                log::trace!("Field value: {}", value);
                s = &s[1..];
                parse_field(field, value, field_index, &mut ps);
            }
        } else {
            log::trace!("Encountered positional argument: {}", token);
            for f in ps.def().fields {
                if f.attributes
                    .iter()
                    .any(|a| matches!(a, FieldAttribute::Arbitrary(a) if a.contains("positional")))
                {
                    let (field_index, field) = ps.field_by_name(f.name).unwrap();
                    parse_field(field, token, field_index, &mut ps);
                    break;
                }
            }
        }
    }
    ps.build(Some(guard))
}
