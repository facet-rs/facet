use rapace_schema::MethodDetail;

pub fn normalized_service(name: &str) -> String {
    rapace_hash::kebab(name)
}

pub fn normalized_method(name: &str) -> String {
    rapace_hash::kebab(name)
}

pub fn fq_name(detail: &MethodDetail) -> String {
    format!(
        "{}.{}",
        normalized_service(&detail.service_name),
        normalized_method(&detail.method_name)
    )
}

pub fn hex_u64(v: u64) -> String {
    format!("0x{v:016x}")
}
