fn main() {
    let source = r#"
meta {
  id hello
  version 1
}
"#;
    let result = styx_tree::parse(source);
    println!("{result:#?}");
}
