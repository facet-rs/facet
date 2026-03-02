use heck::ToKebabCase;
use roam_types::MethodDescriptor;

pub fn normalized_service(name: &str) -> String {
    name.to_kebab_case()
}

pub fn normalized_method(name: &str) -> String {
    name.to_kebab_case()
}

pub fn fq_name(detail: &MethodDescriptor) -> String {
    format!(
        "{}.{}",
        normalized_service(detail.service_name),
        normalized_method(detail.method_name)
    )
}

pub fn hex_u64(v: u64) -> String {
    format!("0x{v:016x}")
}
