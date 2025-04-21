use facet_reflect::Peek;
use owo_colors::OwoColorize;

#[test]
fn main() {
    facet_testhelpers::setup();

    if !cfg!(miri) {
        let (data, shape) = facet_samplelibc::get_foo_and_shape();
        let peek = unsafe { Peek::unchecked_new(data.as_const(), shape) };
        eprintln!("{peek}");
        eprintln!("🔍 Display: {}", format_args!("{}", peek).bright_green());
        eprintln!("🐛 Debug: {}", format_args!("{:?}", peek).bright_blue());

        inspect(peek);
    }
}

fn inspect(peek: Peek) {
    inspect_with_indent(peek, 0);
}

fn inspect_with_indent(peek: Peek, indent: usize) {
    let indent_str = " ".repeat(indent * 4);

    if let Ok(ps) = peek.into_struct() {
        eprintln!(
            "{}📊 That struct has {} fields",
            indent_str,
            ps.field_count().to_string().bright_yellow()
        );
        for (k, v) in ps.fields() {
            eprintln!(
                "{}🔑 Field {} => {}",
                indent_str,
                k.name.bright_cyan(),
                v.to_string().bright_magenta()
            );
            inspect_with_indent(v, indent + 1);
        }
    }
}
