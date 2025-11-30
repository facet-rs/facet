use crate::db::{Heading, Page, Section, SiteTree};
use crate::template::{Context, Engine, InMemoryLoader, Value};
use crate::types::Route;
use std::collections::HashMap;

/// Options for rendering
#[derive(Default, Clone, Copy)]
pub struct RenderOptions {
    /// Whether to inject live reload script
    pub livereload: bool,
    /// Development mode - show error pages instead of failing
    pub dev_mode: bool,
}

/// Inject livereload script if enabled
pub fn inject_livereload(html: &str, options: RenderOptions) -> String {
    if options.livereload {
        let livereload_script = r##"<script>
(function() {
    const protocol = window.location.protocol === 'https:' ? 'wss:' : 'ws:';
    const wsUrl = protocol + '//' + window.location.host + '/__livereload';
    let ws;
    let reconnectTimer;

    function connect() {
        ws = new WebSocket(wsUrl);
        ws.onopen = function() { console.log('[livereload] connected'); };
        ws.onmessage = function(event) {
            if (event.data === 'reload') {
                console.log('[livereload] reloading...');
                window.location.reload();
            }
        };
        ws.onclose = function() {
            console.log('[livereload] disconnected, reconnecting...');
            clearTimeout(reconnectTimer);
            reconnectTimer = setTimeout(connect, 1000);
        };
    }
    connect();
})();
</script>"##;
        html.replace("</body>", &format!("{livereload_script}</body>"))
    } else {
        html.to_string()
    }
}

// ============================================================================
// Pure render functions for Salsa tracked queries
// ============================================================================

/// Pure function to render a page to HTML (for Salsa tracking)
/// Returns Result - caller decides whether to show error page (dev) or fail (prod)
pub fn try_render_page_to_html(
    page: &Page,
    site_tree: &SiteTree,
    templates: &HashMap<String, String>,
) -> std::result::Result<String, String> {
    let mut loader = InMemoryLoader::new();
    for (path, content) in templates {
        loader.add(path.clone(), content.clone());
    }
    let mut engine = Engine::new(loader);

    let mut ctx = build_render_context(site_tree);
    ctx.set("page", page_to_value(page));
    ctx.set(
        "current_path",
        Value::String(page.route.as_str().to_string()),
    );

    engine
        .render("page.html", &ctx)
        .map_err(|e| format!("{e:?}"))
}

/// Render page - development mode (shows error page on failure)
pub fn render_page_to_html(
    page: &Page,
    site_tree: &SiteTree,
    templates: &HashMap<String, String>,
) -> String {
    try_render_page_to_html(page, site_tree, templates).unwrap_or_else(|e| render_error_page(&e))
}

/// Pure function to render a section to HTML (for Salsa tracking)
/// Returns Result - caller decides whether to show error page (dev) or fail (prod)
pub fn try_render_section_to_html(
    section: &Section,
    site_tree: &SiteTree,
    templates: &HashMap<String, String>,
) -> std::result::Result<String, String> {
    let mut loader = InMemoryLoader::new();
    for (path, content) in templates {
        loader.add(path.clone(), content.clone());
    }
    let mut engine = Engine::new(loader);

    let mut ctx = build_render_context(site_tree);
    ctx.set("section", section_to_value(section, site_tree));
    ctx.set(
        "current_path",
        Value::String(section.route.as_str().to_string()),
    );

    let template_name = if section.route.as_str() == "/" {
        "index.html"
    } else {
        "section.html"
    };

    engine
        .render(template_name, &ctx)
        .map_err(|e| format!("{e:?}"))
}

/// Render section - development mode (shows error page on failure)
pub fn render_section_to_html(
    section: &Section,
    site_tree: &SiteTree,
    templates: &HashMap<String, String>,
) -> String {
    try_render_section_to_html(section, site_tree, templates)
        .unwrap_or_else(|e| render_error_page(&e))
}

/// Marker that indicates a page contains a render error (for build mode detection)
pub const RENDER_ERROR_MARKER: &str = "<!-- DODECA_RENDER_ERROR -->";

/// Render a visible error page for development
fn render_error_page(error: &str) -> String {
    format!(
        r#"<!DOCTYPE html>
<html lang="en">
{RENDER_ERROR_MARKER}
<head>
    <meta charset="utf-8">
    <meta name="viewport" content="width=device-width, initial-scale=1">
    <title>Template Error - dodeca</title>
    <style>
        body {{
            font-family: system-ui, -apple-system, sans-serif;
            background: #1a1a2e;
            color: #eee;
            margin: 0;
            padding: 2rem;
        }}
        .error-container {{
            max-width: 900px;
            margin: 0 auto;
        }}
        h1 {{
            color: #ff6b6b;
            border-bottom: 2px solid #ff6b6b;
            padding-bottom: 0.5rem;
        }}
        pre {{
            background: #0f0f1a;
            border: 1px solid #333;
            border-radius: 8px;
            padding: 1rem;
            overflow-x: auto;
            white-space: pre-wrap;
            word-wrap: break-word;
            font-size: 14px;
            line-height: 1.5;
        }}
        .hint {{
            background: #2d2d44;
            border-left: 4px solid #4ecdc4;
            padding: 1rem;
            margin-top: 1rem;
        }}
    </style>
</head>
<body>
    <div class="error-container">
        <h1>Template Render Error</h1>
        <pre>{error}</pre>
        <div class="hint">
            <strong>Hint:</strong> Check your template syntax and ensure all referenced variables exist.
        </div>
    </div>
</body>
</html>"#
    )
}

/// Build the render context with config and global functions
fn build_render_context(site_tree: &SiteTree) -> Context {
    let mut ctx = Context::new();

    // Add config
    let mut config_map = HashMap::new();
    config_map.insert("title".to_string(), Value::String("facet".to_string()));
    config_map.insert(
        "description".to_string(),
        Value::String("A Rust reflection library".to_string()),
    );
    config_map.insert("base_url".to_string(), Value::String("/".to_string()));
    ctx.set("config", Value::Dict(config_map));

    // Register get_url function
    ctx.register_fn(
        "get_url",
        Box::new(move |_args, kwargs| {
            let path = kwargs
                .iter()
                .find(|(k, _)| k == "path")
                .map(|(_, v)| v.render_to_string())
                .unwrap_or_default();

            let url = if path.starts_with('/') {
                path
            } else if path.is_empty() {
                "/".to_string()
            } else {
                format!("/{path}")
            };
            Ok(Value::String(url))
        }),
    );

    // Register get_section function
    let sections = site_tree.sections.clone();
    let pages = site_tree.pages.clone();
    ctx.register_fn(
        "get_section",
        Box::new(move |_args, kwargs| {
            let path = kwargs
                .iter()
                .find(|(k, _)| k == "path")
                .map(|(_, v)| v.render_to_string())
                .unwrap_or_default();

            let route = path_to_route(&path);

            if let Some(section) = sections.get(&route) {
                let mut section_map = HashMap::new();
                section_map.insert(
                    "title".to_string(),
                    Value::String(section.title.as_str().to_string()),
                );
                section_map.insert(
                    "permalink".to_string(),
                    Value::String(section.route.as_str().to_string()),
                );
                section_map.insert("path".to_string(), Value::String(path.clone()));
                section_map.insert(
                    "content".to_string(),
                    Value::String(section.body_html.as_str().to_string()),
                );
                section_map.insert("toc".to_string(), headings_to_toc(&section.headings));

                let section_pages: Vec<Value> = pages
                    .values()
                    .filter(|p| p.section_route == section.route)
                    .map(|p| {
                        let mut page_map = HashMap::new();
                        page_map.insert(
                            "title".to_string(),
                            Value::String(p.title.as_str().to_string()),
                        );
                        page_map.insert(
                            "permalink".to_string(),
                            Value::String(p.route.as_str().to_string()),
                        );
                        page_map.insert(
                            "path".to_string(),
                            Value::String(route_to_path(p.route.as_str())),
                        );
                        page_map.insert("weight".to_string(), Value::Int(p.weight as i64));
                        page_map.insert("toc".to_string(), headings_to_toc(&p.headings));
                        Value::Dict(page_map)
                    })
                    .collect();
                section_map.insert("pages".to_string(), Value::List(section_pages));

                let subsections: Vec<Value> = sections
                    .values()
                    .filter(|s| {
                        s.route != section.route
                            && s.route.as_str().starts_with(section.route.as_str())
                            && s.route.as_str()[section.route.as_str().len()..]
                                .trim_matches('/')
                                .chars()
                                .filter(|c| *c == '/')
                                .count()
                                == 0
                    })
                    .map(|s| Value::String(route_to_path(s.route.as_str())))
                    .collect();
                section_map.insert("subsections".to_string(), Value::List(subsections));

                Ok(Value::Dict(section_map))
            } else {
                Ok(Value::None)
            }
        }),
    );

    ctx
}

/// Convert a heading to a Value dict with children field
fn heading_to_value(h: &Heading, children: Vec<Value>) -> Value {
    let mut map = HashMap::new();
    map.insert("title".to_string(), Value::String(h.title.clone()));
    map.insert("id".to_string(), Value::String(h.id.clone()));
    map.insert("level".to_string(), Value::Int(h.level as i64));
    map.insert("permalink".to_string(), Value::String(format!("#{}", h.id)));
    map.insert("children".to_string(), Value::List(children));
    Value::Dict(map)
}

/// Convert headings to a hierarchical TOC Value (Zola-style nested structure)
fn headings_to_toc(headings: &[Heading]) -> Value {
    build_toc_tree(headings)
}

/// Convert headings to hierarchical Value list for template context
fn headings_to_value(headings: &[Heading]) -> Value {
    build_toc_tree(headings)
}

/// Build a hierarchical tree from a flat list of headings
fn build_toc_tree(headings: &[Heading]) -> Value {
    if headings.is_empty() {
        return Value::List(vec![]);
    }

    // Find the minimum level to use as the "top level"
    let min_level = headings.iter().map(|h| h.level).min().unwrap_or(1);

    // Build tree recursively
    let (result, _) = build_toc_subtree(headings, 0, min_level);
    Value::List(result)
}

/// Recursively build TOC subtree, returns (list of Value nodes, next index to process)
fn build_toc_subtree(headings: &[Heading], start: usize, parent_level: u8) -> (Vec<Value>, usize) {
    let mut result = Vec::new();
    let mut i = start;

    while i < headings.len() {
        let h = &headings[i];

        // If we hit a heading at or above parent level (lower number), we're done with this subtree
        if h.level < parent_level {
            break;
        }

        // If this heading is at the expected level, add it with its children
        if h.level == parent_level {
            // Collect children (headings with level > parent_level until we hit another at parent_level)
            let (children, next_i) = build_toc_subtree(headings, i + 1, parent_level + 1);
            result.push(heading_to_value(h, children));
            i = next_i;
        } else {
            // Heading is deeper than expected - just move on
            i += 1;
        }
    }

    (result, i)
}

/// Convert a Page to a Value for template context
fn page_to_value(page: &Page) -> Value {
    let mut map = HashMap::new();
    map.insert(
        "title".to_string(),
        Value::String(page.title.as_str().to_string()),
    );
    map.insert(
        "content".to_string(),
        Value::String(page.body_html.as_str().to_string()),
    );
    map.insert(
        "permalink".to_string(),
        Value::String(page.route.as_str().to_string()),
    );
    map.insert(
        "path".to_string(),
        Value::String(route_to_path(page.route.as_str())),
    );
    map.insert("weight".to_string(), Value::Int(page.weight as i64));
    map.insert("toc".to_string(), headings_to_value(&page.headings));
    Value::Dict(map)
}

/// Convert a Section to a Value for template context
fn section_to_value(section: &Section, site_tree: &SiteTree) -> Value {
    let mut map = HashMap::new();
    map.insert(
        "title".to_string(),
        Value::String(section.title.as_str().to_string()),
    );
    map.insert(
        "content".to_string(),
        Value::String(section.body_html.as_str().to_string()),
    );
    map.insert(
        "permalink".to_string(),
        Value::String(section.route.as_str().to_string()),
    );
    map.insert(
        "path".to_string(),
        Value::String(route_to_path(section.route.as_str())),
    );
    map.insert("weight".to_string(), Value::Int(section.weight as i64));

    // Add pages in this section (including their headings)
    let section_pages: Vec<Value> = site_tree
        .pages
        .values()
        .filter(|p| p.section_route == section.route)
        .map(|p| {
            let mut page_map = HashMap::new();
            page_map.insert(
                "title".to_string(),
                Value::String(p.title.as_str().to_string()),
            );
            page_map.insert(
                "permalink".to_string(),
                Value::String(p.route.as_str().to_string()),
            );
            page_map.insert(
                "path".to_string(),
                Value::String(route_to_path(p.route.as_str())),
            );
            page_map.insert("weight".to_string(), Value::Int(p.weight as i64));
            page_map.insert("toc".to_string(), headings_to_value(&p.headings));
            Value::Dict(page_map)
        })
        .collect();
    map.insert("pages".to_string(), Value::List(section_pages));

    // Add subsections
    let subsections: Vec<Value> = site_tree
        .sections
        .values()
        .filter(|s| {
            s.route != section.route
                && s.route.as_str().starts_with(section.route.as_str())
                && s.route.as_str()[section.route.as_str().len()..]
                    .trim_matches('/')
                    .chars()
                    .filter(|c| *c == '/')
                    .count()
                    == 0
        })
        .map(|s| Value::String(route_to_path(s.route.as_str())))
        .collect();
    map.insert("subsections".to_string(), Value::List(subsections));
    map.insert("toc".to_string(), headings_to_value(&section.headings));

    Value::Dict(map)
}

/// Convert a source path like "learn/_index.md" to a route like "/learn/"
fn path_to_route(path: &str) -> Route {
    let mut p = path.to_string();

    // Remove .md extension
    if p.ends_with(".md") {
        p = p[..p.len() - 3].to_string();
    }

    // Handle _index
    if p.ends_with("/_index") {
        p = p[..p.len() - 7].to_string();
    } else if p == "_index" {
        p = String::new();
    }

    // Ensure leading and trailing slashes
    if p.is_empty() {
        Route::root()
    } else {
        Route::new(format!("/{p}/"))
    }
}

/// Convert a route like "/learn/" back to a path like "learn/_index.md"
fn route_to_path(route: &str) -> String {
    let r = route.trim_matches('/');
    if r.is_empty() {
        "_index.md".to_string()
    } else {
        format!("{r}/_index.md")
    }
}
