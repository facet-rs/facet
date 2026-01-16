// Regression tests for GitHub issues
// Run with: cargo +nightly miri test -p facet-html --test minimal_repro

use facet_html_dom::Html;
use facet_testhelpers::test;

// Issue #1568: Crash during error cleanup
#[test]
fn issue_1568_html_parse_error_cleanup() {
    // Simplified HTML that previously triggered a crash during error cleanup.
    let html = r#"<ul><li>text <code>code</code></li></ul>"#;

    // This should NOT crash during parsing or cleanup
    let result = facet_html::from_str::<Html>(html);

    // After the fix for #1575, this should parse successfully
    assert!(result.is_ok(), "Parsing should succeed: {:?}", result.err());
}

// Issue #1575: facet-html crashes on <li> with parentheses
// Root cause: Vec<Li> fields in Ul/Ol structs were missing #[facet(xml::elements)]
// attribute, which is required to properly group repeated child elements into a Vec.
#[test]
fn issue_1575_li_with_parentheses() {
    // This HTML previously crashed with SIGABRT when parsing
    let html = r#"<!DOCTYPE html>
<html>
<head><title>Test</title></head>
<body>
<ul>
<li><code>index.html</code> - renders the root section (<code>/</code>)</li>
</ul>
</body>
</html>"#;

    // This should parse successfully (not just not crash)
    let result = facet_html::from_str::<Html>(html);
    assert!(result.is_ok(), "Parsing should succeed: {:?}", result.err());
}

#[test]
fn issue_1575_simple_li_with_parentheses() {
    let html = r#"<ul><li>Some text (with parentheses)</li></ul>"#;

    let result = facet_html::from_str::<Html>(html);
    assert!(result.is_ok(), "Parsing should succeed: {:?}", result.err());
}

#[test]
fn issue_1575_li_with_description_and_parentheses() {
    let html = r#"<ul><li>Item - description (detail)</li></ul>"#;

    let result = facet_html::from_str::<Html>(html);
    assert!(result.is_ok(), "Parsing should succeed: {:?}", result.err());
}

#[test]
fn issue_1575_li_with_mixed_content() {
    use facet_html_dom::{FlowContent, Ul};

    // Test that mixed content (text + elements) in <li> is preserved correctly
    let html = r#"<ul><li><code>a</code> text (<code>b</code>)</li></ul>"#;

    let result = facet_html::from_str::<Ul>(html).expect("should parse");
    assert_eq!(result.li.len(), 1);

    // Verify the mixed content is parsed correctly
    let children = &result.li[0].children;
    assert_eq!(children.len(), 4); // code, text, code, text

    // First child should be code element
    assert!(matches!(&children[0], FlowContent::Code(_)));
    // Second child should be text
    assert!(matches!(&children[1], FlowContent::Text(_)));
    // Third child should be code element
    assert!(matches!(&children[2], FlowContent::Code(_)));
    // Fourth child should be text
    assert!(matches!(&children[3], FlowContent::Text(_)));
}

// Issue #1578: Round-trip serialization fails for script tags
// The problem was that script tags with `src` attribute were serializing attributes
// as child elements like `<script><src>/js/app.js</src></script>` instead of
// `<script src="/js/app.js"></script>`, which could not be re-parsed.
#[test]
fn issue_1578_script_roundtrip() {
    let input = r#"<html><body><script src="/js/app.js"></script></body></html>"#;

    // Parse it
    let doc: Html = facet_html::from_str(input).expect("Initial parse failed");

    // Serialize it
    let serialized = facet_html::to_string(&doc).expect("Serialization failed");

    // The serialized output should have src as an attribute, not a child element
    assert!(
        serialized.contains(r#"src="/js/app.js""#),
        "src should be serialized as an attribute, got: {}",
        serialized
    );
    assert!(
        !serialized.contains("<src>"),
        "src should NOT be serialized as a child element, got: {}",
        serialized
    );

    // Try to parse the serialized output (this is the core bug)
    let reparsed: Html =
        facet_html::from_str(&serialized).expect("Round-trip parse failed - this is issue #1578");

    // Verify the script tag data is preserved
    let body = reparsed.body.expect("body should exist");
    assert!(!body.children.is_empty(), "body should have children");
}

#[test]
fn issue_1578_script_with_inline_content() {
    let input = r#"<script>console.log("hello");</script>"#;

    // Parse it
    let doc: facet_html_dom::Script = facet_html::from_str(input).expect("Initial parse failed");
    assert_eq!(doc.text, r#"console.log("hello");"#);

    // Serialize it
    let serialized = facet_html::to_string(&doc).expect("Serialization failed");

    // Round-trip
    let reparsed: facet_html_dom::Script =
        facet_html::from_str(&serialized).expect("Round-trip failed");
    assert_eq!(reparsed.text, r#"console.log("hello");"#);
}

// Issue #1578: Comprehensive round-trip test for ALL HTML elements with ALL attributes
// This ensures that every element's attributes are properly serialized as HTML attributes
// (not child elements) and can be re-parsed.
#[test]
fn issue_1578_comprehensive_roundtrip_all_elements() {
    // A comprehensive HTML document that exercises all elements with their attributes
    let input = r##"<!DOCTYPE html>
<html lang="en" dir="ltr">
<head>
    <title>Test Page</title>
    <base href="https://example.com/" target="_blank"/>
    <link href="/style.css" rel="stylesheet" type="text/css" media="screen" integrity="sha384-abc" crossorigin="anonymous" sizes="32x32" as="style"/>
    <meta name="description" content="Test" charset="utf-8" http-equiv="refresh" property="og:title"/>
    <style media="print" type="text/css">body { color: red; }</style>
    <script src="/app.js" type="module" async="async" defer="defer" crossorigin="anonymous" integrity="sha384-xyz" referrerpolicy="no-referrer" nomodule="nomodule"></script>
</head>
<body id="main" class="container" style="margin: 0" tabindex="0" accesskey="b" draggable="true" contenteditable="true" spellcheck="true" hidden="hidden" role="main" onclick="alert(1)">
    <header><h1>Title</h1></header>
    <nav><a href="/home" target="_self" rel="noopener" download="file.txt" type="text/html" hreflang="en" referrerpolicy="origin">Home</a></nav>
    <main>
        <article>
            <section>
                <blockquote cite="https://source.com"><p>Quote</p></blockquote>
                <ol start="5" type="a" reversed="reversed"><li value="10">Item</li></ol>
                <ul><li>Bullet</li></ul>
                <dl><dt>Term</dt><dd>Definition</dd></dl>
                <p><q cite="https://ref.com">Inline quote</q></p>
                <p><data value="42">Forty-two</data></p>
                <p><time datetime="2024-01-01">New Year</time></p>
            </section>
            <section>
                <img src="/image.png" alt="Image" width="100" height="100" srcset="/img-2x.png 2x" sizes="100vw" loading="lazy" decoding="async" crossorigin="anonymous" referrerpolicy="no-referrer" usemap="#map" ismap="ismap"/>
                <iframe src="/frame.html" srcdoc="Hello" name="myframe" width="300" height="200" sandbox="allow-scripts" allow="fullscreen" allowfullscreen="allowfullscreen" loading="lazy" referrerpolicy="origin"></iframe>
                <object data="/object.swf" type="application/x-shockwave-flash" name="obj" width="400" height="300" usemap="#objmap"></object>
                <video src="/video.mp4" poster="/poster.jpg" width="640" height="480" controls="controls" autoplay="autoplay" loop="loop" muted="muted" preload="auto" playsinline="playsinline" crossorigin="use-credentials">
                    <source src="/video.webm" type="video/webm" srcset="/video-2x.webm 2x" sizes="100vw" media="(min-width: 800px)" width="1280" height="720"/>
                    <track src="/captions.vtt" kind="subtitles" srclang="en" label="English" default="default"/>
                </video>
                <audio src="/audio.mp3" controls="controls" autoplay="autoplay" loop="loop" muted="muted" preload="metadata" crossorigin="anonymous">
                    <source src="/audio.ogg" type="audio/ogg"/>
                </audio>
                <picture><source srcset="/img.webp" type="image/webp"/><img src="/img.png" alt="Fallback"/></picture>
                <canvas width="200" height="200">Canvas not supported</canvas>
                <svg width="100" height="100" viewBox="0 0 100 100" xmlns="http://www.w3.org/2000/svg" preserveAspectRatio="xMidYMid"></svg>
            </section>
            <section>
                <table>
                    <caption>Table Caption</caption>
                    <colgroup span="2"><col span="1"/></colgroup>
                    <thead><tr><th colspan="2" rowspan="1" scope="col" headers="h1" abbr="Header">Header</th></tr></thead>
                    <tbody><tr><td colspan="1" rowspan="2" headers="h1">Cell</td></tr></tbody>
                    <tfoot><tr><td>Footer</td></tr></tfoot>
                </table>
            </section>
            <section>
                <form action="/submit" method="post" enctype="multipart/form-data" target="_blank" name="myform" autocomplete="on" novalidate="novalidate" accept-charset="UTF-8">
                    <fieldset name="group1" disabled="disabled" form="myform">
                        <legend>Form Group</legend>
                        <label for="name">Name:</label>
                        <input type="text" name="name" value="John" placeholder="Enter name" required="required" disabled="disabled" readonly="readonly" autocomplete="name" autofocus="autofocus" min="0" max="100" step="1" pattern="[A-Za-z]+" size="20" maxlength="50" minlength="2" multiple="multiple" accept="image/*" alt="Image input" src="/button.png" width="50" height="50" list="datalist1" form="myform" formaction="/alt-submit" formmethod="get" formenctype="text/plain" formtarget="_self" formnovalidate="formnovalidate"/>
                        <input type="checkbox" name="agree" checked="checked"/>
                        <button type="submit" name="submit" value="go" disabled="disabled" autofocus="autofocus" form="myform" formaction="/action" formmethod="post" formenctype="multipart/form-data" formtarget="_blank" formnovalidate="formnovalidate">Submit</button>
                        <select name="choice" multiple="multiple" size="3" required="required" disabled="disabled" autofocus="autofocus" autocomplete="off" form="myform">
                            <optgroup label="Group 1" disabled="disabled">
                                <option value="a" selected="selected" disabled="disabled" label="Option A">A</option>
                            </optgroup>
                            <option value="b">B</option>
                        </select>
                        <textarea name="message" rows="5" cols="40" placeholder="Message" required="required" disabled="disabled" readonly="readonly" autofocus="autofocus" autocomplete="off" maxlength="1000" minlength="10" wrap="soft" form="myform">Default text</textarea>
                        <output for="name" name="result" form="myform"></output>
                        <progress value="50" max="100">50%</progress>
                        <meter value="0.6" min="0" max="1" low="0.3" high="0.8" optimum="0.5">60%</meter>
                        <datalist id="datalist1"><option value="Option1">Option1</option></datalist>
                    </fieldset>
                </form>
            </section>
            <section>
                <details open="open">
                    <summary>Details</summary>
                    <p>Hidden content</p>
                </details>
                <dialog open="open"><p>Dialog content</p></dialog>
            </section>
        </article>
    </main>
    <aside><p>Sidebar</p></aside>
    <footer><address>Contact</address></footer>
    <template><div>Template content</div></template>
    <noscript><p>JavaScript required</p></noscript>
</body>
</html>"##;

    // Parse the comprehensive HTML
    let doc: Html =
        facet_html::from_str(input).expect("Initial parse of comprehensive HTML failed");

    // Serialize it
    let serialized =
        facet_html::to_string(&doc).expect("Serialization of comprehensive HTML failed");

    // Verify value attributes are in the serialized output as attributes (not child elements)
    // Format: (should_contain, should_not_contain_as_element)
    let value_attr_checks = vec![
        // Base
        (r#"href="https://example.com/""#, "<href>"),
        (r#"target="_blank""#, "<target>"),
        // Link
        (r#"rel="stylesheet""#, "<rel>"),
        (r#"integrity="sha384-abc""#, "<integrity>"),
        // Meta
        (r#"charset="utf-8""#, "<charset>"),
        (r#"content="Test""#, "<content>"),
        // Style
        (r#"media="print""#, "<media>"),
        // Script
        (r#"src="/app.js""#, "<src>"),
        (r#"type="module""#, "<type>"),
        // A
        (r#"href="/home""#, "<href>"),
        (r#"download="file.txt""#, "<download>"),
        // Img
        (r#"alt="Image""#, "<alt>"),
        (r#"width="100""#, "<width>"),
        (r#"loading="lazy""#, "<loading>"),
        // Video
        (r#"poster="/poster.jpg""#, "<poster>"),
        // Form
        (r#"action="/submit""#, "<action>"),
        (r#"method="post""#, "<method>"),
        // Input
        (r#"placeholder="Enter name""#, "<placeholder>"),
        // Textarea
        (r#"rows="5""#, "<rows>"),
        (r#"cols="40""#, "<cols>"),
    ];

    for (should_contain, should_not_contain) in &value_attr_checks {
        assert!(
            serialized.contains(should_contain),
            "Serialized output should contain {} but got:\n{}",
            should_contain,
            &serialized[..serialized.len().min(2000)]
        );
        assert!(
            !serialized.contains(should_not_contain),
            "Serialized output should NOT contain {} (attribute serialized as element) but got:\n{}",
            should_not_contain,
            &serialized[..serialized.len().min(2000)]
        );
    }

    // Boolean attributes are serialized without values (just the attribute name)
    // Verify they are NOT serialized as child elements
    let boolean_attr_checks = vec![
        "<controls>",
        "<autoplay>",
        "<loop>",
        "<muted>",
        "<required>",
        "<disabled>",
        "<readonly>",
        "<multiple>",
        "<checked>",
        "<selected>",
        "<open>",
        "<async>",
        "<defer>",
        "<nomodule>",
        "<hidden>",
        "<autofocus>",
        "<novalidate>",
        "<formnovalidate>",
        "<reversed>",
        "<ismap>",
        "<allowfullscreen>",
        "<playsinline>",
        "<default>",
    ];

    for should_not_contain in &boolean_attr_checks {
        assert!(
            !serialized.contains(should_not_contain),
            "Boolean attribute {} should NOT be serialized as a child element, got:\n{}",
            should_not_contain,
            &serialized[..serialized.len().min(2000)]
        );
    }

    // The critical test: can we re-parse the serialized output?
    let reparsed: Html = facet_html::from_str(&serialized)
        .expect("Round-trip parse failed - serialized output is not valid facet-html input");

    // Verify we got meaningful content back
    assert!(
        reparsed.head.is_some(),
        "head should exist after round-trip"
    );
    assert!(
        reparsed.body.is_some(),
        "body should exist after round-trip"
    );

    // Second round-trip to ensure stability
    let reserialized = facet_html::to_string(&reparsed).expect("Second serialization failed");
    let _: Html = facet_html::from_str(&reserialized).expect("Second round-trip parse failed");
}

// Issue #1621: data-* attributes not captured in flattened HashMap extra field
#[test]
fn issue_1621_data_attributes_captured() {
    use facet_html_dom::FlowContent;

    let html = r#"<html><head></head><body><div class="test" data-icon="book" data-custom="42">Hello</div></body></html>"#;

    let doc: Html = facet_html::from_str(html).expect("parse");

    if let Some(body) = &doc.body {
        for child in &body.children {
            if let FlowContent::Div(div) = child {
                assert_eq!(div.attrs.class, Some("test".to_string()));
                assert!(
                    div.attrs.extra.contains_key("data-icon"),
                    "data-icon should be in extra, got: {:?}",
                    div.attrs.extra
                );
                assert_eq!(div.attrs.extra.get("data-icon"), Some(&"book".to_string()));
                assert!(
                    div.attrs.extra.contains_key("data-custom"),
                    "data-custom should be in extra"
                );
                assert_eq!(div.attrs.extra.get("data-custom"), Some(&"42".to_string()));
                return;
            }
        }
    }
    panic!("Should have found a div element");
}

// Simpler test for issue #1621: direct div parsing
#[test]
fn issue_1621_data_attributes_direct() {
    use facet_html_dom::Div;

    let html = r#"<div class="test" data-icon="book" data-custom="42">Hello</div>"#;

    let div: Div = facet_html::from_str(html).expect("parse");

    assert_eq!(div.attrs.class, Some("test".to_string()));
    assert!(
        div.attrs.extra.contains_key("data-icon"),
        "data-icon should be in extra, got: {:?}",
        div.attrs.extra
    );
    assert_eq!(div.attrs.extra.get("data-icon"), Some(&"book".to_string()));
    assert!(
        div.attrs.extra.contains_key("data-custom"),
        "data-custom should be in extra"
    );
    assert_eq!(div.attrs.extra.get("data-custom"), Some(&"42".to_string()));
}

// Debug test to understand what's being parsed
#[test]
fn issue_1629_debug() {
    use facet_html_dom::{Div, FlowContent};

    let html = r#"<div><a-k>fn</a-k></div>"#;
    let div: Div = facet_html::from_str(html).expect("Parse failed");

    eprintln!("Children count: {}", div.children.len());
    for (i, child) in div.children.iter().enumerate() {
        match child {
            FlowContent::Text(s) => eprintln!("  [{}] Text: {:?}", i, s),
            FlowContent::Custom(c) => eprintln!("  [{}] Custom: tag={:?}", i, c.tag),
            _ => eprintln!("  [{}] Other element", i),
        }
    }

    // Let's check if the first child is the text "fn" (from a-k being dropped)
    if let Some(FlowContent::Text(t)) = div.children.first() {
        eprintln!("First child is Text: {:?}", t);
    }
}

// Issue #1629: Custom elements (like <a-k>, <a-f> from arborium syntax highlighting)
// are dropped during parse/serialize roundtrip.
#[test]
fn issue_1629_custom_elements_preserved() {
    use facet_html_dom::{Div, FlowContent};

    // HTML with custom elements (syntax highlighting spans)
    let html = r#"<div><a-k>fn</a-k> <a-f>main</a-f>() {}</div>"#;

    // Parse it
    let div: Div = facet_html::from_str(html).expect("Parse failed");

    // Verify custom elements are preserved
    // Should have 4 children: custom "a-k", text " ", custom "a-f", text "() {}"
    assert!(
        div.children.len() >= 2,
        "Should have at least 2 children (custom elements), got {} children",
        div.children.len()
    );

    // Find custom elements
    let mut found_a_k = false;
    let mut found_a_f = false;
    for child in &div.children {
        if let FlowContent::Custom(custom) = child {
            if custom.tag == "a-k" {
                found_a_k = true;
            } else if custom.tag == "a-f" {
                found_a_f = true;
            }
        }
    }

    assert!(
        found_a_k,
        "Should find <a-k> custom element, got {} children",
        div.children.len()
    );
    assert!(
        found_a_f,
        "Should find <a-f> custom element, got {} children",
        div.children.len()
    );

    // Serialize back
    let serialized = facet_html::to_string(&div).expect("Serialize failed");

    // Verify the custom elements appear in the output
    assert!(
        serialized.contains("<a-k>"),
        "Serialized output should contain <a-k>, got: {}",
        serialized
    );
    assert!(
        serialized.contains("<a-f>"),
        "Serialized output should contain <a-f>, got: {}",
        serialized
    );
    assert!(
        serialized.contains("</a-k>"),
        "Serialized output should contain </a-k>, got: {}",
        serialized
    );
    assert!(
        serialized.contains("</a-f>"),
        "Serialized output should contain </a-f>, got: {}",
        serialized
    );
}

#[test]
fn issue_1629_custom_elements_roundtrip() {
    use facet_html_dom::Div;

    // HTML with custom elements
    let html = r#"<div><my-component class="test">Hello</my-component></div>"#;

    // Parse -> Serialize -> Parse
    let div1: Div = facet_html::from_str(html).expect("First parse failed");
    let serialized = facet_html::to_string(&div1).expect("Serialize failed");
    let div2: Div = facet_html::from_str(&serialized).expect("Second parse failed");
    let reserialized = facet_html::to_string(&div2).expect("Reserialize failed");

    // Verify idempotence
    assert_eq!(serialized, reserialized, "Roundtrip should be idempotent");

    // Verify custom element preserved
    assert!(
        serialized.contains("<my-component"),
        "Should contain custom element tag: {}",
        serialized
    );
    assert!(
        serialized.contains("</my-component>"),
        "Should contain closing tag: {}",
        serialized
    );
}

// Issue #1633: Whitespace added inside pre/code elements breaks preformatted content
// When facet-html serializes HTML containing <pre> or <code> elements, it adds
// indentation and newlines between child elements. Inside preformatted content,
// this whitespace is significant and breaks the rendering.
#[test]
fn issue_1633_preformatted_whitespace_preserved() {
    use facet_html_dom::Pre;

    // HTML with custom elements inside <pre><code> (from arborium syntax highlighting)
    let html = r#"<pre><code class="language-bash"><a-f>curl</a-f> <a-co>--proto</a-co> <a-s>'=https'</a-s></code></pre>"#;

    // Parse it
    let pre: Pre = facet_html::from_str(html).expect("Parse failed");

    // Serialize it with pretty printing (this is where the bug manifests)
    let serialized = facet_html::to_string_pretty(&pre).expect("Serialize failed");

    // The serialized output should NOT have newlines/indentation inside pre/code
    // Specifically, it should NOT look like:
    // <pre>
    //   <code>
    //     <a-f>curl</a-f>
    //
    // Instead, content should stay inline within preformatted elements

    // Check that there's no newline immediately after <code
    assert!(
        !serialized.contains("<code class=\"language-bash\">\n"),
        "Should NOT have newline after <code> opening tag in preformatted content, got:\n{}",
        serialized
    );

    // Check that custom elements don't have leading indentation
    assert!(
        !serialized.contains("  <a-f>"),
        "Should NOT have indentation before <a-f> inside preformatted content, got:\n{}",
        serialized
    );

    // The content between <code> and </code> should be on one line
    // Extract the code content and verify
    let code_start = serialized.find("<code").expect("should have code tag");
    let code_end = serialized
        .find("</code>")
        .expect("should have closing code tag");
    let code_section = &serialized[code_start..code_end];

    // Count newlines inside the code section - should be zero
    let newlines_in_code = code_section.matches('\n').count();
    assert_eq!(
        newlines_in_code, 0,
        "Should have no newlines inside <code> element, got {} newlines in:\n{}",
        newlines_in_code, code_section
    );
}

#[test]
fn issue_1633_nested_preformatted_elements() {
    use facet_html_dom::Div;

    // A div containing a pre with code and custom elements
    let html = r#"<div><pre><code><a-k>fn</a-k> <a-f>main</a-f>() {}</code></pre></div>"#;

    let div: Div = facet_html::from_str(html).expect("Parse failed");
    let serialized = facet_html::to_string_pretty(&div).expect("Serialize failed");

    // The div can have normal formatting, but pre/code content should be preserved
    // Check that inside pre/code there are no extra newlines
    let pre_start = serialized.find("<pre>").expect("should have pre tag");
    let pre_end = serialized
        .find("</pre>")
        .expect("should have closing pre tag");
    let pre_content = &serialized[pre_start + 5..pre_end]; // Skip "<pre>"

    // The entire content between <pre> and </pre> should be on one line
    let newlines_in_pre = pre_content.matches('\n').count();
    assert_eq!(
        newlines_in_pre, 0,
        "Should have no newlines inside <pre> element, got {} newlines in:\n{}",
        newlines_in_pre, pre_content
    );
}

#[test]
fn issue_1633_roundtrip_preserves_preformatted() {
    use facet_html_dom::Pre;

    let html = r#"<pre><code class="language-rust"><a-k>let</a-k> x = <a-n>42</a-n>;</code></pre>"#;

    // Parse -> Serialize -> Parse -> Serialize should be stable
    let pre1: Pre = facet_html::from_str(html).expect("First parse failed");
    let serialized1 = facet_html::to_string_pretty(&pre1).expect("First serialize failed");

    let pre2: Pre = facet_html::from_str(&serialized1).expect("Second parse failed");
    let serialized2 = facet_html::to_string_pretty(&pre2).expect("Second serialize failed");

    // Round-trip should be stable (idempotent after first serialize)
    assert_eq!(
        serialized1, serialized2,
        "Serialization should be idempotent for preformatted content"
    );
}

#[test]
fn issue_1633_textarea_whitespace_preserved() {
    use facet_html_dom::Textarea;

    // Textarea is another whitespace-sensitive element
    let html = r#"<textarea>Line 1
Line 2
Line 3</textarea>"#;

    let textarea: Textarea = facet_html::from_str(html).expect("Parse failed");
    let serialized = facet_html::to_string_pretty(&textarea).expect("Serialize failed");

    // Should NOT add extra indentation inside textarea
    assert!(
        !serialized.contains("  Line"),
        "Should NOT add indentation to textarea content, got:\n{}",
        serialized
    );
}

#[test]
fn issue_1656_deserialization_of_extra_element() {
    let input = r#"<html><extra>value</extra></html>"#;
    let html: facet_html_dom::Html = facet_html::from_str(input).unwrap();
    let output = facet_html::to_string(&html).unwrap();
    assert_eq!(output, r#"<html></html>"#);
}

#[test]
fn issue_1656_deserialization_of_extra_attribute() {
    let input = r#"<html extra="value"></html>"#;
    let html: facet_html_dom::Html = facet_html::from_str(input).unwrap();
    let output = facet_html::to_string(&html).unwrap();
    assert_eq!(output, input);
}

#[test]
fn preserve_meaningful_ws_1() {
    let input = indoc::indoc! {r#"
        <html><body><pre>
        line 1
        line 2
        line 3
        </pre></body></html>
    "#}
    .trim_end_matches('\n');
    let html: facet_html_dom::Html = facet_html::from_str(input).unwrap();
    let output = facet_html::to_string(&html).unwrap();
    assert_eq!(output, input);
}

#[test]
fn preserve_meaningful_ws_2() {
    let input = indoc::indoc! {r#"
        <html><body><pre>
            +-------+
            |       |
            +-------+
        </pre></body></html>
    "#}
    .trim_end_matches('\n');
    let html: facet_html_dom::Html = facet_html::from_str(input).unwrap();
    let output = facet_html::to_string(&html).unwrap();
    assert_eq!(output, input);
}

// Issue: Code blocks with syntax highlighting lose newlines
// When arborium syntax-highlights code, it produces custom elements like <a-k>, <a-f>, etc.
// These are inside <pre><code>...</code></pre>. The newlines between lines of code
// are being stripped during parse/serialize.
#[test]
fn issue_code_highlighting_newlines_preserved() {
    use facet_html_dom::Pre;

    // This is what arborium produces for highlighted Rust code.
    // Note the newlines between lines of code - these MUST be preserved.
    let input = indoc::indoc! {r#"
        <pre><code class="language-rust"><a-k>fn</a-k> <a-f>greet</a-f>(<a-v>name</a-v>: &amp;<a-t>str</a-t>) {
            <a-m>println!</a-m>(<a-s>"Hello, {}!"</a-s>, name);
        }

        <a-k>fn</a-k> <a-f>main</a-f>() {
            <a-f>greet</a-f>(<a-s>"World"</a-s>);
        }</code></pre>
    "#}
    .trim();

    // Parse it
    let pre: Pre = facet_html::from_str(input).expect("Parse failed");

    // Serialize it back
    let output = facet_html::to_string(&pre).expect("Serialize failed");

    // The output should preserve newlines between lines of code
    // Count newlines in the code block
    let code_start = output.find("<code").expect("should have code tag");
    let code_end = output
        .find("</code>")
        .expect("should have closing code tag");
    let code_section = &output[code_start..code_end];

    let newlines = code_section.matches('\n').count();

    // The original has 5 newlines inside the code block:
    // 1. After first line "...str) {"
    // 2. After println line
    // 3. After closing brace "}"
    // 4. Blank line between functions
    // 5. After fn main line
    // 6. After greet call
    assert!(
        newlines >= 4,
        "Code block should preserve newlines. Expected at least 4, got {}.\nOutput:\n{}",
        newlines,
        output
    );
}

#[test]
fn issue_code_highlighting_simple_two_lines() {
    use facet_html_dom::Pre;

    // Simplest case: two lines of code with a newline between them
    let input = "<pre><code><a-k>fn</a-k> <a-f>foo</a-f>() {}\n<a-k>fn</a-k> <a-f>bar</a-f>() {}</code></pre>";

    let pre: Pre = facet_html::from_str(input).expect("Parse failed");
    let output = facet_html::to_string(&pre).expect("Serialize failed");

    // Should have the newline between the two function declarations
    assert!(
        output.contains("}\n<a-k>") || output.contains("{}\n"),
        "Should preserve newline between lines. Got:\n{}",
        output
    );
}

#[test]
fn issue_code_highlighting_text_nodes_with_newlines() {
    use facet_html_dom::Code;

    // Even simpler: just text nodes with newlines inside <code>
    let input = "<code>line1\nline2\nline3</code>";

    let code: Code = facet_html::from_str(input).expect("Parse failed");
    let output = facet_html::to_string(&code).expect("Serialize failed");

    assert!(
        output.contains("line1\nline2"),
        "Should preserve newlines in text. Got:\n{}",
        output
    );
}

// Test the ACTUAL use case: parsing a full HTML document with code blocks in the body.
// This is what dodeca's HTML cell does.
#[test]
fn issue_full_document_with_code_block() {
    use facet_html_dom::Html;

    // Simulate what dodeca produces: a full HTML document with syntax-highlighted code
    let input = r#"<!DOCTYPE html>
<html>
<head>
  <title>Test</title>
</head>
<body>
  <h1>Test</h1>
  <pre><code class="language-rust"><a-k>fn</a-k> <a-f>greet</a-f>(<a-v>name</a-v>: &amp;<a-t>str</a-t>) {
    <a-m>println!</a-m>(<a-s>"Hello, {}!"</a-s>, name);
}

<a-k>fn</a-k> <a-f>main</a-f>() {
    <a-f>greet</a-f>(<a-s>"World"</a-s>);
}</code></pre>
</body>
</html>"#;

    let doc: Html = facet_html::from_str(input).expect("Parse failed");
    let output = facet_html::to_string(&doc).expect("Serialize failed");

    // Find the code block
    let code_start = output.find("<code").expect("should have <code>");
    let code_end = output.find("</code>").expect("should have </code>");
    let code_section = &output[code_start..code_end];

    let newlines = code_section.matches('\n').count();

    // The original code block has 5 newlines inside it
    assert!(
        newlines >= 4,
        "Full document code block should preserve newlines. Found {} newlines in:\n{}",
        newlines,
        code_section
    );
}

// Issue #1737: HTML-like closing tags inside <script> corrupt document parsing
// When parsing HTML with <script> tags containing HTML-like content in JavaScript strings
// (e.g., '</dt>'), facet-html was incorrectly interpreting these as actual HTML closing tags.
#[test]
fn issue_1737_script_with_html_closing_tag_in_string() {
    use facet_html_dom::Script;

    let html = r#"<script>var x = '</dt>';</script>"#;
    let script: Script =
        facet_html::from_str(html).expect("Should parse script with closing tag in content");

    assert_eq!(
        script.text, "var x = '</dt>';",
        "Script content should preserve HTML-like strings"
    );
}

#[test]
fn issue_1737_script_with_complex_html_in_strings() {
    use facet_html_dom::Script;

    // Test case from the issue: build info popup script that generates HTML dynamically
    let html = r#"<script>depsHtml = '<dt>Dependencies</dt><dd><div class="deps-list">';</script>"#;
    let script: Script =
        facet_html::from_str(html).expect("Should parse script with complex HTML in strings");

    assert_eq!(
        script.text, r#"depsHtml = '<dt>Dependencies</dt><dd><div class="deps-list">';"#,
        "Script content should preserve complex HTML-like strings"
    );
}

#[test]
fn issue_1737_full_document_with_script_html_strings() {
    use facet_html_dom::Html;

    let html = r#"<!DOCTYPE html>
<html lang="en">
<head>
<title>Test</title>
<script>
var x = '<dt>Term</dt>';
</script>
</head>
<body>
<p>Hello</p>
</body>
</html>"#;

    let doc: Html = facet_html::from_str(html).expect("Should parse document");

    // Check the script content is preserved
    let head = doc.head.as_ref().expect("Should have head");
    let scripts: Vec<_> = head.scripts().collect();
    assert!(!scripts.is_empty(), "Should have script element");
    assert!(
        scripts[0].text.contains("'<dt>Term</dt>'"),
        "Script text should contain the HTML string literal, got: {:?}",
        scripts[0].text
    );

    // Verify document structure wasn't corrupted
    assert!(doc.body.is_some(), "Should have body");
}

#[test]
fn issue_1737_style_with_html_like_content() {
    use facet_html_dom::Style;

    // Style elements should also be treated as raw text
    let html = r#"<style>/* comment with </div> in it */ .class { color: red; }</style>"#;
    let style: Style =
        facet_html::from_str(html).expect("Should parse style with HTML-like content");

    assert!(
        style.text.contains("</div>"),
        "Style content should preserve HTML-like content, got: {:?}",
        style.text
    );
}

// Test full Html document with custom elements (like arborium syntax highlighting)
#[test]
fn custom_elements_in_full_html_document() {
    let html = r#"<!DOCTYPE html>
<html lang="en">
<head><title>Test</title></head>
<body>
<pre><code><a-c>/// Comment</a-c></code></pre>
</body>
</html>"#;

    let result = facet_html::from_str::<Html>(html);
    match &result {
        Ok(doc) => {
            eprintln!("Parse succeeded!");
            eprintln!("head is_some: {}", doc.head.is_some());
            eprintln!("body is_some: {}", doc.body.is_some());
        }
        Err(e) => {
            eprintln!("Parse failed: {}", e);
        }
    }

    assert!(result.is_ok(), "Parsing should succeed: {:?}", result.err());
    let doc = result.unwrap();
    assert!(doc.head.is_some(), "head should be present");
    assert!(doc.body.is_some(), "body should be present");
}

// Test nested pre/code blocks (malformed but should not produce empty document)
#[test]
fn nested_pre_code_blocks() {
    let html = r#"<!DOCTYPE html>
<html lang="en">
<head><title>Test</title></head>
<body>
<pre><code>outer code
<pre><code>inner code</code></pre>
more outer code
</code></pre>
</body>
</html>"#;

    let result = facet_html::from_str::<Html>(html);
    match &result {
        Ok(doc) => {
            eprintln!("Parse succeeded!");
            eprintln!("head is_some: {}", doc.head.is_some());
            eprintln!("body is_some: {}", doc.body.is_some());
        }
        Err(e) => {
            eprintln!("Parse failed: {}", e);
        }
    }

    // Even if the HTML is malformed, we should get a valid document structure
    assert!(result.is_ok(), "Parsing should succeed: {:?}", result.err());
    let doc = result.unwrap();
    assert!(doc.head.is_some(), "head should be present");
    assert!(doc.body.is_some(), "body should be present");
}

// Stray end tag for custom element should not corrupt document structure
// This was causing the entire document to become empty when an end tag
// like </a-c> appeared with no matching start tag.
#[test]
fn stray_end_tag_does_not_corrupt_document() {
    // This HTML has a stray </a-c> end tag after nested pre/code
    // (two <a-c> elements already closed, then stray </a-c>)
    let html = r#"<!DOCTYPE html>
<html><head><title>T</title></head><body>
<pre><code><a-c>x</a-c><a-c>z</a-c><pre><code>q</code></pre></a-c><a-at>m</a-at></code></pre>
</body></html>"#;

    let result = facet_html::from_str::<Html>(html);
    assert!(result.is_ok(), "Parsing should succeed: {:?}", result.err());

    let doc = result.unwrap();
    assert!(
        doc.head.is_some(),
        "head should be present despite stray end tag"
    );
    assert!(
        doc.body.is_some(),
        "body should be present despite stray end tag"
    );
}

// =============================================================================
// Canonical roundtrip tests: verify html == serialize(parse(html))
// These test that already-normalized HTML is preserved exactly through roundtrip.
// These would have caught the inline element spacing bug.
// =============================================================================

use facet_html_dom::{Blockquote, Code, Div, Figcaption, H1, Label, Li, P, Pre, Td};

#[test]
fn canonical_inline_link_in_text() {
    // Inline <a> should not have spaces added around it
    let html = "<p>See the <a href=\"#\">documentation</a> for details.</p>";
    let parsed: P = facet_html::from_str(html).expect("parse");
    let serialized = facet_html::to_string(&parsed).expect("serialize");
    assert_eq!(html, serialized, "Canonical HTML should roundtrip exactly");
}

#[test]
fn canonical_multiple_inline_elements() {
    // Multiple inline elements in a row
    let html = "<p>This is <strong>bold</strong> and <em>italic</em> text.</p>";
    let parsed: P = facet_html::from_str(html).expect("parse");
    let serialized = facet_html::to_string(&parsed).expect("serialize");
    assert_eq!(html, serialized, "Canonical HTML should roundtrip exactly");
}

#[test]
fn canonical_nested_inline_elements() {
    // Nested inline elements
    let html = "<p>This is <strong><em>bold italic</em></strong> text.</p>";
    let parsed: P = facet_html::from_str(html).expect("parse");
    let serialized = facet_html::to_string(&parsed).expect("serialize");
    assert_eq!(html, serialized, "Canonical HTML should roundtrip exactly");
}

#[test]
fn canonical_inline_code_in_text() {
    // Inline <code> in text
    let html = "<p>Use the <code>foo()</code> function.</p>";
    let parsed: P = facet_html::from_str(html).expect("parse");
    let serialized = facet_html::to_string(&parsed).expect("serialize");
    assert_eq!(html, serialized, "Canonical HTML should roundtrip exactly");
}

#[test]
fn canonical_link_with_code() {
    // Link containing code
    let html = "<p>See <a href=\"#\"><code>example</code></a> here.</p>";
    let parsed: P = facet_html::from_str(html).expect("parse");
    let serialized = facet_html::to_string(&parsed).expect("serialize");
    assert_eq!(html, serialized, "Canonical HTML should roundtrip exactly");
}

#[test]
fn canonical_list_item_with_link() {
    // List item with inline link - this was the original bug
    let html = "<li>First item with <a href=\"#\">link</a>.</li>";
    let parsed: Li = facet_html::from_str(html).expect("parse");
    let serialized = facet_html::to_string(&parsed).expect("serialize");
    assert_eq!(html, serialized, "Canonical HTML should roundtrip exactly");
}

#[test]
fn canonical_span_in_div() {
    // Span (inline) in div (block)
    let html = "<div>Text with <span class=\"highlight\">highlighted</span> word.</div>";
    let parsed: Div = facet_html::from_str(html).expect("parse");
    let serialized = facet_html::to_string(&parsed).expect("serialize");
    assert_eq!(html, serialized, "Canonical HTML should roundtrip exactly");
}

// --- Block/inline boundaries ---

#[test]
fn canonical_inline_at_start_of_block() {
    let html = "<p><strong>Bold</strong> then text.</p>";
    let parsed: P = facet_html::from_str(html).expect("parse");
    let serialized = facet_html::to_string(&parsed).expect("serialize");
    assert_eq!(html, serialized, "Canonical HTML should roundtrip exactly");
}

#[test]
fn canonical_inline_at_end_of_block() {
    let html = "<p>Text then <em>italic</em></p>";
    let parsed: P = facet_html::from_str(html).expect("parse");
    let serialized = facet_html::to_string(&parsed).expect("serialize");
    assert_eq!(html, serialized, "Canonical HTML should roundtrip exactly");
}

#[test]
fn canonical_only_inline_in_block() {
    let html = "<p><a href=\"#\">just a link</a></p>";
    let parsed: P = facet_html::from_str(html).expect("parse");
    let serialized = facet_html::to_string(&parsed).expect("serialize");
    assert_eq!(html, serialized, "Canonical HTML should roundtrip exactly");
}

#[test]
fn canonical_adjacent_inline_no_text() {
    let html = "<p><strong>bold</strong><em>italic</em></p>";
    let parsed: P = facet_html::from_str(html).expect("parse");
    let serialized = facet_html::to_string(&parsed).expect("serialize");
    assert_eq!(html, serialized, "Canonical HTML should roundtrip exactly");
}

#[test]
fn canonical_inline_with_whitespace_between() {
    let html = "<p><b>a</b> <i>b</i></p>";
    let parsed: P = facet_html::from_str(html).expect("parse");
    let serialized = facet_html::to_string(&parsed).expect("serialize");
    assert_eq!(html, serialized, "Canonical HTML should roundtrip exactly");
}

// --- Void inline elements ---

#[test]
fn canonical_img_in_text() {
    let html = "<p>See <img src=\"x.png\" alt=\"img\"> here.</p>";
    let parsed: P = facet_html::from_str(html).expect("parse");
    let serialized = facet_html::to_string(&parsed).expect("serialize");
    assert_eq!(html, serialized, "Canonical HTML should roundtrip exactly");
}

#[test]
fn canonical_br_in_text() {
    let html = "<p>Line one<br>Line two</p>";
    let parsed: P = facet_html::from_str(html).expect("parse");
    let serialized = facet_html::to_string(&parsed).expect("serialize");
    assert_eq!(html, serialized, "Canonical HTML should roundtrip exactly");
}

#[test]
fn canonical_input_in_label() {
    let html = "<label>Name: <input type=\"text\"></label>";
    let parsed: Label = facet_html::from_str(html).expect("parse");
    let serialized = facet_html::to_string(&parsed).expect("serialize");
    assert_eq!(html, serialized, "Canonical HTML should roundtrip exactly");
}

// --- Nesting depth ---

#[test]
fn canonical_deeply_nested_inline() {
    let html = "<p><a href=\"#\"><strong><em><code>deep</code></em></strong></a></p>";
    let parsed: P = facet_html::from_str(html).expect("parse");
    let serialized = facet_html::to_string(&parsed).expect("serialize");
    assert_eq!(html, serialized, "Canonical HTML should roundtrip exactly");
}

#[test]
fn canonical_inline_in_inline_in_block() {
    let html = "<div><span><a href=\"#\">link</a></span></div>";
    let parsed: Div = facet_html::from_str(html).expect("parse");
    let serialized = facet_html::to_string(&parsed).expect("serialize");
    assert_eq!(html, serialized, "Canonical HTML should roundtrip exactly");
}

// --- Different block containers ---

#[test]
fn canonical_inline_in_heading() {
    let html = "<h1>Title with <code>code</code></h1>";
    let parsed: H1 = facet_html::from_str(html).expect("parse");
    let serialized = facet_html::to_string(&parsed).expect("serialize");
    assert_eq!(html, serialized, "Canonical HTML should roundtrip exactly");
}

#[test]
fn canonical_inline_in_blockquote() {
    let html = "<blockquote>Quote with <cite>citation</cite></blockquote>";
    let parsed: Blockquote = facet_html::from_str(html).expect("parse");
    let serialized = facet_html::to_string(&parsed).expect("serialize");
    assert_eq!(html, serialized, "Canonical HTML should roundtrip exactly");
}

#[test]
fn canonical_inline_in_td() {
    let html = "<td>Cell with <strong>bold</strong></td>";
    let parsed: Td = facet_html::from_str(html).expect("parse");
    let serialized = facet_html::to_string(&parsed).expect("serialize");
    assert_eq!(html, serialized, "Canonical HTML should roundtrip exactly");
}

#[test]
fn canonical_inline_in_figcaption() {
    let html = "<figcaption>Caption <em>emphasized</em></figcaption>";
    let parsed: Figcaption = facet_html::from_str(html).expect("parse");
    let serialized = facet_html::to_string(&parsed).expect("serialize");
    assert_eq!(html, serialized, "Canonical HTML should roundtrip exactly");
}

// --- List edge cases ---

#[test]
fn canonical_li_starting_with_inline() {
    let html = "<li><code>cmd</code> - description</li>";
    let parsed: Li = facet_html::from_str(html).expect("parse");
    let serialized = facet_html::to_string(&parsed).expect("serialize");
    assert_eq!(html, serialized, "Canonical HTML should roundtrip exactly");
}

#[test]
fn canonical_li_ending_with_inline() {
    let html = "<li>description - <code>cmd</code></li>";
    let parsed: Li = facet_html::from_str(html).expect("parse");
    let serialized = facet_html::to_string(&parsed).expect("serialize");
    assert_eq!(html, serialized, "Canonical HTML should roundtrip exactly");
}

// --- Custom elements (syntax highlighting) ---

#[test]
fn canonical_custom_inline_elements() {
    let html = "<code><a-k>fn</a-k> <a-f>main</a-f>()</code>";
    let parsed: Code = facet_html::from_str(html).expect("parse");
    let serialized = facet_html::to_string(&parsed).expect("serialize");
    assert_eq!(html, serialized, "Canonical HTML should roundtrip exactly");
}

#[test]
fn canonical_mixed_custom_and_standard() {
    let html = "<p>The <a-k>keyword</a-k> and <code>code</code></p>";
    let parsed: P = facet_html::from_str(html).expect("parse");
    let serialized = facet_html::to_string(&parsed).expect("serialize");
    assert_eq!(html, serialized, "Canonical HTML should roundtrip exactly");
}

// --- Whitespace sensitive contexts ---

#[test]
fn canonical_pre_with_inline_spans() {
    let html = "<pre><code><span>line1</span>\n<span>line2</span></code></pre>";
    let parsed: Pre = facet_html::from_str(html).expect("parse");
    let serialized = facet_html::to_string(&parsed).expect("serialize");
    assert_eq!(html, serialized, "Canonical HTML should roundtrip exactly");
}

// Issue #1744: Stray spaces inside inline elements after roundtrip
// When parsing and serializing HTML, spaces are being added inside elements:
// - `<h2>Title</h2>` becomes `<h2> Title </h2>`
// - `<code>Value</code>` becomes `<code>Value </code>`
// - `<a href="#">Link</a>` gets newlines/spaces inside
use facet_html_dom::{A, H2};

#[test]
fn issue_1744_no_stray_spaces_in_h2() {
    let html = "<h2>Start here</h2>";
    let parsed: H2 = facet_html::from_str(html).expect("parse");
    let serialized = facet_html::to_string(&parsed).expect("serialize");

    // Should NOT have spaces inside: `<h2> Start here </h2>`
    assert!(
        !serialized.contains("> "),
        "Should not have space after opening tag, got: {}",
        serialized
    );
    assert!(
        !serialized.contains(" </"),
        "Should not have space before closing tag, got: {}",
        serialized
    );
    assert_eq!(html, serialized, "Canonical HTML should roundtrip exactly");
}

#[test]
fn issue_1744_no_stray_spaces_in_h2_pretty() {
    let html = "<h2>Start here</h2>";
    let parsed: H2 = facet_html::from_str(html).expect("parse");
    let serialized = facet_html::to_string_pretty(&parsed).expect("serialize");

    // Should NOT have newlines/spaces inside: `<h2> Start here </h2>` or `<h2>\nStart here\n</h2>`
    assert!(
        !serialized.contains(">\n") && !serialized.contains("> "),
        "Should not have space/newline after opening tag, got: {}",
        serialized
    );
    assert!(
        !serialized.contains("\n</") && !serialized.contains(" </"),
        "Should not have space/newline before closing tag, got: {}",
        serialized
    );
    // The output should match the input exactly (no extra whitespace)
    assert_eq!(
        html, serialized,
        "Pretty print of text-only element should match input"
    );
}

#[test]
fn issue_1744_no_stray_spaces_in_h2_with_id() {
    let html = "<h2 id=\"start-here\">Start here</h2>";
    let parsed: H2 = facet_html::from_str(html).expect("parse");
    let serialized = facet_html::to_string(&parsed).expect("serialize");

    assert!(
        !serialized.contains("> "),
        "Should not have space after opening tag, got: {}",
        serialized
    );
    assert!(
        !serialized.contains(" </"),
        "Should not have space before closing tag, got: {}",
        serialized
    );
    assert_eq!(html, serialized, "Canonical HTML should roundtrip exactly");
}

#[test]
fn issue_1744_no_stray_spaces_in_code() {
    let html = "<code>Value</code>";
    let parsed: Code = facet_html::from_str(html).expect("parse");
    let serialized = facet_html::to_string(&parsed).expect("serialize");

    assert!(
        !serialized.contains("Value </code>"),
        "Should not have trailing space in code, got: {}",
        serialized
    );
    assert_eq!(html, serialized, "Canonical HTML should roundtrip exactly");
}

#[test]
fn issue_1744_no_stray_spaces_in_link() {
    let html = "<a href=\"#\">Link text</a>";
    let parsed: A = facet_html::from_str(html).expect("parse");
    let serialized = facet_html::to_string(&parsed).expect("serialize");

    // Should NOT have newlines or extra spaces inside
    assert!(
        !serialized.contains(">\n"),
        "Should not have newline after opening tag, got: {}",
        serialized
    );
    assert!(
        !serialized.contains("\n</"),
        "Should not have newline before closing tag, got: {}",
        serialized
    );
    assert!(
        !serialized.contains("> "),
        "Should not have space after opening tag, got: {}",
        serialized
    );
    assert!(
        !serialized.contains(" </"),
        "Should not have space before closing tag, got: {}",
        serialized
    );
    assert_eq!(html, serialized, "Canonical HTML should roundtrip exactly");
}

#[test]
fn issue_1744_li_with_multiple_inline_elements() {
    // This is the exact pattern from the user's screenshot
    let html = "<li><a href=\"@/guide/dynamic-values.md\">Dynamic Values</a> _ <code>Value</code>, <code>assert_same!</code>, <code>RawJson</code></li>";
    let parsed: Li = facet_html::from_str(html).expect("parse");
    let serialized = facet_html::to_string(&parsed).expect("serialize");

    // Check no stray spaces inside elements
    assert!(
        !serialized.contains("Value </code>"),
        "Should not have trailing space in code, got: {}",
        serialized
    );
    assert!(
        !serialized.contains("assert_same! </code>"),
        "Should not have trailing space in code, got: {}",
        serialized
    );
    assert!(
        !serialized.contains("RawJson </code>"),
        "Should not have trailing space in code, got: {}",
        serialized
    );
    assert!(
        !serialized.contains("> Dynamic"),
        "Should not have space after link opening tag, got: {}",
        serialized
    );
}

// Verify that pretty-printing DOES add newlines between block-level siblings (the correct case)
#[test]
fn issue_1744_pretty_does_add_newlines_between_blocks() {
    // A div with multiple block children - newlines between them ARE correct
    let html = "<div><p>First</p><p>Second</p></div>";
    let parsed: Div = facet_html::from_str(html).expect("parse");
    let serialized = facet_html::to_string_pretty(&parsed).expect("serialize");

    // Should have newlines between block children
    assert!(
        serialized.contains("</p>\n"),
        "Should have newline after block elements, got: {}",
        serialized
    );
    // But NOT inside the p elements themselves (they only contain text)
    assert!(
        !serialized.contains("<p>\n"),
        "Should NOT have newline inside p elements, got: {}",
        serialized
    );
}

/// Roundtrip test for G115 fixture - catches issues like aria-label becoming ariaLabel
#[test]
fn g115_roundtrip() {
    use std::path::Path;
    let path = Path::new("tests/fixtures/https_w3.org_WAI_WCAG21_Techniques_general_G115.html");
    let html_str = std::fs::read_to_string(path).unwrap();

    let parsed: facet_html_dom::Html = facet_html::from_str(&html_str).expect("parse 1");
    let serialized = facet_html::to_string(&parsed).expect("serialize 1");

    let reparsed: facet_html_dom::Html = facet_html::from_str(&serialized).expect("parse 2");
    let reserialized = facet_html::to_string(&reparsed).expect("serialize 2");

    assert_eq!(serialized, reserialized, "Roundtrip should be idempotent");
}

/// Issue: aria-label and other hyphenated attributes should roundtrip exactly.
/// The `extra` HashMap in GlobalAttrs captures unknown attributes with their
/// original keys - we must NOT apply lowerCamelCase transformation to string keys.
#[test]
fn issue_aria_label_roundtrip() {
    let input = r#"<div aria-label="Close" data-foo="bar">Content</div>"#;

    let parsed: facet_html_dom::Div = facet_html::from_str(input).expect("parse");
    let serialized = facet_html::to_string(&parsed).expect("serialize");

    // The hyphenated attribute names must be preserved exactly
    assert!(
        serialized.contains(r#"aria-label="Close""#),
        "aria-label should be preserved exactly, got: {}",
        serialized
    );
    assert!(
        serialized.contains(r#"data-foo="bar""#),
        "data-foo should be preserved exactly, got: {}",
        serialized
    );

    // Roundtrip should be stable
    let reparsed: facet_html_dom::Div = facet_html::from_str(&serialized).expect("reparse");
    let reserialized = facet_html::to_string(&reparsed).expect("reserialize");

    assert_eq!(serialized, reserialized, "Roundtrip should be idempotent");
}

// Issue: DlContent enum lacks Text variant for whitespace between dt/dd elements
// When parsing HTML like <dl>\n<dt>...</dt>\n<dd>...</dd>\n</dl>, the whitespace
// between elements cannot be deserialized because DlContent only has Dt and Dd variants.
#[test]
fn issue_dl_content_text_variant() {
    use facet_html_dom::Dl;

    // Simple dl with whitespace between elements (common in real HTML)
    let html = r#"<dl>
<dt>Term</dt>
<dd>Definition</dd>
</dl>"#;

    let result = facet_html::from_str::<Dl>(html);
    assert!(
        result.is_ok(),
        "DlContent should handle text/whitespace between dt/dd: {:?}",
        result.err()
    );
}

#[test]
fn issue_dl_content_in_full_document() {
    // This is the pattern from the args showcase page that fails
    let html = r#"<!DOCTYPE html>
<html lang="en">
<head><title>Test</title></head>
<body>
<dl>
<dt>Source</dt><dd>Test</dd>
</dl>
</body>
</html>"#;

    let result = facet_html::from_str::<Html>(html);
    assert!(
        result.is_ok(),
        "Full document with dl should parse: {:?}",
        result.err()
    );
}

#[test]
fn issue_tr_content_text_variant() {
    use facet_html_dom::Tr;

    // Text between th/td cells - should this work?
    let html = r#"<tr><th>A</th>WOOPS<th>B</th></tr>"#;

    let result = facet_html::from_str::<Tr>(html);
    assert!(
        result.is_ok(),
        "Tr should handle text between cells: {:?}",
        result.err()
    );
}

// ============================================================================
// Devilish HTML corner cases - text in unexpected places
// ============================================================================

#[test]
fn lenient_text_in_dl_is_discarded() {
    use facet_html_dom::Dl;

    // Whitespace AND actual text between dt/dd - all should be discarded
    let html = r#"<dl>
        RANDOM TEXT HERE
        <dt>Term</dt>
        more garbage
        <dd>Definition</dd>
        trailing nonsense
    </dl>"#;

    let dl: Dl = facet_html::from_str(html).expect("should parse despite garbage text");
    assert_eq!(
        dl.children.len(),
        2,
        "should have exactly 2 children (dt + dd)"
    );
}

#[test]
fn lenient_text_in_table_structure() {
    // Text in places it shouldn't be: between rows, between cells
    let html = r#"<!DOCTYPE html>
<html>
<head><title>Test</title></head>
<body>
<table>
    GARBAGE IN TABLE
    <tr>
        TEXT IN ROW
        <td>Cell 1</td>
        MORE TEXT
        <td>Cell 2</td>
    </tr>
    BETWEEN ROWS
    <tr>
        <th>Header</th>
    </tr>
</table>
</body>
</html>"#;

    let doc: Html = facet_html::from_str(html).expect("should parse table with garbage text");
    // Just verify it parses - the text is silently discarded
    assert!(doc.body.is_some());
}

#[test]
fn lenient_text_in_select() {
    // Text between options in a select
    let html = r#"<!DOCTYPE html>
<html>
<head><title>Test</title></head>
<body>
<select>
    WHAT IS THIS
    <option>One</option>
    DOING HERE
    <option>Two</option>
    ???
</select>
</body>
</html>"#;

    let doc: Html = facet_html::from_str(html).expect("should parse select with garbage text");
    assert!(doc.body.is_some());
}

#[test]
fn lenient_text_in_ul_ol() {
    // Text directly in ul/ol (not in li)
    let html = r#"<!DOCTYPE html>
<html>
<head><title>Test</title></head>
<body>
<ul>
    BARE TEXT IN UL
    <li>Item 1</li>
    MORE BARE TEXT
    <li>Item 2</li>
</ul>
<ol>
    BARE TEXT IN OL
    <li>First</li>
</ol>
</body>
</html>"#;

    let doc: Html = facet_html::from_str(html).expect("should parse lists with garbage text");
    assert!(doc.body.is_some());
}

#[test]
fn lenient_deeply_nested_garbage() {
    // Garbage text at multiple nesting levels
    let html = r#"<!DOCTYPE html>
<html>
GARBAGE AT HTML LEVEL - wait this is actually in body
<head><title>Test</title></head>
<body>
    <div>
        <dl>
            TEXT IN DL
            <dt>
                <dl>
                    NESTED DL TEXT
                    <dt>Inner term</dt>
                    <dd>Inner def</dd>
                </dl>
            </dt>
            <dd>Outer def</dd>
        </dl>
    </div>
</body>
</html>"#;

    let doc: Html = facet_html::from_str(html).expect("should handle deeply nested garbage");
    assert!(doc.body.is_some());
}

#[test]
fn lenient_mixed_valid_and_invalid_text() {
    // Mix of valid text (in p, span) and invalid text (in dl, table)
    let html = r#"<!DOCTYPE html>
<html>
<head><title>Test</title></head>
<body>
    <p>This text is valid and should be preserved.</p>
    <dl>
        This text is invalid and should be discarded.
        <dt>Term with <span>valid inline text</span></dt>
        <dd>Definition with <em>emphasis</em></dd>
    </dl>
    <p>More valid text.</p>
</body>
</html>"#;

    let doc: Html = facet_html::from_str(html).expect("should handle mixed valid/invalid text");
    assert!(doc.body.is_some());
}

#[test]
fn lenient_only_whitespace_vs_actual_text() {
    use facet_html_dom::Dl;

    // Just whitespace - common in pretty-printed HTML
    let whitespace_only = r#"<dl>
        <dt>Term</dt>
        <dd>Def</dd>
    </dl>"#;

    // Actual text content - still invalid but should be discarded
    let with_text = r#"<dl>ACTUAL TEXT<dt>Term</dt><dd>Def</dd></dl>"#;

    let dl1: Dl = facet_html::from_str(whitespace_only).expect("whitespace should parse");
    let dl2: Dl = facet_html::from_str(with_text).expect("text should also parse");

    // Both should have the same structure
    assert_eq!(dl1.children.len(), dl2.children.len());
}

#[test]
fn lenient_empty_elements_with_text() {
    // Elements that are normally empty but have text stuffed in them
    let html = r#"<!DOCTYPE html>
<html>
<head>
    <meta charset="utf-8">
    <link rel="stylesheet" href="style.css">
</head>
<body>
    <br>
    <hr>
    <img src="test.png" alt="test">
    <input type="text">
</body>
</html>"#;

    let doc: Html = facet_html::from_str(html).expect("should handle void elements");
    assert!(doc.body.is_some());
}

#[test]
fn doctype_roundtrip() {
    let html = r#"<!DOCTYPE html>
<html>
<head><title>Test</title></head>
<body><p>Hello</p></body>
</html>"#;

    let doc: Html = facet_html::from_str(html).expect("parse");
    eprintln!("doctype field: {:?}", doc.doctype);

    let serialized = facet_html::to_string(&doc).expect("serialize");
    eprintln!("serialized:\n{}", serialized);

    assert!(
        serialized.contains("<!DOCTYPE"),
        "DOCTYPE should be preserved in roundtrip"
    );
}
