//! Template renderer
//!
//! Renders templates to strings using a context.
//! This is the main public API for the template engine.

use super::ast::{self, Expr, Node, Target};
use super::error::TemplateSource;
use super::eval::{Context, Evaluator, Value};
use super::parser::Parser;
use camino::{Utf8Path, Utf8PathBuf};
use miette::Result;
use std::collections::HashMap;
use std::rc::Rc;

/// A stored macro definition
#[derive(Debug, Clone)]
struct MacroDef {
    params: Vec<ast::MacroParam>,
    body: Vec<Node>,
}

/// Stored macros organized by namespace
/// Key is namespace ("self" for current template, or import alias)
/// Value is map of macro_name -> MacroDef
type MacroRegistry = HashMap<String, HashMap<String, MacroDef>>;

/// Trait for loading templates by name (for inheritance and includes)
pub trait TemplateLoader {
    /// Load a template by path/name, returning the source code
    fn load(&self, name: &str) -> Option<String>;
}

/// A simple in-memory template loader
#[derive(Default)]
pub struct InMemoryLoader {
    templates: HashMap<String, String>,
}

impl InMemoryLoader {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn add(&mut self, name: impl Into<String>, source: impl Into<String>) {
        self.templates.insert(name.into(), source.into());
    }
}

impl TemplateLoader for InMemoryLoader {
    fn load(&self, name: &str) -> Option<String> {
        self.templates.get(name).cloned()
    }
}

/// A file-based template loader that reads from a directory
pub struct FileLoader {
    root: Utf8PathBuf,
}

impl FileLoader {
    /// Create a new file loader rooted at the given directory
    pub fn new(root: impl AsRef<Utf8Path>) -> Self {
        Self {
            root: root.as_ref().to_owned(),
        }
    }

    /// Get the root directory
    pub fn root(&self) -> &Utf8Path {
        &self.root
    }
}

impl TemplateLoader for FileLoader {
    fn load(&self, name: &str) -> Option<String> {
        let path = self.root.join(name);
        std::fs::read_to_string(&path).ok()
    }
}

/// A compiled template ready for rendering
#[derive(Debug, Clone)]
pub struct Template {
    ast: ast::Template,
    source: TemplateSource,
}

impl Template {
    /// Parse a template from source
    pub fn parse(name: impl Into<String>, source: impl Into<String>) -> Result<Self> {
        let name = name.into();
        let source_str: String = source.into();
        let template_source = TemplateSource::new(&name, &source_str);

        let parser = Parser::new(name, source_str);
        let ast = parser.parse()?;

        Ok(Self {
            ast,
            source: template_source,
        })
    }

    /// Get the extends path if this template extends another
    pub fn extends_path(&self) -> Option<&str> {
        for node in &self.ast.body {
            match node {
                Node::Extends(e) => return Some(&e.path.value),
                Node::Text(t) if t.text.trim().is_empty() => continue,
                _ => return None,
            }
        }
        None
    }

    /// Extract block definitions from this template
    pub fn blocks(&self) -> HashMap<String, &[Node]> {
        let mut blocks = HashMap::new();
        self.collect_blocks(&self.ast.body, &mut blocks);
        blocks
    }

    fn collect_blocks<'a>(&'a self, nodes: &'a [Node], blocks: &mut HashMap<String, &'a [Node]>) {
        for node in nodes {
            if let Node::Block(block) = node {
                blocks.insert(block.name.name.clone(), &block.body);
            }
        }
    }

    /// Extract import statements from this template
    pub fn imports(&self) -> Vec<(&str, &str)> {
        self.ast
            .body
            .iter()
            .filter_map(|node| {
                if let Node::Import(import) = node {
                    Some((import.path.value.as_str(), import.alias.name.as_str()))
                } else {
                    None
                }
            })
            .collect()
    }

    /// Render the template with the given context (no inheritance support)
    pub fn render(&self, ctx: &Context) -> Result<String> {
        let mut output = String::new();
        let mut renderer = Renderer {
            ctx: ctx.clone(),
            source: &self.source,
            output: &mut output,
            blocks: HashMap::new(),
            loader: None,
            macros: HashMap::new(),
        };
        renderer.collect_macros(&self.ast.body);
        renderer.render_nodes(&self.ast.body)?;
        Ok(output)
    }

    /// Render the template with a simple key-value context
    pub fn render_with<I, K, V>(&self, vars: I) -> Result<String>
    where
        I: IntoIterator<Item = (K, V)>,
        K: Into<String>,
        V: Into<Value>,
    {
        let mut ctx = Context::new();
        for (k, v) in vars {
            ctx.set(k, v.into());
        }
        self.render(&ctx)
    }
}

/// Template engine with support for inheritance and includes
pub struct Engine {
    loader: Rc<dyn TemplateLoader>,
}

impl Engine {
    /// Create a new engine with the given loader
    pub fn new(loader: impl TemplateLoader + 'static) -> Self {
        Self {
            loader: Rc::new(loader),
        }
    }

    /// Load a template (no caching - that's Salsa's job)
    pub fn load(&self, name: &str) -> Result<Template> {
        let source = self
            .loader
            .load(name)
            .ok_or_else(|| miette::miette!("Template not found: {}", name))?;
        Template::parse(name, source)
    }

    /// Render a template by name with inheritance support
    pub fn render(&mut self, name: &str, ctx: &Context) -> Result<String> {
        // Load the template
        let template = self.load(name)?;

        // Check for extends
        if let Some(parent_path) = template.extends_path() {
            // Collect blocks and imports from child template
            let child_blocks = template.blocks();
            let child_imports: Vec<(String, String)> = template
                .imports()
                .into_iter()
                .map(|(p, a)| (p.to_string(), a.to_string()))
                .collect();

            // Recursively resolve the inheritance chain
            self.render_with_blocks(parent_path, ctx, child_blocks, child_imports)
        } else {
            // No inheritance, render directly
            let mut output = String::new();
            let mut renderer = Renderer {
                ctx: ctx.clone(),
                source: &template.source,
                output: &mut output,
                blocks: HashMap::new(),
                loader: Some(self.loader.clone()),
                macros: HashMap::new(),
            };
            renderer.collect_macros(&template.ast.body);
            renderer.render_nodes(&template.ast.body)?;
            Ok(output)
        }
    }

    fn render_with_blocks(
        &mut self,
        name: &str,
        ctx: &Context,
        child_blocks: HashMap<String, &[Node]>,
        child_imports: Vec<(String, String)>,
    ) -> Result<String> {
        let template = self.load(name)?;

        // Check if parent also extends
        if let Some(grandparent_path) = template.extends_path() {
            // Merge blocks: child blocks override parent blocks
            let parent_blocks = template.blocks();
            let mut merged_blocks: HashMap<String, Vec<Node>> = HashMap::new();

            // Add parent blocks first
            for (name, nodes) in parent_blocks {
                merged_blocks.insert(name, nodes.to_vec());
            }
            // Child blocks override
            for (name, nodes) in child_blocks {
                merged_blocks.insert(name, nodes.to_vec());
            }

            // Merge imports: add parent imports first, then child imports
            let parent_imports: Vec<(String, String)> = template
                .imports()
                .into_iter()
                .map(|(p, a)| (p.to_string(), a.to_string()))
                .collect();
            let mut all_imports = parent_imports;
            all_imports.extend(child_imports);

            // Convert to owned for recursive call
            let merged: HashMap<String, &[Node]> = merged_blocks
                .iter()
                .map(|(k, v)| (k.clone(), v.as_slice()))
                .collect();

            self.render_with_blocks(grandparent_path, ctx, merged, all_imports)
        } else {
            // This is the root template, render it with block overrides
            // Convert child_blocks to owned nodes
            let owned_blocks: HashMap<String, Vec<Node>> = child_blocks
                .into_iter()
                .map(|(k, v)| (k, v.to_vec()))
                .collect();

            let mut output = String::new();
            let mut renderer = Renderer {
                ctx: ctx.clone(),
                source: &template.source,
                output: &mut output,
                blocks: owned_blocks,
                loader: Some(self.loader.clone()),
                macros: HashMap::new(),
            };

            // Process imports from child templates
            for (path, alias) in &child_imports {
                renderer.load_macros_from(path, alias)?;
            }

            renderer.collect_macros(&template.ast.body);
            renderer.render_nodes(&template.ast.body)?;
            Ok(output)
        }
    }
}

/// Internal renderer state
struct Renderer<'a> {
    ctx: Context,
    source: &'a TemplateSource,
    output: &'a mut String,
    /// Block overrides from child templates
    blocks: HashMap<String, Vec<Node>>,
    /// Template loader for includes and imports
    loader: Option<Rc<dyn TemplateLoader>>,
    /// Macro definitions by namespace
    macros: MacroRegistry,
}

impl<'a> Renderer<'a> {
    fn render_nodes(&mut self, nodes: &[Node]) -> Result<()> {
        for node in nodes {
            self.render_node(node)?;
        }
        Ok(())
    }

    fn render_node(&mut self, node: &Node) -> Result<()> {
        match node {
            Node::Text(text) => {
                self.output.push_str(&text.text);
            }
            Node::Print(print) => {
                // Check if this is a macro call
                if let Expr::MacroCall(macro_call) = &print.expr {
                    // Evaluate arguments
                    let eval = Evaluator::new(&self.ctx, self.source);
                    let args: Vec<Value> = macro_call
                        .args
                        .iter()
                        .map(|a| eval.eval(a))
                        .collect::<Result<Vec<_>>>()?;
                    let kwargs: Vec<(String, Value)> = macro_call
                        .kwargs
                        .iter()
                        .map(|(ident, expr)| Ok((ident.name.clone(), eval.eval(expr)?)))
                        .collect::<Result<Vec<_>>>()?;

                    // Call the macro
                    let result = self.call_macro(
                        &macro_call.namespace.name,
                        &macro_call.macro_name.name,
                        &args,
                        &kwargs,
                    )?;
                    // Macro output is already HTML, don't escape it
                    self.output.push_str(&result);
                } else {
                    let eval = Evaluator::new(&self.ctx, self.source);
                    let value = eval.eval(&print.expr)?;
                    // Skip escaping for safe values, auto-escape everything else
                    let s = if value.is_safe() {
                        value.render_to_string()
                    } else {
                        html_escape(&value.render_to_string())
                    };
                    self.output.push_str(&s);
                }
            }
            Node::If(if_node) => {
                let eval = Evaluator::new(&self.ctx, self.source);
                let condition = eval.eval(&if_node.condition)?;

                if condition.is_truthy() {
                    self.render_nodes(&if_node.then_body)?;
                } else {
                    // Check elif branches
                    let mut handled = false;
                    for elif in &if_node.elif_branches {
                        let eval = Evaluator::new(&self.ctx, self.source);
                        let cond = eval.eval(&elif.condition)?;
                        if cond.is_truthy() {
                            self.render_nodes(&elif.body)?;
                            handled = true;
                            break;
                        }
                    }

                    // Else branch
                    if !handled {
                        if let Some(else_body) = &if_node.else_body {
                            self.render_nodes(else_body)?;
                        }
                    }
                }
            }
            Node::For(for_node) => {
                let eval = Evaluator::new(&self.ctx, self.source);
                let iter_value = eval.eval(&for_node.iter)?;

                let items: Vec<Value> = match iter_value {
                    Value::List(list) => list,
                    Value::Dict(map) => map
                        .into_iter()
                        .map(|(k, v)| {
                            let mut entry = std::collections::HashMap::new();
                            entry.insert("key".to_string(), Value::String(k));
                            entry.insert("value".to_string(), v);
                            Value::Dict(entry)
                        })
                        .collect(),
                    Value::String(s) => s.chars().map(|c| Value::String(c.to_string())).collect(),
                    _ => Vec::new(),
                };

                if items.is_empty() {
                    // Render else body if present
                    if let Some(else_body) = &for_node.else_body {
                        self.render_nodes(else_body)?;
                    }
                } else {
                    let len = items.len();
                    for (index, item) in items.into_iter().enumerate() {
                        self.ctx.push_scope();

                        // Bind loop variable(s)
                        match &for_node.target {
                            Target::Single { name, .. } => {
                                self.ctx.set(name.clone(), item);
                            }
                            Target::Tuple { names, .. } => {
                                // For tuple unpacking, expect item to be a list
                                if let Value::List(parts) = item {
                                    for (i, (name, _)) in names.iter().enumerate() {
                                        let val = parts.get(i).cloned().unwrap_or(Value::None);
                                        self.ctx.set(name.clone(), val);
                                    }
                                } else if let Value::Dict(map) = item {
                                    // Special case: dict iteration gives key, value
                                    if names.len() == 2 {
                                        if let Some(key) = map.get("key") {
                                            self.ctx.set(names[0].0.clone(), key.clone());
                                        }
                                        if let Some(value) = map.get("value") {
                                            self.ctx.set(names[1].0.clone(), value.clone());
                                        }
                                    }
                                }
                            }
                        }

                        // Bind loop helper variables
                        let mut loop_var = std::collections::HashMap::new();
                        loop_var.insert("index".to_string(), Value::Int((index + 1) as i64));
                        loop_var.insert("index0".to_string(), Value::Int(index as i64));
                        loop_var.insert("first".to_string(), Value::Bool(index == 0));
                        loop_var.insert("last".to_string(), Value::Bool(index == len - 1));
                        loop_var.insert("length".to_string(), Value::Int(len as i64));
                        self.ctx.set("loop", Value::Dict(loop_var));

                        self.render_nodes(&for_node.body)?;
                        self.ctx.pop_scope();
                    }
                }
            }
            Node::Include(_include) => {
                // TODO: Template loading/caching
                self.output.push_str("<!-- include not implemented -->");
            }
            Node::Block(block) => {
                // Check if we have an override for this block
                if let Some(override_body) = self.blocks.get(&block.name.name).cloned() {
                    // Render the overridden block content
                    self.render_nodes(&override_body)?;
                } else {
                    // Render the default block content
                    self.render_nodes(&block.body)?;
                }
            }
            Node::Extends(_extends) => {
                // Extends is handled at the Engine level, not during node rendering
                // When we reach here, we're rendering the parent template
            }
            Node::Comment(_) => {
                // Comments are not rendered
            }
            Node::Set(set_node) => {
                let eval = Evaluator::new(&self.ctx, self.source);
                let value = eval.eval(&set_node.value)?;
                self.ctx.set(set_node.name.name.clone(), value);
            }
            Node::Import(import) => {
                // Load macros from the imported template
                if let Some(loader) = &self.loader {
                    if let Some(source) = loader.load(&import.path.value) {
                        if let Ok(template) = Template::parse(&import.path.value, source) {
                            // Extract macros from the imported template
                            let mut namespace_macros = HashMap::new();
                            for node in &template.ast.body {
                                if let Node::Macro(m) = node {
                                    namespace_macros.insert(
                                        m.name.name.clone(),
                                        MacroDef {
                                            params: m.params.clone(),
                                            body: m.body.clone(),
                                        },
                                    );
                                }
                            }
                            self.macros
                                .insert(import.alias.name.clone(), namespace_macros);
                        }
                    }
                }
            }
            Node::Macro(_macro_def) => {
                // Macro definitions are collected by collect_macros before rendering
            }
        }

        Ok(())
    }

    /// Collect macro definitions from the template body into the "self" namespace
    fn collect_macros(&mut self, nodes: &[Node]) {
        let mut self_macros = HashMap::new();
        for node in nodes {
            if let Node::Macro(m) = node {
                self_macros.insert(
                    m.name.name.clone(),
                    MacroDef {
                        params: m.params.clone(),
                        body: m.body.clone(),
                    },
                );
            }
        }
        if !self_macros.is_empty() {
            self.macros.insert("self".to_string(), self_macros);
        }
    }

    /// Load macros from an imported template file into the given namespace
    fn load_macros_from(&mut self, path: &str, alias: &str) -> Result<()> {
        if let Some(loader) = &self.loader {
            if let Some(source) = loader.load(path) {
                let template = Template::parse(path, source)?;
                // Extract macros from the imported template
                let mut namespace_macros = HashMap::new();
                for node in &template.ast.body {
                    if let Node::Macro(m) = node {
                        namespace_macros.insert(
                            m.name.name.clone(),
                            MacroDef {
                                params: m.params.clone(),
                                body: m.body.clone(),
                            },
                        );
                    }
                }
                self.macros.insert(alias.to_string(), namespace_macros);
            }
        }
        Ok(())
    }

    /// Call a macro and return its rendered output
    fn call_macro(
        &mut self,
        namespace: &str,
        macro_name: &str,
        args: &[Value],
        kwargs: &[(String, Value)],
    ) -> Result<String> {
        // Find the macro
        let macro_def = self
            .macros
            .get(namespace)
            .and_then(|ns| ns.get(macro_name))
            .cloned()
            .ok_or_else(|| miette::miette!("Macro not found: {}::{}", namespace, macro_name))?;

        // Save output and create new buffer for macro
        let mut macro_output = String::new();
        std::mem::swap(self.output, &mut macro_output);

        // Set up "self" namespace with macros from the called macro's namespace
        // This allows macros to call other macros from the same file via self::
        // Only do this if we're not already calling from the "self" namespace
        let saved_self = if namespace != "self" {
            let saved = self.macros.remove("self");
            if let Some(ns_macros) = self.macros.get(namespace).cloned() {
                self.macros.insert("self".to_string(), ns_macros);
            }
            saved
        } else {
            None
        };

        // Push a new scope for macro arguments
        self.ctx.push_scope();

        // Bind positional arguments
        for (i, param) in macro_def.params.iter().enumerate() {
            let value = if i < args.len() {
                args[i].clone()
            } else if let Some((_, v)) = kwargs.iter().find(|(k, _)| k == &param.name.name) {
                v.clone()
            } else if let Some(ref default_expr) = param.default {
                // Evaluate default value
                let eval = Evaluator::new(&self.ctx, self.source);
                eval.eval(default_expr)?
            } else {
                Value::None
            };
            self.ctx.set(param.name.name.clone(), value);
        }

        // Render macro body
        self.render_nodes(&macro_def.body)?;

        // Restore scope
        self.ctx.pop_scope();

        // Restore "self" namespace (only if we modified it)
        if namespace != "self" {
            self.macros.remove("self");
            if let Some(saved) = saved_self {
                self.macros.insert("self".to_string(), saved);
            }
        }

        // Swap back and return macro output
        std::mem::swap(self.output, &mut macro_output);
        Ok(macro_output)
    }
}

/// HTML escape a string
fn html_escape(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
        .replace('\'', "&#x27;")
}

// Convenience conversions for common types
impl From<&str> for Value {
    fn from(s: &str) -> Self {
        Value::String(s.to_string())
    }
}

impl From<String> for Value {
    fn from(s: String) -> Self {
        Value::String(s)
    }
}

impl From<i64> for Value {
    fn from(i: i64) -> Self {
        Value::Int(i)
    }
}

impl From<i32> for Value {
    fn from(i: i32) -> Self {
        Value::Int(i as i64)
    }
}

impl From<f64> for Value {
    fn from(f: f64) -> Self {
        Value::Float(f)
    }
}

impl From<bool> for Value {
    fn from(b: bool) -> Self {
        Value::Bool(b)
    }
}

impl<T: Into<Value>> From<Vec<T>> for Value {
    fn from(v: Vec<T>) -> Self {
        Value::List(v.into_iter().map(Into::into).collect())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_simple_text() {
        let t = Template::parse("test", "Hello, world!").unwrap();
        assert_eq!(t.render(&Context::new()).unwrap(), "Hello, world!");
    }

    #[test]
    fn test_variable() {
        let t = Template::parse("test", "Hello, {{ name }}!").unwrap();
        let result = t.render_with([("name", "Alice")]).unwrap();
        assert_eq!(result, "Hello, Alice!");
    }

    #[test]
    fn test_if_true() {
        let t = Template::parse("test", "{% if show %}visible{% endif %}").unwrap();
        let result = t.render_with([("show", Value::Bool(true))]).unwrap();
        assert_eq!(result, "visible");
    }

    #[test]
    fn test_if_false() {
        let t = Template::parse("test", "{% if show %}visible{% endif %}").unwrap();
        let result = t.render_with([("show", Value::Bool(false))]).unwrap();
        assert_eq!(result, "");
    }

    #[test]
    fn test_if_else() {
        let t = Template::parse("test", "{% if show %}yes{% else %}no{% endif %}").unwrap();
        let result = t.render_with([("show", Value::Bool(false))]).unwrap();
        assert_eq!(result, "no");
    }

    #[test]
    fn test_for_loop() {
        let t = Template::parse("test", "{% for item in items %}{{ item }} {% endfor %}").unwrap();
        let items: Value = vec!["a", "b", "c"].into();
        let result = t.render_with([("items", items)]).unwrap();
        assert_eq!(result, "a b c ");
    }

    #[test]
    fn test_filter() {
        let t = Template::parse("test", "{{ name | upper }}").unwrap();
        let result = t.render_with([("name", "alice")]).unwrap();
        assert_eq!(result, "ALICE");
    }

    #[test]
    fn test_html_escape() {
        let t = Template::parse("test", "{{ content }}").unwrap();
        let result = t
            .render_with([("content", "<script>alert('xss')</script>")])
            .unwrap();
        assert_eq!(
            result,
            "&lt;script&gt;alert(&#x27;xss&#x27;)&lt;/script&gt;"
        );
    }

    #[test]
    fn test_field_access() {
        let t = Template::parse("test", "{{ user.name }}").unwrap();
        let mut user = std::collections::HashMap::new();
        user.insert("name".to_string(), Value::String("Bob".to_string()));
        let result = t.render_with([("user", Value::Dict(user))]).unwrap();
        assert_eq!(result, "Bob");
    }

    #[test]
    fn test_loop_index() {
        let t =
            Template::parse("test", "{% for x in items %}{{ loop.index }}{% endfor %}").unwrap();
        let items: Value = vec!["a", "b", "c"].into();
        let result = t.render_with([("items", items)]).unwrap();
        assert_eq!(result, "123");
    }

    #[test]
    fn test_set() {
        let t = Template::parse("test", "{% set greeting = \"hello\" %}{{ greeting }}").unwrap();
        let result = t.render(&Context::new()).unwrap();
        assert_eq!(result, "hello");
    }

    #[test]
    fn test_set_expression() {
        let t = Template::parse("test", "{% set x = 1 + 2 %}{{ x }}").unwrap();
        let result = t.render(&Context::new()).unwrap();
        assert_eq!(result, "3");
    }

    #[test]
    fn test_is_starting_with() {
        let t = Template::parse(
            "test",
            r#"{% if path is starting_with("/learn") %}yes{% else %}no{% endif %}"#,
        )
        .unwrap();
        let result = t.render_with([("path", "/learn/intro")]).unwrap();
        assert_eq!(result, "yes");

        let result = t.render_with([("path", "/other")]).unwrap();
        assert_eq!(result, "no");
    }

    #[test]
    fn test_is_containing() {
        let t = Template::parse(
            "test",
            r#"{% if text is containing("foo") %}yes{% else %}no{% endif %}"#,
        )
        .unwrap();
        let result = t.render_with([("text", "hello foo bar")]).unwrap();
        assert_eq!(result, "yes");

        let result = t.render_with([("text", "hello bar")]).unwrap();
        assert_eq!(result, "no");
    }

    #[test]
    fn test_is_not() {
        let t = Template::parse(
            "test",
            r#"{% if x is not empty %}has content{% else %}empty{% endif %}"#,
        )
        .unwrap();
        let result = t.render_with([("x", "hello")]).unwrap();
        assert_eq!(result, "has content");

        let result = t.render_with([("x", "")]).unwrap();
        assert_eq!(result, "empty");
    }

    #[test]
    fn test_safe_filter() {
        // Without safe filter, HTML is escaped
        let t = Template::parse("test", "{{ content }}").unwrap();
        let result = t.render_with([("content", "<b>bold</b>")]).unwrap();
        assert_eq!(result, "&lt;b&gt;bold&lt;/b&gt;");

        // With safe filter, HTML is NOT escaped
        let t = Template::parse("test", "{{ content | safe }}").unwrap();
        let result = t.render_with([("content", "<b>bold</b>")]).unwrap();
        assert_eq!(result, "<b>bold</b>");
    }

    #[test]
    fn test_safe_filter_with_other_filters() {
        // Safe filter should work with other filters chained
        let t = Template::parse("test", "{{ content | upper | safe }}").unwrap();
        let result = t.render_with([("content", "<b>bold</b>")]).unwrap();
        assert_eq!(result, "<B>BOLD</B>");
    }

    #[test]
    fn test_block_default_content() {
        // Block with default content, no override
        let t = Template::parse("test", "{% block title %}Default Title{% endblock %}").unwrap();
        let result = t.render(&Context::new()).unwrap();
        assert_eq!(result, "Default Title");
    }

    #[test]
    fn test_template_inheritance() {
        // Create a loader with base and child templates
        let mut loader = InMemoryLoader::new();
        loader.add(
            "base.html",
            "Header {% block content %}default{% endblock %} Footer",
        );
        loader.add(
            "child.html",
            "{% extends \"base.html\" %}{% block content %}Custom Content{% endblock %}",
        );

        let mut engine = Engine::new(loader);
        let ctx = Context::new();

        // Render child template - should use base with overridden block
        let result = engine.render("child.html", &ctx).unwrap();
        assert_eq!(result, "Header Custom Content Footer");
    }

    #[test]
    fn test_template_inheritance_with_variables() {
        let mut loader = InMemoryLoader::new();
        loader.add(
            "base.html",
            "<title>{% block title %}{{ default_title }}{% endblock %}</title>",
        );
        loader.add(
            "child.html",
            "{% extends \"base.html\" %}{% block title %}{{ page_title }}{% endblock %}",
        );

        let mut engine = Engine::new(loader);
        let mut ctx = Context::new();
        ctx.set("page_title", Value::String("My Page".to_string()));

        let result = engine.render("child.html", &ctx).unwrap();
        assert_eq!(result, "<title>My Page</title>");
    }

    #[test]
    fn test_template_no_extends() {
        // Template without extends should render normally
        let mut loader = InMemoryLoader::new();
        loader.add("simple.html", "Hello {{ name }}!");

        let mut engine = Engine::new(loader);
        let mut ctx = Context::new();
        ctx.set("name", Value::String("World".to_string()));

        let result = engine.render("simple.html", &ctx).unwrap();
        assert_eq!(result, "Hello World!");
    }

    #[test]
    fn test_multiple_blocks() {
        let mut loader = InMemoryLoader::new();
        loader.add(
            "base.html",
            "{% block head %}head{% endblock %}|{% block body %}body{% endblock %}",
        );
        loader.add(
            "child.html",
            "{% extends \"base.html\" %}{% block body %}custom body{% endblock %}",
        );

        let mut engine = Engine::new(loader);
        let result = engine.render("child.html", &Context::new()).unwrap();
        // head block keeps default, body is overridden
        assert_eq!(result, "head|custom body");
    }

    #[test]
    fn test_global_function() {
        // Test calling a global function from a template
        let t = Template::parse("test", r#"{{ get_url(path="main.css") }}"#).unwrap();

        let mut ctx = Context::new();
        ctx.register_fn(
            "get_url",
            Box::new(|_args, kwargs| {
                // Simple get_url: prepend "/" to path
                let path = kwargs
                    .iter()
                    .find(|(k, _)| k == "path")
                    .map(|(_, v)| v.render_to_string())
                    .unwrap_or_default();
                Ok(Value::String(format!("/{path}")))
            }),
        );

        let result = t.render(&ctx).unwrap();
        assert_eq!(result, "/main.css");
    }

    #[test]
    fn test_macro_simple() {
        // Test a simple macro in the same template
        let t = Template::parse(
            "test",
            r#"{% macro greet(name) %}Hello, {{ name }}!{% endmacro %}{{ self::greet("World") }}"#,
        )
        .unwrap();
        let result = t.render(&Context::new()).unwrap();
        assert_eq!(result, "Hello, World!");
    }

    #[test]
    fn test_macro_with_default() {
        // Test a macro with default parameter value
        let t = Template::parse(
            "test",
            r#"{% macro greet(name="Guest") %}Hello, {{ name }}!{% endmacro %}{{ self::greet() }}"#,
        )
        .unwrap();
        let result = t.render(&Context::new()).unwrap();
        assert_eq!(result, "Hello, Guest!");
    }

    #[test]
    fn test_macro_override_default() {
        // Test calling a macro with defaults but providing a value
        let t = Template::parse(
            "test",
            r#"{% macro greet(name="Guest") %}Hello, {{ name }}!{% endmacro %}{{ self::greet("Alice") }}"#,
        )
        .unwrap();
        let result = t.render(&Context::new()).unwrap();
        assert_eq!(result, "Hello, Alice!");
    }

    #[test]
    fn test_macro_kwargs() {
        // Test calling a macro with keyword arguments
        let t = Template::parse(
            "test",
            r#"{% macro item(text, active=false) %}{% if active %}<b>{{ text }}</b>{% else %}{{ text }}{% endif %}{% endmacro %}{{ self::item("Home", active=true) }}"#,
        )
        .unwrap();
        let result = t.render(&Context::new()).unwrap();
        assert_eq!(result, "<b>Home</b>");
    }

    #[test]
    fn test_macro_import() {
        // Test importing macros from another template
        let mut loader = InMemoryLoader::new();
        loader.add(
            "macros.html",
            r#"{% macro button(text) %}<button>{{ text }}</button>{% endmacro %}"#,
        );
        loader.add(
            "page.html",
            r#"{% import "macros.html" as m %}{{ m::button("Click me") }}"#,
        );

        let mut engine = Engine::new(loader);
        let result = engine.render("page.html", &Context::new()).unwrap();
        assert_eq!(result, "<button>Click me</button>");
    }

    #[test]
    fn test_macro_multiple_calls() {
        // Test calling a macro multiple times
        let t = Template::parse(
            "test",
            r#"{% macro li(text) %}<li>{{ text }}</li>{% endmacro %}<ul>{{ self::li("One") }}{{ self::li("Two") }}{{ self::li("Three") }}</ul>"#,
        )
        .unwrap();
        let result = t.render(&Context::new()).unwrap();
        assert_eq!(result, "<ul><li>One</li><li>Two</li><li>Three</li></ul>");
    }
}
