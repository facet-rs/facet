//! Template renderer
//!
//! Renders templates to strings using a context.
//! This is the main public API for the template engine.

use super::ast::{self, Node, Target};
use super::error::TemplateSource;
use super::eval::{Context, Evaluator, Value};
use super::parser::Parser;
use miette::Result;

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

    /// Render the template with the given context
    pub fn render(&self, ctx: &Context) -> Result<String> {
        let mut output = String::new();
        let mut renderer = Renderer {
            ctx: ctx.clone(),
            source: &self.source,
            output: &mut output,
        };
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

/// Internal renderer state
struct Renderer<'a> {
    ctx: Context,
    source: &'a TemplateSource,
    output: &'a mut String,
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
                let eval = Evaluator::new(&self.ctx, self.source);
                let value = eval.eval(&print.expr)?;
                // Auto-escape by default (for HTML safety)
                let s = html_escape(&value.to_string());
                self.output.push_str(&s);
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
            Node::Block(_block) => {
                // TODO: Template inheritance
                self.output.push_str("<!-- block not implemented -->");
            }
            Node::Extends(_extends) => {
                // TODO: Template inheritance
            }
            Node::Comment(_) => {
                // Comments are not rendered
            }
        }

        Ok(())
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
}
