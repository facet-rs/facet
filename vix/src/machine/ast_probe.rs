use std::collections::BTreeMap;
use std::sync::OnceLock;

use crate::VixParser;
use crate::ast::{self, FnItem, Item, SourceFile, Span, Type};
use crate::value::{Payload, Value};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub(super) enum Projection {
    Items,
    Fns,
    Fn,
    FnBodyChildren,
}

impl Projection {
    pub(super) fn name(self) -> &'static str {
        match self {
            Projection::Items => "items",
            Projection::Fns => "fns",
            Projection::Fn => "fn",
            Projection::FnBodyChildren => "fn.body.children",
        }
    }

    pub(super) fn to_word(self) -> i64 {
        match self {
            Projection::Items => 1000,
            Projection::Fns => 1001,
            Projection::Fn => 1002,
            Projection::FnBodyChildren => 1003,
        }
    }

    pub(super) fn from_word(word: i64) -> Result<Self, String> {
        Ok(match word {
            1000 => Projection::Items,
            1001 => Projection::Fns,
            1002 => Projection::Fn,
            1003 => Projection::FnBodyChildren,
            other => return Err(format!("unknown ast projection {other}")),
        })
    }
}

pub(super) fn parse(source: &str) -> Result<SourceFile, String> {
    static PARSER: OnceLock<VixParser> = OnceLock::new();
    PARSER
        .get_or_init(VixParser::new)
        .parse(source)
        .map_err(|err| err.message)
}

pub(super) fn items(file: &SourceFile) -> Value {
    Value::Array(file.items.iter().map(item_summary).collect())
}

pub(super) fn fns(file: &SourceFile) -> Value {
    Value::Array(
        file.items
            .iter()
            .filter_map(|item| match item {
                Item::Fn(item) => Some(fn_summary(item)),
                _ => None,
            })
            .collect(),
    )
}

pub(super) fn fn_item<'a>(file: &'a SourceFile, name: &str) -> Result<&'a FnItem, String> {
    file.items
        .iter()
        .find_map(|item| match item {
            Item::Fn(item) if item.name.value == name => Some(item.as_ref()),
            _ => None,
        })
        .ok_or_else(|| format!("no fn named {name}"))
}

pub(super) fn fn_body_children(item: &FnItem) -> Value {
    let mut children = item.body.stmts.iter().map(stmt_summary).collect::<Vec<_>>();
    if let Some(tail) = &item.body.tail {
        children.push(expr_summary("tail", tail));
    }
    Value::Array(children)
}

pub(super) fn fn_fields(item: &FnItem) -> BTreeMap<Value, Value> {
    BTreeMap::from([
        (string_key("kind"), Value::Str("fn".to_string())),
        (string_key("name"), Value::Str(item.name.value.clone())),
        (string_key("name_span"), span_value(item.name.span)),
        (string_key("public"), Value::Bool(item.vis.is_some())),
        (
            string_key("visibility_prefix"),
            Value::Str(if item.vis.is_some() { "pub " } else { "" }.to_string()),
        ),
        (string_key("span"), span_value(item.span)),
        (
            string_key("params"),
            Value::Array(item.params.params.iter().map(param_value).collect()),
        ),
        (
            string_key("param_types"),
            Value::Array(
                item.params
                    .params
                    .iter()
                    .map(|param| Value::Str(type_text(&param.ty)))
                    .collect(),
            ),
        ),
        (
            string_key("generic_params"),
            Value::Array(
                item.generics
                    .iter()
                    .flat_map(|generics| &generics.params)
                    .map(|param| Value::Str(param.value.clone()))
                    .collect(),
            ),
        ),
        (
            string_key("return_type"),
            item.return_type
                .as_ref()
                .map(type_value)
                .unwrap_or_else(option_none),
        ),
        (
            string_key("has_return_type"),
            Value::Bool(item.return_type.is_some()),
        ),
        (
            string_key("return_type_status"),
            Value::Str(
                if item.return_type.is_some() {
                    "some"
                } else {
                    "none"
                }
                .to_string(),
            ),
        ),
        (
            string_key("return_type_text"),
            item.return_type
                .as_ref()
                .map(|ty| Value::Str(type_text(ty)))
                .unwrap_or_else(option_none),
        ),
        (
            string_key("tail"),
            item.body
                .tail
                .as_ref()
                .map(|tail| expr_summary("tail", tail))
                .unwrap_or_else(option_none),
        ),
    ])
}

pub(super) fn span_value(span: Span) -> Value {
    Value::Map(BTreeMap::from([
        (string_key("start"), Value::Int(i64::from(span.start))),
        (string_key("end"), Value::Int(i64::from(span.end))),
    ]))
}

fn item_summary(item: &Item) -> Value {
    match item {
        Item::Use(item) => node_summary("use", None, item.span),
        Item::Fn(item) => fn_summary(item),
        Item::Struct(item) => node_summary("struct", Some(&item.name.value), item.span),
        Item::Enum(item) => node_summary("enum", Some(&item.name.value), item.span),
    }
}

fn fn_summary(item: &FnItem) -> Value {
    node_summary("fn", Some(&item.name.value), item.span)
}

fn stmt_summary(stmt: &ast::Stmt) -> Value {
    match stmt {
        ast::Stmt::Let(stmt) => node_summary("let", Some(&stmt.name.value), stmt.span),
        ast::Stmt::Expr(stmt) => node_summary("expr", None, stmt.span),
    }
}

fn expr_summary(role: &str, expr: &ast::Expr) -> Value {
    let mut fields = BTreeMap::from([
        (string_key("role"), Value::Str(role.to_string())),
        (string_key("kind"), Value::Str(expr_kind(expr).to_string())),
        (string_key("span"), span_value(expr_span(expr))),
    ]);
    if let Some(text) = expr_leaf_text(expr) {
        fields.insert(string_key("text"), Value::Str(text));
    }
    Value::Map(fields)
}

fn expr_kind(expr: &ast::Expr) -> &'static str {
    match expr {
        ast::Expr::Binary(_) => "binary",
        ast::Expr::Unary(_) => "unary",
        ast::Expr::Call(_) => "call",
        ast::Expr::MethodCall(_) => "method_call",
        ast::Expr::Field(_) => "field",
        ast::Expr::Match(_) => "match",
        ast::Expr::Closure(_) => "closure",
        ast::Expr::Command(_) => "command",
        ast::Expr::StructLit(_) => "struct_lit",
        ast::Expr::Map(_) => "map",
        ast::Expr::Tuple(_) => "tuple",
        ast::Expr::Array(_) => "array",
        ast::Expr::Paren(_) => "paren",
        ast::Expr::Scoped(_) => "scoped",
        ast::Expr::Identifier(_) => "identifier",
        ast::Expr::Str(_) => "string",
        ast::Expr::Template(_) => "template",
        ast::Expr::Path(_) => "path",
        ast::Expr::Number(_) => "number",
        ast::Expr::Bool(_) => "bool",
    }
}

fn expr_leaf_text(expr: &ast::Expr) -> Option<String> {
    match expr {
        ast::Expr::Identifier(expr)
        | ast::Expr::Str(expr)
        | ast::Expr::Template(expr)
        | ast::Expr::Path(expr)
        | ast::Expr::Number(expr) => Some(expr.value.clone()),
        ast::Expr::Bool(expr) => Some(expr.value.to_string()),
        _ => None,
    }
}

fn node_summary(kind: &str, name: Option<&str>, span: Span) -> Value {
    let mut fields = BTreeMap::from([
        (string_key("kind"), Value::Str(kind.to_string())),
        (string_key("span"), span_value(span)),
    ]);
    if let Some(name) = name {
        fields.insert(string_key("name"), Value::Str(name.to_string()));
    }
    Value::Map(fields)
}

fn param_value(param: &ast::Param) -> Value {
    Value::Map(BTreeMap::from([
        (string_key("name"), Value::Str(param.name.value.clone())),
        (string_key("type"), type_value(&param.ty)),
        (string_key("span"), span_value(param.span)),
    ]))
}

fn type_value(ty: &Type) -> Value {
    Value::Map(BTreeMap::from([
        (string_key("kind"), Value::Str(type_kind(ty).to_string())),
        (string_key("text"), Value::Str(type_text(ty))),
        (string_key("span"), span_value(type_span(ty))),
    ]))
}

fn type_kind(ty: &Type) -> &'static str {
    match ty {
        Type::Array(_) => "array",
        Type::Fn(_) => "fn",
        Type::Tuple(_) => "tuple",
        Type::Generic(_) => "generic",
        Type::Path(_) => "path",
    }
}

fn type_span(ty: &Type) -> Span {
    match ty {
        Type::Array(ty) => ty.span,
        Type::Fn(ty) => ty.span,
        Type::Tuple(ty) => ty.span,
        Type::Generic(ty) => ty.span,
        Type::Path(ty) => ty.span,
    }
}

fn type_text(ty: &Type) -> String {
    match ty {
        Type::Array(array) => format!("[{}]", type_text(&array.elem)),
        Type::Fn(func) => {
            let params = func
                .params
                .iter()
                .map(type_text)
                .collect::<Vec<_>>()
                .join(",");
            match &func.return_type {
                Some(ret) => format!("fn({params})->{}", type_text(ret)),
                None => format!("fn({params})"),
            }
        }
        Type::Tuple(tuple) => format!(
            "({})",
            tuple
                .elems
                .iter()
                .map(type_text)
                .collect::<Vec<_>>()
                .join(",")
        ),
        Type::Generic(generic) => format!(
            "{}<{}>",
            type_path_text(&generic.base),
            generic
                .args
                .iter()
                .map(type_text)
                .collect::<Vec<_>>()
                .join(",")
        ),
        Type::Path(path) => type_path_text(path),
    }
}

fn type_path_text(path: &ast::TypePath) -> String {
    path.segments
        .iter()
        .map(|segment| segment.value.as_str())
        .collect::<Vec<_>>()
        .join("::")
}

fn expr_span(expr: &ast::Expr) -> Span {
    match expr {
        ast::Expr::Binary(expr) => expr.span,
        ast::Expr::Unary(expr) => expr.span,
        ast::Expr::Call(expr) => expr.span,
        ast::Expr::MethodCall(expr) => expr.span,
        ast::Expr::Field(expr) => expr.span,
        ast::Expr::Match(expr) => expr.span,
        ast::Expr::Closure(expr) => expr.span,
        ast::Expr::Command(expr) => expr.span,
        ast::Expr::StructLit(expr) => expr.span,
        ast::Expr::Map(expr) => expr.span,
        ast::Expr::Tuple(expr) => expr.span,
        ast::Expr::Array(expr) => expr.span,
        ast::Expr::Paren(expr) => expr.span,
        ast::Expr::Scoped(expr) => expr.span,
        ast::Expr::Identifier(expr)
        | ast::Expr::Template(expr)
        | ast::Expr::Str(expr)
        | ast::Expr::Path(expr)
        | ast::Expr::Number(expr) => expr.span,
        ast::Expr::Bool(expr) => expr.span,
    }
}

fn option_none() -> Value {
    Value::Variant {
        enum_name: "Option".to_string(),
        index: 1,
        name: "None".to_string(),
        payload: Payload::Unit,
    }
}

fn string_key(key: &str) -> Value {
    Value::Str(key.to_string())
}
