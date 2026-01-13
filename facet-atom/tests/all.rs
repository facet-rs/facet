use facet_atom::*;
use indoc::indoc;
use test_log::test;

#[test]
fn test_parse_basic_feed() {
    let xml = indoc! {r#"
            <?xml version="1.0" encoding="utf-8"?>
            <feed xmlns="http://www.w3.org/2005/Atom">
                <title>Example Feed</title>
                <id>urn:uuid:60a76c80-d399-11d9-b93C-0003939e0af6</id>
                <updated>2003-12-13T18:30:02Z</updated>
                <author>
                    <name>John Doe</name>
                </author>
                <link href="http://example.org/"/>
            </feed>
        "#};

    let feed: Feed = from_str(xml).unwrap();

    assert_eq!(
        feed.id.as_deref(),
        Some("urn:uuid:60a76c80-d399-11d9-b93C-0003939e0af6")
    );
    assert_eq!(
        feed.title.as_ref().and_then(|t| t.content.as_deref()),
        Some("Example Feed")
    );
    assert_eq!(feed.updated.as_deref(), Some("2003-12-13T18:30:02Z"));
    assert_eq!(feed.authors.len(), 1);
    assert_eq!(
        feed.authors.first().and_then(|a| a.name.as_deref()),
        Some("John Doe")
    );
    assert_eq!(feed.links.len(), 1);
    assert_eq!(
        feed.links.first().and_then(|l| l.href.as_deref()),
        Some("http://example.org/")
    );
}

#[test]
fn test_parse_feed_with_entries() {
    let xml = indoc! {r#"
            <?xml version="1.0" encoding="utf-8"?>
            <feed xmlns="http://www.w3.org/2005/Atom">
                <title>Example Feed</title>
                <id>urn:uuid:60a76c80-d399-11d9-b93C-0003939e0af6</id>
                <updated>2003-12-13T18:30:02Z</updated>
                <entry>
                    <title>Atom-Powered Robots Run Amok</title>
                    <id>urn:uuid:1225c695-cfb8-4ebb-aaaa-80da344efa6a</id>
                    <updated>2003-12-13T18:30:02Z</updated>
                    <link href="http://example.org/2003/12/13/atom03"/>
                    <summary>Some text.</summary>
                </entry>
            </feed>
        "#};

    let feed: Feed = from_str(xml).unwrap();

    assert_eq!(feed.entries.len(), 1);
    let entry = &feed.entries[0];
    assert_eq!(
        entry.title.as_ref().and_then(|t| t.content.as_deref()),
        Some("Atom-Powered Robots Run Amok")
    );
    assert_eq!(
        entry.id.as_deref(),
        Some("urn:uuid:1225c695-cfb8-4ebb-aaaa-80da344efa6a")
    );
    assert_eq!(
        entry.summary.as_ref().and_then(|s| s.content.as_deref()),
        Some("Some text.")
    );
}

#[test]
fn test_parse_entry_with_content() {
    let xml = indoc! {r#"
            <?xml version="1.0" encoding="utf-8"?>
            <feed xmlns="http://www.w3.org/2005/Atom">
                <title>Test</title>
                <id>test:feed</id>
                <updated>2024-01-01T00:00:00Z</updated>
                <entry>
                    <title>Test Entry</title>
                    <id>test:entry:1</id>
                    <updated>2024-01-01T00:00:00Z</updated>
                    <content type="html">&lt;p&gt;Hello, World!&lt;/p&gt;</content>
                </entry>
            </feed>
        "#};

    let feed: Feed = from_str(xml).unwrap();
    let entry = &feed.entries[0];
    let content = entry.content.as_ref().unwrap();

    assert_eq!(content.content_type.as_deref(), Some("html"));
    assert_eq!(content.body.as_deref(), Some("<p>Hello, World!</p>"));
}

#[test]
fn test_parse_link_attributes() {
    let xml = indoc! {r#"
            <?xml version="1.0" encoding="utf-8"?>
            <feed xmlns="http://www.w3.org/2005/Atom">
                <title>Test</title>
                <id>test:feed</id>
                <updated>2024-01-01T00:00:00Z</updated>
                <link href="http://example.org/" rel="alternate" type="text/html" hreflang="en" title="Example"/>
                <link href="http://example.org/feed.atom" rel="self" type="application/atom+xml"/>
            </feed>
        "#};

    let feed: Feed = from_str(xml).unwrap();

    assert_eq!(feed.links.len(), 2);

    let alternate = &feed.links[0];
    assert_eq!(alternate.href.as_deref(), Some("http://example.org/"));
    assert_eq!(alternate.rel.as_deref(), Some("alternate"));
    assert_eq!(alternate.media_type.as_deref(), Some("text/html"));
    assert_eq!(alternate.hreflang.as_deref(), Some("en"));
    assert_eq!(alternate.title.as_deref(), Some("Example"));

    let self_link = &feed.links[1];
    assert_eq!(
        self_link.href.as_deref(),
        Some("http://example.org/feed.atom")
    );
    assert_eq!(self_link.rel.as_deref(), Some("self"));
}

#[test]
fn test_parse_category() {
    let xml = indoc! {r#"
            <?xml version="1.0" encoding="utf-8"?>
            <feed xmlns="http://www.w3.org/2005/Atom">
                <title>Test</title>
                <id>test:feed</id>
                <updated>2024-01-01T00:00:00Z</updated>
                <category term="technology" scheme="http://example.org/categories" label="Technology"/>
            </feed>
        "#};

    let feed: Feed = from_str(xml).unwrap();

    assert_eq!(feed.categories.len(), 1);
    let cat = &feed.categories[0];
    assert_eq!(cat.term.as_deref(), Some("technology"));
    assert_eq!(cat.scheme.as_deref(), Some("http://example.org/categories"));
    assert_eq!(cat.label.as_deref(), Some("Technology"));
}

#[test]
fn test_parse_generator() {
    let xml = indoc! {r#"
            <?xml version="1.0" encoding="utf-8"?>
            <feed xmlns="http://www.w3.org/2005/Atom">
                <title>Test</title>
                <id>test:feed</id>
                <updated>2024-01-01T00:00:00Z</updated>
                <generator uri="http://example.org/generator" version="1.0">Example Generator</generator>
            </feed>
        "#};

    let feed: Feed = from_str(xml).unwrap();

    let generator = feed.generator.as_ref().unwrap();
    assert_eq!(generator.name.as_deref(), Some("Example Generator"));
    assert_eq!(
        generator.uri.as_deref(),
        Some("http://example.org/generator")
    );
    assert_eq!(generator.version.as_deref(), Some("1.0"));
}

#[test]
fn test_parse_person_full() {
    let xml = indoc! {r#"
            <?xml version="1.0" encoding="utf-8"?>
            <feed xmlns="http://www.w3.org/2005/Atom">
                <title>Test</title>
                <id>test:feed</id>
                <updated>2024-01-01T00:00:00Z</updated>
                <author>
                    <name>John Doe</name>
                    <uri>http://example.org/johndoe</uri>
                    <email>john@example.org</email>
                </author>
                <contributor>
                    <name>Jane Smith</name>
                </contributor>
            </feed>
        "#};

    let feed: Feed = from_str(xml).unwrap();

    assert_eq!(feed.authors.len(), 1);
    let author = &feed.authors[0];
    assert_eq!(author.name.as_deref(), Some("John Doe"));
    assert_eq!(author.uri.as_deref(), Some("http://example.org/johndoe"));
    assert_eq!(author.email.as_deref(), Some("john@example.org"));

    assert_eq!(feed.contributors.len(), 1);
    assert_eq!(feed.contributors[0].name.as_deref(), Some("Jane Smith"));
}

#[test]
fn test_roundtrip_simple_feed() {
    let feed = Feed {
        id: Some("urn:uuid:test".to_string()),
        title: Some(TextContent {
            content_type: None,
            content: Some("Test Feed".to_string()),
        }),
        updated: Some("2024-01-01T00:00:00Z".to_string()),
        authors: vec![Person {
            name: Some("Test Author".to_string()),
            uri: None,
            email: None,
        }],
        links: vec![Link {
            href: Some("http://example.org/".to_string()),
            rel: Some("alternate".to_string()),
            ..Default::default()
        }],
        ..Default::default()
    };

    let xml = to_string(&feed).unwrap();
    tracing::debug!("Generated XML:\n{}", xml);
    let parsed: Feed = from_str(&xml).unwrap();
    tracing::debug!("Parsed authors: {:?}", parsed.authors);

    assert_eq!(parsed.id, feed.id);
    assert_eq!(
        parsed.title.as_ref().and_then(|t| t.content.as_ref()),
        feed.title.as_ref().and_then(|t| t.content.as_ref())
    );
    assert_eq!(parsed.updated, feed.updated);
    assert_eq!(parsed.authors.len(), feed.authors.len());
}
