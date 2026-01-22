#![no_main]

use arbitrary::Arbitrary;
use libfuzzer_sys::fuzz_target;

#[derive(Arbitrary, Debug, Clone)]
enum Node {
    Text(SmallString),
    P {
        class: Option<SmallString>,
        text: SmallString,
    },
    Div {
        class: Option<SmallString>,
        id: Option<SmallString>,
        children: Vec<Node>,
    },
    Span {
        class: Option<SmallString>,
        text: SmallString,
    },
}

#[derive(Arbitrary, Debug, Clone)]
struct SmallString(
    #[arbitrary(with = |u: &mut arbitrary::Unstructured| {
    let len = u.int_in_range(0..=20)?;
    let bytes: Vec<u8> = (0..len)
        .map(|_| u.int_in_range(b'a'..=b'z'))
        .collect::<Result<_, _>>()?;
    Ok(String::from_utf8(bytes).unwrap())
})]
    String,
);

impl Node {
    fn to_html(&self, depth: usize) -> String {
        if depth > 4 {
            return String::new();
        }
        match self {
            Node::Text(s) => s.0.clone(),
            Node::P { class, text } => {
                let attrs = class
                    .as_ref()
                    .map(|c| format!(" class=\"{}\"", c.0))
                    .unwrap_or_default();
                format!("<p{}>{}</p>", attrs, text.0)
            }
            Node::Div {
                class,
                id,
                children,
            } => {
                let mut attrs = String::new();
                if let Some(c) = class {
                    attrs.push_str(&format!(" class=\"{}\"", c.0));
                }
                if let Some(i) = id {
                    attrs.push_str(&format!(" id=\"{}\"", i.0));
                }
                let inner: String = children
                    .iter()
                    .take(3)
                    .map(|c| c.to_html(depth + 1))
                    .collect();
                format!("<div{}>{}</div>", attrs, inner)
            }
            Node::Span { class, text } => {
                let attrs = class
                    .as_ref()
                    .map(|c| format!(" class=\"{}\"", c.0))
                    .unwrap_or_default();
                format!("<span{}>{}</span>", attrs, text.0)
            }
        }
    }
}

#[derive(Arbitrary, Debug)]
struct FuzzInput {
    old: Vec<Node>,
    new: Vec<Node>,
}

fn nodes_to_html(nodes: &[Node]) -> String {
    let inner: String = nodes.iter().take(4).map(|n| n.to_html(0)).collect();
    format!("<html><body>{}</body></html>", inner)
}

fuzz_target!(|input: FuzzInput| {
    let old_html = nodes_to_html(&input.old);
    let new_html = nodes_to_html(&input.new);

    let Ok(patches) = facet_html_diff::diff_html(&old_html, &new_html) else {
        return;
    };
    let Ok(mut tree) = facet_html_diff::parse_html(&old_html) else {
        return;
    };
    let Ok(()) = facet_html_diff::apply_patches(&mut tree, &patches) else {
        return;
    };

    let result = tree.to_html();
    let Ok(expected_tree) = facet_html_diff::parse_html(&new_html) else {
        return;
    };
    let expected = expected_tree.to_html();

    assert_eq!(
        result, expected,
        "Roundtrip failed!\nOld: {}\nNew: {}\nPatches: {:?}",
        old_html, new_html, patches
    );
});
