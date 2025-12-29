//! Real-world XML format tests for facet-xml.
//!
//! Tests various common XML formats to ensure compatibility.

use facet::Facet;
use facet_xml_legacy as xml;

// ============================================================================
// SVG - Scalable Vector Graphics
// ============================================================================

#[derive(Facet, Debug, PartialEq)]
#[facet(rename = "svg", rename_all = "camelCase")]
struct Svg {
    #[facet(default, xml::attribute)]
    width: Option<String>,
    #[facet(default, xml::attribute)]
    height: Option<String>,
    #[facet(default, xml::attribute)]
    view_box: Option<String>,
    #[facet(recursive_type, xml::elements)]
    children: Vec<SvgElement>,
}

#[derive(Facet, Debug, PartialEq)]
#[repr(C)]
enum SvgElement {
    #[facet(rename = "rect")]
    Rect(SvgRect),
    #[facet(rename = "circle")]
    Circle(SvgCircle),
    #[facet(rename = "path")]
    Path(SvgPath),
    #[facet(rename = "g")]
    Group(SvgGroup),
}

#[derive(Facet, Debug, PartialEq)]
struct SvgRect {
    #[facet(default, xml::attribute)]
    x: Option<String>,
    #[facet(default, xml::attribute)]
    y: Option<String>,
    #[facet(default, xml::attribute)]
    width: Option<String>,
    #[facet(default, xml::attribute)]
    height: Option<String>,
    #[facet(default, xml::attribute)]
    fill: Option<String>,
    #[facet(default, xml::attribute)]
    stroke: Option<String>,
}

#[derive(Facet, Debug, PartialEq)]
struct SvgCircle {
    #[facet(default, xml::attribute)]
    cx: Option<String>,
    #[facet(default, xml::attribute)]
    cy: Option<String>,
    #[facet(default, xml::attribute)]
    r: Option<String>,
    #[facet(default, xml::attribute)]
    fill: Option<String>,
}

#[derive(Facet, Debug, PartialEq)]
struct SvgPath {
    #[facet(default, xml::attribute)]
    d: Option<String>,
    #[facet(default, xml::attribute)]
    fill: Option<String>,
    #[facet(default, xml::attribute)]
    stroke: Option<String>,
}

#[derive(Facet, Debug, PartialEq)]
struct SvgGroup {
    #[facet(default, xml::attribute)]
    id: Option<String>,
    #[facet(default, xml::attribute)]
    transform: Option<String>,
    #[facet(recursive_type, xml::elements)]
    children: Vec<SvgElement>,
}

#[test]
fn test_svg_simple() {
    let svg_xml = r#"<svg width="100" height="100" xmlns="http://www.w3.org/2000/svg">
        <circle cx="50" cy="50" r="40" fill="red"/>
        <rect x="10" y="10" width="30" height="30" fill="blue"/>
    </svg>"#;

    let svg: Svg = xml::from_str(svg_xml).unwrap();
    assert_eq!(svg.width, Some("100".into()));
    assert_eq!(svg.height, Some("100".into()));
    assert_eq!(svg.children.len(), 2);
}

#[test]
fn test_svg_with_groups() {
    let svg_xml = r#"<svg viewBox="0 0 200 200">
        <g id="layer1" transform="translate(10,10)">
            <rect x="0" y="0" width="50" height="50" fill="green"/>
            <circle cx="25" cy="25" r="20" fill="yellow"/>
        </g>
        <path d="M10 10 L90 90" stroke="black"/>
    </svg>"#;

    let svg: Svg = xml::from_str(svg_xml).unwrap();
    assert_eq!(svg.children.len(), 2);
}

// ============================================================================
// Maven POM - Java Project Configuration
// ============================================================================

#[derive(Facet, Debug, PartialEq)]
#[facet(rename = "project", rename_all = "camelCase")]
struct MavenPom {
    #[facet(default, xml::element)]
    model_version: Option<String>,
    #[facet(default, xml::element)]
    group_id: String,
    #[facet(default, xml::element)]
    artifact_id: String,
    #[facet(default, xml::element)]
    version: String,
    #[facet(default, xml::element)]
    packaging: Option<String>,
    #[facet(default, xml::element)]
    name: Option<String>,
    #[facet(default, xml::element)]
    description: Option<String>,
    #[facet(default, xml::element)]
    dependencies: Option<MavenDependencies>,
}

#[derive(Facet, Debug, PartialEq)]
struct MavenDependencies {
    #[facet(xml::elements)]
    dependency: Vec<MavenDependency>,
}

#[derive(Facet, Debug, PartialEq)]
#[facet(rename = "dependency", rename_all = "camelCase")]
struct MavenDependency {
    #[facet(default, xml::element)]
    group_id: String,
    #[facet(default, xml::element)]
    artifact_id: String,
    #[facet(default, xml::element)]
    version: Option<String>,
    #[facet(default, xml::element)]
    scope: Option<String>,
}

#[test]
fn test_maven_pom() {
    let pom_xml = r#"<project xmlns="http://maven.apache.org/POM/4.0.0">
        <modelVersion>4.0.0</modelVersion>
        <groupId>com.example</groupId>
        <artifactId>my-app</artifactId>
        <version>1.0.0</version>
        <packaging>jar</packaging>
        <name>My Application</name>
        <dependencies>
            <dependency>
                <groupId>org.junit.jupiter</groupId>
                <artifactId>junit-jupiter</artifactId>
                <version>5.9.0</version>
                <scope>test</scope>
            </dependency>
            <dependency>
                <groupId>com.google.guava</groupId>
                <artifactId>guava</artifactId>
                <version>31.1-jre</version>
            </dependency>
        </dependencies>
    </project>"#;

    let pom: MavenPom = xml::from_str(pom_xml).unwrap();
    assert_eq!(pom.group_id, "com.example");
    assert_eq!(pom.artifact_id, "my-app");
    assert_eq!(pom.version, "1.0.0");
    assert_eq!(pom.packaging, Some("jar".into()));

    let deps = pom.dependencies.unwrap();
    assert_eq!(deps.dependency.len(), 2);
    assert_eq!(deps.dependency[0].artifact_id, "junit-jupiter");
    assert_eq!(deps.dependency[0].scope, Some("test".into()));
}

// ============================================================================
// RSS Feed
// ============================================================================

#[derive(Facet, Debug, PartialEq)]
#[facet(rename = "rss")]
struct RssFeed {
    #[facet(default, xml::attribute)]
    version: String,
    #[facet(default, xml::element)]
    channel: RssChannel,
}

#[derive(Facet, Debug, PartialEq, Default)]
struct RssChannel {
    #[facet(default, xml::element)]
    title: String,
    #[facet(default, xml::element)]
    link: String,
    #[facet(default, xml::element)]
    description: String,
    #[facet(default, xml::element)]
    language: Option<String>,
    #[facet(xml::elements)]
    item: Vec<RssItem>,
}

#[derive(Facet, Debug, PartialEq)]
#[facet(rename = "item", rename_all = "camelCase")]
struct RssItem {
    #[facet(default, xml::element)]
    title: String,
    #[facet(default, xml::element)]
    link: Option<String>,
    #[facet(default, xml::element)]
    description: Option<String>,
    #[facet(default, xml::element)]
    pub_date: Option<String>,
    #[facet(default, xml::element)]
    guid: Option<String>,
}

#[test]
fn test_rss_feed() {
    let rss_xml = r#"<rss version="2.0">
        <channel>
            <title>Example Blog</title>
            <link>https://example.com</link>
            <description>An example blog feed</description>
            <language>en-us</language>
            <item>
                <title>First Post</title>
                <link>https://example.com/post/1</link>
                <description>This is the first post</description>
                <pubDate>Mon, 01 Jan 2024 00:00:00 GMT</pubDate>
                <guid>post-1</guid>
            </item>
            <item>
                <title>Second Post</title>
                <link>https://example.com/post/2</link>
                <description>This is the second post</description>
            </item>
        </channel>
    </rss>"#;

    let rss: RssFeed = xml::from_str(rss_xml).unwrap();
    assert_eq!(rss.version, "2.0");
    assert_eq!(rss.channel.title, "Example Blog");
    assert_eq!(rss.channel.item.len(), 2);
    assert_eq!(rss.channel.item[0].title, "First Post");
    assert_eq!(rss.channel.item[0].guid, Some("post-1".into()));
}

// ============================================================================
// Android Manifest
// ============================================================================

#[derive(Facet, Debug, PartialEq)]
#[facet(rename = "manifest")]
struct AndroidManifest {
    #[facet(default, xml::attribute)]
    package: String,
    #[facet(xml::elements, rename = "uses-permission")]
    permissions: Vec<AndroidPermission>,
    #[facet(default, xml::element)]
    application: AndroidApplication,
}

#[derive(Facet, Debug, PartialEq)]
#[facet(rename = "uses-permission")]
struct AndroidPermission {
    #[facet(default, xml::attribute, rename = "android:name")]
    name: String,
}

#[derive(Facet, Debug, PartialEq, Default)]
struct AndroidApplication {
    #[facet(default, xml::attribute, rename = "android:label")]
    label: Option<String>,
    #[facet(default, xml::attribute, rename = "android:icon")]
    icon: Option<String>,
    #[facet(xml::elements)]
    activity: Vec<AndroidActivity>,
}

#[derive(Facet, Debug, PartialEq)]
struct AndroidActivity {
    #[facet(default, xml::attribute, rename = "android:name")]
    name: String,
    #[facet(default, xml::attribute, rename = "android:exported")]
    exported: Option<String>,
}

#[test]
fn test_android_manifest() {
    let manifest_xml = r#"<manifest package="com.example.myapp">
        <uses-permission android:name="android.permission.INTERNET"/>
        <uses-permission android:name="android.permission.CAMERA"/>
        <application android:label="My App" android:icon="@mipmap/ic_launcher">
            <activity android:name=".MainActivity" android:exported="true"/>
            <activity android:name=".SettingsActivity"/>
        </application>
    </manifest>"#;

    let manifest: AndroidManifest = xml::from_str(manifest_xml).unwrap();
    assert_eq!(manifest.package, "com.example.myapp");
    assert_eq!(manifest.permissions.len(), 2);
    assert_eq!(manifest.permissions[0].name, "android.permission.INTERNET");
    assert_eq!(manifest.application.label, Some("My App".into()));
    assert_eq!(manifest.application.activity.len(), 2);
    assert_eq!(manifest.application.activity[0].name, ".MainActivity");
}

// ============================================================================
// Atom Feed
// ============================================================================

#[derive(Facet, Debug, PartialEq)]
#[facet(rename = "feed")]
struct AtomFeed {
    #[facet(default, xml::element)]
    title: String,
    #[facet(default, xml::element)]
    id: String,
    #[facet(default, xml::element)]
    updated: String,
    #[facet(default, xml::element)]
    author: Option<AtomAuthor>,
    #[facet(xml::elements)]
    entry: Vec<AtomEntry>,
}

#[derive(Facet, Debug, PartialEq)]
struct AtomAuthor {
    #[facet(default, xml::element)]
    name: String,
    #[facet(default, xml::element)]
    email: Option<String>,
    #[facet(default, xml::element)]
    uri: Option<String>,
}

#[derive(Facet, Debug, PartialEq)]
#[facet(rename = "entry")]
struct AtomEntry {
    #[facet(default, xml::element)]
    title: String,
    #[facet(default, xml::element)]
    id: String,
    #[facet(default, xml::element)]
    updated: String,
    #[facet(default, xml::element)]
    summary: Option<String>,
    #[facet(default, xml::element)]
    content: Option<AtomContent>,
}

#[derive(Facet, Debug, PartialEq)]
struct AtomContent {
    #[facet(default, xml::attribute, rename = "type")]
    content_type: Option<String>,
    #[facet(xml::text)]
    text: String,
}

#[test]
fn test_atom_feed() {
    let atom_xml = r#"<feed xmlns="http://www.w3.org/2005/Atom">
        <title>Example Feed</title>
        <id>urn:uuid:60a76c80-d399-11d9-b93C-0003939e0af6</id>
        <updated>2024-01-01T00:00:00Z</updated>
        <author>
            <name>John Doe</name>
            <email>john@example.com</email>
        </author>
        <entry>
            <title>Atom Entry 1</title>
            <id>urn:uuid:1225c695-cfb8-4ebb-aaaa-80da344efa6a</id>
            <updated>2024-01-01T00:00:00Z</updated>
            <summary>Summary of entry 1</summary>
            <content type="html">Full content here</content>
        </entry>
    </feed>"#;

    let feed: AtomFeed = xml::from_str(atom_xml).unwrap();
    assert_eq!(feed.title, "Example Feed");
    assert!(feed.author.is_some());
    assert_eq!(feed.author.as_ref().unwrap().name, "John Doe");
    assert_eq!(feed.entry.len(), 1);
    assert_eq!(feed.entry[0].title, "Atom Entry 1");

    let content = feed.entry[0].content.as_ref().unwrap();
    assert_eq!(content.content_type, Some("html".into()));
    assert_eq!(content.text, "Full content here");
}

// ============================================================================
// OpenDocument Manifest (META-INF/manifest.xml)
// ============================================================================

#[derive(Facet, Debug, PartialEq)]
#[facet(rename = "manifest:manifest")]
struct OdfManifest {
    #[facet(default, xml::attribute, rename = "manifest:version")]
    version: Option<String>,
    #[facet(xml::elements, rename = "manifest:file-entry")]
    file_entries: Vec<OdfFileEntry>,
}

#[derive(Facet, Debug, PartialEq)]
#[facet(rename = "manifest:file-entry")]
struct OdfFileEntry {
    #[facet(default, xml::attribute, rename = "manifest:full-path")]
    full_path: String,
    #[facet(default, xml::attribute, rename = "manifest:media-type")]
    media_type: String,
}

#[test]
fn test_odf_manifest() {
    let manifest_xml = r#"<manifest:manifest manifest:version="1.2">
        <manifest:file-entry manifest:full-path="/" manifest:media-type="application/vnd.oasis.opendocument.text"/>
        <manifest:file-entry manifest:full-path="content.xml" manifest:media-type="text/xml"/>
        <manifest:file-entry manifest:full-path="styles.xml" manifest:media-type="text/xml"/>
        <manifest:file-entry manifest:full-path="meta.xml" manifest:media-type="text/xml"/>
    </manifest:manifest>"#;

    let manifest: OdfManifest = xml::from_str(manifest_xml).unwrap();
    assert_eq!(manifest.version, Some("1.2".into()));
    assert_eq!(manifest.file_entries.len(), 4);
    assert_eq!(manifest.file_entries[0].full_path, "/");
    assert_eq!(
        manifest.file_entries[0].media_type,
        "application/vnd.oasis.opendocument.text"
    );
}

// ============================================================================
// XHTML
// ============================================================================

#[derive(Facet, Debug, PartialEq)]
#[facet(rename = "html")]
struct XhtmlDocument {
    #[facet(default, xml::attribute)]
    lang: Option<String>,
    #[facet(default, xml::element)]
    head: XhtmlHead,
    #[facet(default, xml::element)]
    body: XhtmlBody,
}

#[derive(Facet, Debug, PartialEq, Default)]
struct XhtmlHead {
    #[facet(default, xml::element)]
    title: String,
    #[facet(xml::elements)]
    meta: Vec<XhtmlMeta>,
    #[facet(xml::elements)]
    link: Vec<XhtmlLink>,
}

#[derive(Facet, Debug, PartialEq)]
#[facet(rename = "meta")]
struct XhtmlMeta {
    #[facet(default, xml::attribute)]
    charset: Option<String>,
    #[facet(default, xml::attribute)]
    name: Option<String>,
    #[facet(default, xml::attribute)]
    content: Option<String>,
}

#[derive(Facet, Debug, PartialEq)]
#[facet(rename = "link")]
struct XhtmlLink {
    #[facet(default, xml::attribute)]
    rel: String,
    #[facet(default, xml::attribute)]
    href: String,
    #[facet(default, xml::attribute, rename = "type")]
    link_type: Option<String>,
}

#[derive(Facet, Debug, PartialEq, Default)]
struct XhtmlBody {
    #[facet(default, xml::attribute)]
    class: Option<String>,
    #[facet(recursive_type, xml::elements)]
    children: Vec<XhtmlBodyElement>,
}

#[derive(Facet, Debug, PartialEq)]
#[repr(C)]
enum XhtmlBodyElement {
    #[facet(rename = "h1")]
    H1(XhtmlTextElement),
    #[facet(rename = "p")]
    P(XhtmlTextElement),
    #[facet(rename = "div")]
    Div(XhtmlDiv),
}

#[derive(Facet, Debug, PartialEq)]
struct XhtmlTextElement {
    #[facet(default, xml::attribute)]
    class: Option<String>,
    #[facet(default, xml::attribute)]
    id: Option<String>,
    #[facet(xml::text)]
    text: String,
}

#[derive(Facet, Debug, PartialEq)]
struct XhtmlDiv {
    #[facet(default, xml::attribute)]
    class: Option<String>,
    #[facet(default, xml::attribute)]
    id: Option<String>,
    #[facet(recursive_type, xml::elements)]
    children: Vec<XhtmlBodyElement>,
}

#[test]
fn test_xhtml() {
    let xhtml = r#"<html xmlns="http://www.w3.org/1999/xhtml" lang="en">
        <head>
            <title>Test Page</title>
            <meta charset="UTF-8"/>
            <meta name="description" content="A test page"/>
            <link rel="stylesheet" href="style.css" type="text/css"/>
        </head>
        <body class="main">
            <h1 id="title">Welcome</h1>
            <p class="intro">This is a paragraph.</p>
            <div class="content">
                <p>Nested paragraph</p>
            </div>
        </body>
    </html>"#;

    let doc: XhtmlDocument = xml::from_str(xhtml).unwrap();
    assert_eq!(doc.lang, Some("en".into()));
    assert_eq!(doc.head.title, "Test Page");
    assert_eq!(doc.head.meta.len(), 2);
    assert_eq!(doc.head.link.len(), 1);
    assert_eq!(doc.body.class, Some("main".into()));
    assert_eq!(doc.body.children.len(), 3);
}

// ============================================================================
// Sitemap XML
// ============================================================================

#[derive(Facet, Debug, PartialEq)]
#[facet(rename = "urlset")]
struct Sitemap {
    #[facet(xml::elements)]
    url: Vec<SitemapUrl>,
}

#[derive(Facet, Debug, PartialEq)]
#[facet(rename = "url")]
struct SitemapUrl {
    #[facet(default, xml::element)]
    loc: String,
    #[facet(default, xml::element)]
    lastmod: Option<String>,
    #[facet(default, xml::element)]
    changefreq: Option<String>,
    #[facet(default, xml::element)]
    priority: Option<String>,
}

#[test]
fn test_sitemap() {
    let sitemap_xml = r#"<urlset xmlns="http://www.sitemaps.org/schemas/sitemap/0.9">
        <url>
            <loc>https://example.com/</loc>
            <lastmod>2024-01-01</lastmod>
            <changefreq>daily</changefreq>
            <priority>1.0</priority>
        </url>
        <url>
            <loc>https://example.com/about</loc>
            <lastmod>2024-01-01</lastmod>
            <changefreq>monthly</changefreq>
            <priority>0.8</priority>
        </url>
    </urlset>"#;

    let sitemap: Sitemap = xml::from_str(sitemap_xml).unwrap();
    assert_eq!(sitemap.url.len(), 2);
    assert_eq!(sitemap.url[0].loc, "https://example.com/");
    assert_eq!(sitemap.url[0].priority, Some("1.0".into()));
    assert_eq!(sitemap.url[1].changefreq, Some("monthly".into()));
}

// ============================================================================
// GPX (GPS Exchange Format)
// ============================================================================

#[derive(Facet, Debug, PartialEq)]
#[facet(rename = "gpx")]
struct GpxFile {
    #[facet(default, xml::attribute)]
    version: String,
    #[facet(default, xml::attribute)]
    creator: Option<String>,
    #[facet(default, xml::element)]
    metadata: Option<GpxMetadata>,
    #[facet(xml::elements)]
    wpt: Vec<GpxWaypoint>,
    #[facet(xml::elements)]
    trk: Vec<GpxTrack>,
}

#[derive(Facet, Debug, PartialEq)]
struct GpxMetadata {
    #[facet(default, xml::element)]
    name: Option<String>,
    #[facet(default, xml::element)]
    desc: Option<String>,
    #[facet(default, xml::element)]
    author: Option<GpxAuthor>,
    #[facet(default, xml::element)]
    time: Option<String>,
}

#[derive(Facet, Debug, PartialEq)]
struct GpxAuthor {
    #[facet(default, xml::element)]
    name: String,
}

#[derive(Facet, Debug, PartialEq)]
struct GpxWaypoint {
    #[facet(default, xml::attribute)]
    lat: String,
    #[facet(default, xml::attribute)]
    lon: String,
    #[facet(default, xml::element)]
    ele: Option<String>,
    #[facet(default, xml::element)]
    name: Option<String>,
    #[facet(default, xml::element)]
    desc: Option<String>,
}

#[derive(Facet, Debug, PartialEq)]
struct GpxTrack {
    #[facet(default, xml::element)]
    name: Option<String>,
    #[facet(xml::elements)]
    trkseg: Vec<GpxTrackSegment>,
}

#[derive(Facet, Debug, PartialEq)]
struct GpxTrackSegment {
    #[facet(xml::elements)]
    trkpt: Vec<GpxWaypoint>,
}

#[test]
fn test_gpx() {
    let gpx_xml = r#"<gpx version="1.1" creator="facet-xml">
        <metadata>
            <name>Morning Run</name>
            <desc>A nice morning run</desc>
            <author><name>Runner</name></author>
            <time>2024-01-01T06:00:00Z</time>
        </metadata>
        <wpt lat="45.0" lon="-122.0">
            <ele>100</ele>
            <name>Start</name>
        </wpt>
        <trk>
            <name>Track 1</name>
            <trkseg>
                <trkpt lat="45.0" lon="-122.0"><ele>100</ele></trkpt>
                <trkpt lat="45.001" lon="-122.001"><ele>101</ele></trkpt>
                <trkpt lat="45.002" lon="-122.002"><ele>102</ele></trkpt>
            </trkseg>
        </trk>
    </gpx>"#;

    let gpx: GpxFile = xml::from_str(gpx_xml).unwrap();
    assert_eq!(gpx.version, "1.1");
    assert_eq!(gpx.creator, Some("facet-xml".into()));
    assert!(gpx.metadata.is_some());
    assert_eq!(
        gpx.metadata.as_ref().unwrap().name,
        Some("Morning Run".into())
    );
    assert_eq!(gpx.wpt.len(), 1);
    assert_eq!(gpx.wpt[0].lat, "45.0");
    assert_eq!(gpx.trk.len(), 1);
    assert_eq!(gpx.trk[0].trkseg[0].trkpt.len(), 3);
}
