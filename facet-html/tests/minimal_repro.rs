// Regression tests for GitHub issues
// Run with: cargo +nightly miri test -p facet-html --test minimal_repro

use facet_html_dom::Html;

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
