use crate::types::{HtmlBody, Route, RouteRef, Title};
use crate::{Page, Section};
use camino::{Utf8Path, Utf8PathBuf};
use color_eyre::Result;
use maud::{DOCTYPE, Markup, PreEscaped, html};
use rayon::prelude::*;
use std::collections::BTreeMap;
use std::fs;

/// Render all sections and pages to the output directory
pub fn render_all(
    sections: &BTreeMap<Route, Section>,
    pages: &BTreeMap<Route, Page>,
    output_dir: &Utf8Path,
) -> Result<()> {
    // Ensure output directory exists
    fs::create_dir_all(output_dir)?;

    // Collect render data
    let section_data: Vec<_> = sections
        .values()
        .map(|s| RenderData {
            route: s.route.clone(),
            title: s.title.clone(),
            body_html: s.body_html.clone(),
        })
        .collect();

    let page_data: Vec<_> = pages
        .values()
        .map(|p| RenderData {
            route: p.route.clone(),
            title: p.title.clone(),
            body_html: p.body_html.clone(),
        })
        .collect();

    // Build sidebar info
    let sidebar_sections: Vec<SidebarSection> = sections
        .values()
        .map(|s| SidebarSection {
            route: s.route.clone(),
            title: s.title.clone(),
            weight: s.weight,
        })
        .collect();

    let sidebar_pages: Vec<SidebarPage> = pages
        .values()
        .map(|p| SidebarPage {
            route: p.route.clone(),
            title: p.title.clone(),
            weight: p.weight,
            section_route: p.section_route.clone(),
        })
        .collect();

    let sidebar_info = SidebarInfo {
        sections: sidebar_sections,
        pages: sidebar_pages,
    };

    // Render sections in parallel
    section_data
        .par_iter()
        .try_for_each(|data| render_item(data, &sidebar_info, output_dir))?;

    // Render pages in parallel
    page_data
        .par_iter()
        .try_for_each(|data| render_item(data, &sidebar_info, output_dir))?;

    Ok(())
}

/// Data needed to render a page
struct RenderData {
    route: Route,
    title: Title,
    body_html: HtmlBody,
}

/// Sidebar section info (for rendering navigation)
#[derive(Clone)]
struct SidebarSection {
    route: Route,
    title: Title,
    weight: i32,
}

/// Sidebar page info
#[derive(Clone)]
struct SidebarPage {
    route: Route,
    title: Title,
    weight: i32,
    section_route: Route,
}

/// All sidebar information
struct SidebarInfo {
    sections: Vec<SidebarSection>,
    pages: Vec<SidebarPage>,
}

impl SidebarInfo {
    fn get_section(&self, route: &RouteRef) -> Option<&SidebarSection> {
        self.sections
            .iter()
            .find(|s| s.route.as_str() == route.as_str())
    }

    fn top_section_for(&self, route: &RouteRef) -> Option<&SidebarSection> {
        if route.is_in_section("learn") {
            self.get_section(RouteRef::from_static("/learn/"))
        } else if route.is_in_section("extend") {
            self.get_section(RouteRef::from_static("/extend/"))
        } else if route.is_in_section("contribute") {
            self.get_section(RouteRef::from_static("/contribute/"))
        } else {
            None
        }
    }

    fn pages_in_section(&self, section_route: &RouteRef) -> Vec<&SidebarPage> {
        let mut pages: Vec<_> = self
            .pages
            .iter()
            .filter(|p| p.section_route.as_str() == section_route.as_str())
            .collect();
        pages.sort_by(|a, b| {
            a.weight
                .cmp(&b.weight)
                .then_with(|| a.title.as_str().cmp(b.title.as_str()))
        });
        pages
    }

    fn subsections(&self, section_route: &RouteRef) -> Vec<&SidebarSection> {
        let mut subs: Vec<_> = self
            .sections
            .iter()
            .filter(|s| {
                s.route.as_str() != section_route.as_str()
                    && s.route.as_str().starts_with(section_route.as_str())
                    && s.route.as_str()[section_route.as_str().len()..]
                        .trim_matches('/')
                        .chars()
                        .filter(|c| *c == '/')
                        .count()
                        == 0
            })
            .collect();
        subs.sort_by(|a, b| {
            a.weight
                .cmp(&b.weight)
                .then_with(|| a.title.as_str().cmp(b.title.as_str()))
        });
        subs
    }
}

/// Render a single item (section or page)
fn render_item(data: &RenderData, sidebar: &SidebarInfo, output_dir: &Utf8Path) -> Result<()> {
    let html = render_full_page(sidebar, &data.route, &data.title, &data.body_html);

    let out_path = output_path(output_dir, &data.route);
    fs::create_dir_all(out_path.parent().unwrap_or(output_dir))?;
    fs::write(&out_path, html.into_string())?;

    Ok(())
}

/// Get the output file path for a route
fn output_path(output_dir: &Utf8Path, route: &Route) -> Utf8PathBuf {
    let relative = route.as_str().trim_start_matches('/');
    if relative.is_empty() {
        output_dir.join("index.html")
    } else {
        output_dir.join(relative).join("index.html")
    }
}

/// Render a full HTML page with layout
fn render_full_page(
    sidebar: &SidebarInfo,
    route: &Route,
    title: &Title,
    body_html: &HtmlBody,
) -> Markup {
    let has_sidebar = route.is_in_section("learn")
        || route.is_in_section("extend")
        || route.is_in_section("contribute");

    html! {
        (DOCTYPE)
        html lang="en" {
            head {
                meta charset="utf-8";
                meta name="viewport" content="width=device-width, initial-scale=1";
                title { (title.as_str()) " - facet" }
                link rel="stylesheet" href="/main.css";
                link rel="stylesheet" href="/pagefind/pagefind-ui.css";
                link rel="icon" type="image/png" href="/favicon.png";
            }
            body {
                (render_nav(route))

                @if has_sidebar {
                    div.docs-layout {
                        @if let Some(top_section) = sidebar.top_section_for(route.as_ref()) {
                            (render_sidebar(sidebar, top_section, route))
                        }
                        main.docs-content {
                            article {
                                h1.page-title { (title.as_str()) }
                                (PreEscaped(body_html.as_str()))
                            }
                        }
                    }
                } @else {
                    div.container {
                        main.content {
                            (PreEscaped(body_html.as_str()))
                        }
                    }
                }

                script src="/pagefind/pagefind-ui.js" {}
                (render_scripts())
            }
        }
    }
}

/// Render the top navigation
fn render_nav(current_route: &Route) -> Markup {
    let in_learn = current_route.is_in_section("learn");
    let in_extend = current_route.is_in_section("extend");
    let in_contribute = current_route.is_in_section("contribute");

    html! {
        nav.site-nav {
            a.site-nav-brand href="/" {
                img.site-nav-logo src="/favicon.png" alt="";
                span { "facet" }
            }
            div.site-nav-links {
                a href="/learn/" class=[in_learn.then_some("active")] { "Learn" }
                a href="/extend/" class=[in_extend.then_some("active")] { "Extend" }
                a href="/contribute/" class=[in_contribute.then_some("active")] { "Contribute" }
            }
            div.site-nav-search id="search" {}
            a.site-nav-github href="https://github.com/facet-rs/facet" title="GitHub" {
                (github_icon())
            }
        }
    }
}

/// Render the sidebar navigation
fn render_sidebar(
    sidebar: &SidebarInfo,
    section: &SidebarSection,
    current_route: &Route,
) -> Markup {
    html! {
        aside.sidebar {
            nav {
                div.sidebar-header {
                    a href=(section.route.as_str()) { (section.title.as_str()) }
                }
                (render_section_tree(sidebar, &section.route, current_route))
            }
        }
    }
}

/// Recursively render a section's navigation tree
fn render_section_tree(
    sidebar: &SidebarInfo,
    section_route: &Route,
    current_route: &Route,
) -> Markup {
    let pages = sidebar.pages_in_section(section_route.as_ref());
    let subsections = sidebar.subsections(section_route.as_ref());

    if pages.is_empty() && subsections.is_empty() {
        return html! {};
    }

    html! {
        ul {
            @for page in pages {
                li {
                    a href=(page.route.as_str())
                      class=[is_active(&page.route, current_route).then_some("active")] {
                        (page.title.as_str())
                    }
                }
            }
            @for subsection in subsections {
                li.has-children {
                    a href=(subsection.route.as_str())
                      class=[is_active_or_ancestor(&subsection.route, current_route).then_some("active")] {
                        (subsection.title.as_str())
                    }
                    (render_section_tree(sidebar, &subsection.route, current_route))
                }
            }
        }
    }
}

fn is_active(route: &Route, current: &Route) -> bool {
    route == current
}

fn is_active_or_ancestor(section_route: &Route, current: &Route) -> bool {
    current.as_str().starts_with(section_route.as_str())
}

fn github_icon() -> Markup {
    html! {
        svg viewBox="0 0 16 16" width="24" height="24" fill="currentColor" {
            path d="M8 0C3.58 0 0 3.58 0 8c0 3.54 2.29 6.53 5.47 7.59.4.07.55-.17.55-.38 0-.19-.01-.82-.01-1.49-2.01.37-2.53-.49-2.69-.94-.09-.23-.48-.94-.82-1.13-.28-.15-.68-.52-.01-.53.63-.01 1.08.58 1.23.82.72 1.21 1.87.87 2.33.66.07-.52.28-.87.51-1.07-1.78-.2-3.64-.89-3.64-3.95 0-.87.31-1.59.82-2.15-.08-.2-.36-1.02.08-2.12 0 0 .67-.21 2.2.82.64-.18 1.32-.27 2-.27.68 0 1.36.09 2 .27 1.53-1.04 2.2-.82 2.2-.82.44 1.1.16 1.92.08 2.12.51.56.82 1.27.82 2.15 0 3.07-1.87 3.75-3.65 3.95.29.25.54.73.54 1.48 0 1.07-.01 1.93-.01 2.2 0 .21.15.46.55.38A8.013 8.013 0 0016 8c0-4.42-3.58-8-8-8z" {}
        }
    }
}

fn render_scripts() -> Markup {
    let script_content = r##"
document.addEventListener('DOMContentLoaded', function() {
    new PagefindUI({
        element: "#search",
        showSubResults: true,
        showImages: false,
        translations: { placeholder: "Search" }
    });

    document.addEventListener('keydown', function(e) {
        const searchInput = document.querySelector('#search input');
        if (!searchInput) return;
        if ((e.metaKey || e.ctrlKey) && e.key === 'k') {
            e.preventDefault();
            searchInput.focus();
            searchInput.select();
        }
        if (e.key === '/' && e.target.tagName !== 'INPUT') {
            e.preventDefault();
            searchInput.focus();
            searchInput.select();
        }
    });
});
"##;
    html! {
        script {
            (PreEscaped(script_content))
        }
    }
}

/// Compile Sass to CSS
pub fn compile_sass(content_dir: &Utf8Path, output_dir: &Utf8Path) -> Result<()> {
    let sass_dir = content_dir.parent().unwrap_or(content_dir).join("sass");
    let main_scss = sass_dir.join("main.scss");

    if main_scss.exists() {
        let css = grass::from_path(&main_scss, &grass::Options::default())
            .map_err(|e| color_eyre::eyre::eyre!("Sass compilation failed: {}", e))?;

        fs::write(output_dir.join("main.css"), css)?;
    }

    Ok(())
}
