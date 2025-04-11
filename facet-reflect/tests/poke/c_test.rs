use facet_ansi::Stylize as _;
use facet_reflect::Peek;

#[test]
fn test_sample_libc() {
    facet_testhelpers::setup();

    if !cfg!(miri) {
        let (data, shape) = facet_samplelibc::get_foo_and_shape();
        let peek = unsafe { Peek::unchecked_new(data.as_const(), shape) };
        eprintln!("{}", facet_pretty::PrettyPrinter::new().format_peek(peek),);
        eprintln!("🔍 Display: {}", format!("{}", peek).bright_green());
        eprintln!("🐛 Debug: {}", format!("{:?}", peek).bright_blue());

        inspect(peek);
    }
}

fn inspect(peek: Peek) {
    inspect_with_indent(peek, 0);
}

fn inspect_with_indent(peek: Peek, indent: usize) {
    let indent_str = " ".repeat(indent * 4);

    let ps = match peek {
        Peek::Struct(ps) => ps,
        _ => {
            return;
        }
    };

    eprintln!(
        "{}📊 That struct has {} fields",
        indent_str,
        ps.field_count().to_string().bright_yellow()
    );
    for (k, v) in ps.fields() {
        eprintln!(
            "{}🔑 Field {} => {}",
            indent_str,
            k.to_string().bright_cyan(),
            v.to_string().bright_magenta()
        );
        inspect_with_indent(v.wrap(), indent + 1);
    }
}
