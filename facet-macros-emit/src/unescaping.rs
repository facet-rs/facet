use facet_macros_parse::DocInner;

pub fn unescape(doc_attr: &DocInner) -> String {
    // Handle doc comments - unescape quotes and backslashes
    // Note: Order matters - we must unescape \\ last to avoid double-unescaping
    let unescaped = doc_attr
        .value
        .as_str()
        .replace("\\\"", "\"")
        .replace("\\'", "'")
        .replace("\\\\", "\\");
    return unescaped;
}
