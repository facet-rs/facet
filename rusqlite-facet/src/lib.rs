use facet::Facet;
use facet_core::{Def, Shape, StructKind, Type, UserType};
use facet_reflect::{AllocError, HasFields, Partial, Peek, ReflectError, ShapeMismatchError};
use rusqlite::types::{Type as SqlType, Value as SqlValue, ValueRef};
use rusqlite::{Connection, Row, Rows, Statement};

#[derive(Debug)]
pub enum Error {
    Sql(rusqlite::Error),
    Reflect(ReflectError),
    Alloc(AllocError),
    ShapeMismatch(ShapeMismatchError),
    NotAStruct {
        shape: &'static Shape,
    },
    UnsupportedParamType {
        field: String,
        shape: &'static Shape,
    },
    UnsupportedRowType {
        field: String,
        shape: &'static Shape,
    },
    MissingNamedParam {
        parameter: String,
    },
    MissingColumn {
        column: String,
    },
    UnnamedParameter {
        index: usize,
    },
    UnusedParamFields {
        fields: Vec<String>,
    },
    UnusedPositionalParams {
        provided: usize,
        used: usize,
    },
    TooManyRows {
        expected: usize,
        actual_at_least: usize,
    },
    WithSqlContext {
        sql: String,
        source: Box<Error>,
    },
    OutOfRange {
        field: String,
        source: i128,
        target: &'static str,
    },
    TypeMismatch {
        field: String,
        expected: &'static Shape,
        actual: SqlType,
    },
}

impl core::fmt::Display for Error {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            Error::Sql(e) => write!(f, "sqlite error: {e}"),
            Error::Reflect(e) => write!(f, "reflection error: {e}"),
            Error::Alloc(e) => write!(f, "allocation error: {e}"),
            Error::ShapeMismatch(e) => write!(f, "shape mismatch: {e}"),
            Error::NotAStruct { shape } => write!(f, "expected a struct shape, got {shape}"),
            Error::UnsupportedParamType { field, shape } => {
                write!(f, "unsupported parameter type for field '{field}': {shape}")
            }
            Error::UnsupportedRowType { field, shape } => {
                write!(f, "unsupported row type for field '{field}': {shape}")
            }
            Error::MissingNamedParam { parameter } => {
                write!(f, "missing named parameter for SQL binding: {parameter}")
            }
            Error::MissingColumn { column } => write!(f, "missing required column: {column}"),
            Error::UnnamedParameter { index } => {
                write!(f, "statement parameter #{index} is unnamed")
            }
            Error::UnusedParamFields { fields } => {
                write!(f, "unused parameter fields: {}", fields.join(", "))
            }
            Error::UnusedPositionalParams { provided, used } => {
                write!(
                    f,
                    "unused positional parameters: provided {provided}, used {used}"
                )
            }
            Error::TooManyRows {
                expected,
                actual_at_least,
            } => {
                write!(
                    f,
                    "query returned too many rows: expected {expected}, got at least {actual_at_least}"
                )
            }
            Error::WithSqlContext { sql, source } => {
                write!(f, "{source} (sql: {sql})")
            }
            Error::OutOfRange {
                field,
                source,
                target,
            } => {
                write!(
                    f,
                    "out-of-range conversion for field '{field}': {source} cannot fit in {target}"
                )
            }
            Error::TypeMismatch {
                field,
                expected,
                actual,
            } => {
                write!(
                    f,
                    "type mismatch for field '{field}': expected {expected}, got {actual:?}"
                )
            }
        }
    }
}

impl std::error::Error for Error {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Error::Sql(err) => Some(err),
            Error::Reflect(err) => Some(err),
            Error::Alloc(err) => Some(err),
            Error::ShapeMismatch(err) => Some(err),
            Error::WithSqlContext { source, .. } => Some(source),
            _ => None,
        }
    }
}

impl From<rusqlite::Error> for Error {
    fn from(value: rusqlite::Error) -> Self {
        Self::Sql(value)
    }
}

impl From<ReflectError> for Error {
    fn from(value: ReflectError) -> Self {
        Self::Reflect(value)
    }
}

impl From<AllocError> for Error {
    fn from(value: AllocError) -> Self {
        Self::Alloc(value)
    }
}

impl From<ShapeMismatchError> for Error {
    fn from(value: ShapeMismatchError) -> Self {
        Self::ShapeMismatch(value)
    }
}

pub type Result<T> = core::result::Result<T, Error>;

pub struct FacetRows<'stmt, T> {
    rows: Rows<'stmt>,
    _marker: core::marker::PhantomData<T>,
}

impl<T: Facet<'static>> Iterator for FacetRows<'_, T> {
    type Item = Result<T>;

    fn next(&mut self) -> Option<Self::Item> {
        match self.rows.next() {
            Ok(Some(row)) => Some(from_row::<T>(row)),
            Ok(None) => None,
            Err(err) => Some(Err(Error::Sql(err))),
        }
    }
}

pub trait StatementFacetExt {
    fn facet_execute_ref<'p, P: Facet<'p> + ?Sized>(&mut self, params: &'p P) -> Result<usize>;
    fn facet_query_iter_ref<'stmt, 'p, T: Facet<'static>, P: Facet<'p> + ?Sized>(
        &'stmt mut self,
        params: &'p P,
    ) -> Result<FacetRows<'stmt, T>>;
    fn facet_query_ref<'p, T: Facet<'static>, P: Facet<'p> + ?Sized>(
        &mut self,
        params: &'p P,
    ) -> Result<Vec<T>>;
    fn facet_query_optional_ref<'p, T: Facet<'static>, P: Facet<'p> + ?Sized>(
        &mut self,
        params: &'p P,
    ) -> Result<Option<T>>;
    fn facet_query_one_ref<'p, T: Facet<'static>, P: Facet<'p> + ?Sized>(
        &mut self,
        params: &'p P,
    ) -> Result<T>;
    fn facet_query_row_ref<'p, T: Facet<'static>, P: Facet<'p> + ?Sized>(
        &mut self,
        params: &'p P,
    ) -> Result<T>;
    fn facet_execute<P: Facet<'static>>(&mut self, params: P) -> Result<usize>;
    fn facet_query_iter<'stmt, T: Facet<'static>, P: Facet<'static>>(
        &'stmt mut self,
        params: P,
    ) -> Result<FacetRows<'stmt, T>>;
    fn facet_query<T: Facet<'static>, P: Facet<'static>>(&mut self, params: P) -> Result<Vec<T>>;
    fn facet_query_optional<T: Facet<'static>, P: Facet<'static>>(
        &mut self,
        params: P,
    ) -> Result<Option<T>>;
    fn facet_query_one<T: Facet<'static>, P: Facet<'static>>(&mut self, params: P) -> Result<T>;
    fn facet_query_row<T: Facet<'static>, P: Facet<'static>>(&mut self, params: P) -> Result<T>;
}

pub trait ConnectionFacetExt {
    fn facet_execute_ref<'p, P: Facet<'p> + ?Sized>(
        &self,
        sql: &str,
        params: &'p P,
    ) -> Result<usize>;
    fn facet_query_ref<'p, T: Facet<'static>, P: Facet<'p> + ?Sized>(
        &self,
        sql: &str,
        params: &'p P,
    ) -> Result<Vec<T>>;
    fn facet_query_optional_ref<'p, T: Facet<'static>, P: Facet<'p> + ?Sized>(
        &self,
        sql: &str,
        params: &'p P,
    ) -> Result<Option<T>>;
    fn facet_query_one_ref<'p, T: Facet<'static>, P: Facet<'p> + ?Sized>(
        &self,
        sql: &str,
        params: &'p P,
    ) -> Result<T>;
    fn facet_execute<P: Facet<'static>>(&self, sql: &str, params: P) -> Result<usize>;
    fn facet_query<T: Facet<'static>, P: Facet<'static>>(
        &self,
        sql: &str,
        params: P,
    ) -> Result<Vec<T>>;
    fn facet_query_optional<T: Facet<'static>, P: Facet<'static>>(
        &self,
        sql: &str,
        params: P,
    ) -> Result<Option<T>>;
    fn facet_query_one<T: Facet<'static>, P: Facet<'static>>(
        &self,
        sql: &str,
        params: P,
    ) -> Result<T>;
}

impl StatementFacetExt for Statement<'_> {
    fn facet_execute_ref<'p, P: Facet<'p> + ?Sized>(&mut self, params: &'p P) -> Result<usize> {
        bind_facet_params_ref(self, params)?;
        Ok(self.raw_execute()?)
    }

    fn facet_query_iter_ref<'stmt, 'p, T: Facet<'static>, P: Facet<'p> + ?Sized>(
        &'stmt mut self,
        params: &'p P,
    ) -> Result<FacetRows<'stmt, T>> {
        bind_facet_params_ref(self, params)?;
        Ok(FacetRows {
            rows: self.raw_query(),
            _marker: core::marker::PhantomData,
        })
    }

    fn facet_query_ref<'p, T: Facet<'static>, P: Facet<'p> + ?Sized>(
        &mut self,
        params: &'p P,
    ) -> Result<Vec<T>> {
        let mut out = Vec::new();
        for row in self.facet_query_iter_ref::<T, P>(params)? {
            out.push(row?);
        }
        Ok(out)
    }

    fn facet_query_optional_ref<'p, T: Facet<'static>, P: Facet<'p> + ?Sized>(
        &mut self,
        params: &'p P,
    ) -> Result<Option<T>> {
        bind_facet_params_ref(self, params)?;
        let mut rows = self.raw_query();
        let Some(first_row) = rows.next()? else {
            return Ok(None);
        };
        let first = from_row::<T>(first_row)?;
        if rows.next()?.is_some() {
            return Err(Error::TooManyRows {
                expected: 1,
                actual_at_least: 2,
            });
        }
        Ok(Some(first))
    }

    fn facet_query_one_ref<'p, T: Facet<'static>, P: Facet<'p> + ?Sized>(
        &mut self,
        params: &'p P,
    ) -> Result<T> {
        match self.facet_query_optional_ref::<T, P>(params)? {
            Some(row) => Ok(row),
            None => Err(Error::Sql(rusqlite::Error::QueryReturnedNoRows)),
        }
    }

    fn facet_query_row_ref<'p, T: Facet<'static>, P: Facet<'p> + ?Sized>(
        &mut self,
        params: &'p P,
    ) -> Result<T> {
        self.facet_query_one_ref::<T, P>(params)
    }

    fn facet_execute<P: Facet<'static>>(&mut self, params: P) -> Result<usize> {
        bind_facet_params_static(self, &params)?;
        Ok(self.raw_execute()?)
    }

    fn facet_query_iter<'stmt, T: Facet<'static>, P: Facet<'static>>(
        &'stmt mut self,
        params: P,
    ) -> Result<FacetRows<'stmt, T>> {
        bind_facet_params_static(self, &params)?;
        Ok(FacetRows {
            rows: self.raw_query(),
            _marker: core::marker::PhantomData,
        })
    }

    fn facet_query<T: Facet<'static>, P: Facet<'static>>(&mut self, params: P) -> Result<Vec<T>> {
        let mut out = Vec::new();
        for row in self.facet_query_iter::<T, P>(params)? {
            out.push(row?);
        }
        Ok(out)
    }

    fn facet_query_optional<T: Facet<'static>, P: Facet<'static>>(
        &mut self,
        params: P,
    ) -> Result<Option<T>> {
        bind_facet_params_static(self, &params)?;
        let mut rows = self.raw_query();
        let Some(first_row) = rows.next()? else {
            return Ok(None);
        };
        let first = from_row::<T>(first_row)?;
        if rows.next()?.is_some() {
            return Err(Error::TooManyRows {
                expected: 1,
                actual_at_least: 2,
            });
        }
        Ok(Some(first))
    }

    fn facet_query_one<T: Facet<'static>, P: Facet<'static>>(&mut self, params: P) -> Result<T> {
        match self.facet_query_optional::<T, P>(params)? {
            Some(row) => Ok(row),
            None => Err(Error::Sql(rusqlite::Error::QueryReturnedNoRows)),
        }
    }

    fn facet_query_row<T: Facet<'static>, P: Facet<'static>>(&mut self, params: P) -> Result<T> {
        self.facet_query_one::<T, P>(params)
    }
}

impl ConnectionFacetExt for Connection {
    fn facet_execute_ref<'p, P: Facet<'p> + ?Sized>(
        &self,
        sql: &str,
        params: &'p P,
    ) -> Result<usize> {
        let mut stmt = self.prepare(sql)?;
        with_sql_context(sql, stmt.facet_execute_ref(params))
    }

    fn facet_query_ref<'p, T: Facet<'static>, P: Facet<'p> + ?Sized>(
        &self,
        sql: &str,
        params: &'p P,
    ) -> Result<Vec<T>> {
        let mut stmt = self.prepare(sql)?;
        with_sql_context(sql, stmt.facet_query_ref::<T, P>(params))
    }

    fn facet_query_optional_ref<'p, T: Facet<'static>, P: Facet<'p> + ?Sized>(
        &self,
        sql: &str,
        params: &'p P,
    ) -> Result<Option<T>> {
        let mut stmt = self.prepare(sql)?;
        with_sql_context(sql, stmt.facet_query_optional_ref::<T, P>(params))
    }

    fn facet_query_one_ref<'p, T: Facet<'static>, P: Facet<'p> + ?Sized>(
        &self,
        sql: &str,
        params: &'p P,
    ) -> Result<T> {
        let mut stmt = self.prepare(sql)?;
        with_sql_context(sql, stmt.facet_query_one_ref::<T, P>(params))
    }

    fn facet_execute<P: Facet<'static>>(&self, sql: &str, params: P) -> Result<usize> {
        let mut stmt = self.prepare(sql)?;
        with_sql_context(sql, stmt.facet_execute(params))
    }

    fn facet_query<T: Facet<'static>, P: Facet<'static>>(
        &self,
        sql: &str,
        params: P,
    ) -> Result<Vec<T>> {
        let mut stmt = self.prepare(sql)?;
        with_sql_context(sql, stmt.facet_query::<T, P>(params))
    }

    fn facet_query_optional<T: Facet<'static>, P: Facet<'static>>(
        &self,
        sql: &str,
        params: P,
    ) -> Result<Option<T>> {
        let mut stmt = self.prepare(sql)?;
        with_sql_context(sql, stmt.facet_query_optional::<T, P>(params))
    }

    fn facet_query_one<T: Facet<'static>, P: Facet<'static>>(
        &self,
        sql: &str,
        params: P,
    ) -> Result<T> {
        let mut stmt = self.prepare(sql)?;
        with_sql_context(sql, stmt.facet_query_one::<T, P>(params))
    }
}

fn with_sql_context<T>(sql: &str, result: Result<T>) -> Result<T> {
    result.map_err(|source| Error::WithSqlContext {
        sql: sql.to_string(),
        source: Box::new(source),
    })
}

pub fn from_row<T: Facet<'static>>(row: &Row<'_>) -> Result<T> {
    let partial = Partial::alloc_owned::<T>()?;
    let partial = deserialize_row_into(row, partial, T::SHAPE)?;
    let heap_value = partial.build()?;
    Ok(heap_value.materialize()?)
}

fn bind_facet_params_static<P: Facet<'static>>(stmt: &mut Statement<'_>, params: &P) -> Result<()> {
    bind_facet_params_impl(stmt, Peek::new(params), P::SHAPE)
}

fn bind_facet_params_ref<'p, P: Facet<'p> + ?Sized>(
    stmt: &mut Statement<'_>,
    params: &'p P,
) -> Result<()> {
    bind_facet_params_impl(stmt, Peek::new(params), P::SHAPE)
}

fn bind_facet_params_impl(
    stmt: &mut Statement<'_>,
    peek: Peek<'_, '_>,
    shape: &'static Shape,
) -> Result<()> {
    stmt.clear_bindings();

    if matches!(
        peek.shape().def,
        Def::List(_) | Def::Array(_) | Def::Slice(_)
    ) {
        return bind_list_like_params(stmt, peek);
    }

    let struct_peek = peek
        .into_struct()
        .map_err(|_| Error::NotAStruct { shape })?;

    let mut field_names: Vec<String> = Vec::new();
    let mut field_values: Vec<SqlValue> = Vec::new();
    for (field, value) in struct_peek.fields() {
        let name = field.rename.unwrap_or(field.name).to_string();
        field_names.push(name.clone());
        field_values.push(peek_to_sql_value(value, &name)?);
    }

    let mut used = vec![false; field_names.len()];
    let mut positional_cursor = 0usize;
    for param_index in 1..=stmt.parameter_count() {
        let field_index = if let Some(name) = stmt.parameter_name(param_index) {
            if let Some(stripped) = name.strip_prefix(':') {
                field_names
                    .iter()
                    .position(|f| f == stripped)
                    .ok_or_else(|| Error::MissingNamedParam {
                        parameter: name.to_string(),
                    })?
            } else if let Some(stripped) = name.strip_prefix('@') {
                field_names
                    .iter()
                    .position(|f| f == stripped)
                    .ok_or_else(|| Error::MissingNamedParam {
                        parameter: name.to_string(),
                    })?
            } else if let Some(stripped) = name.strip_prefix('$') {
                field_names
                    .iter()
                    .position(|f| f == stripped)
                    .ok_or_else(|| Error::MissingNamedParam {
                        parameter: name.to_string(),
                    })?
            } else if let Some(stripped) = name.strip_prefix('?') {
                if stripped.is_empty() {
                    let idx = positional_cursor;
                    positional_cursor += 1;
                    idx
                } else {
                    let raw = stripped
                        .parse::<usize>()
                        .map_err(|_| Error::MissingNamedParam {
                            parameter: name.to_string(),
                        })?;
                    raw.saturating_sub(1)
                }
            } else {
                return Err(Error::UnnamedParameter { index: param_index });
            }
        } else {
            if positional_cursor >= field_values.len() {
                return Err(Error::UnnamedParameter { index: param_index });
            }
            let idx = positional_cursor;
            positional_cursor += 1;
            idx
        };

        let value = field_values
            .get(field_index)
            .ok_or(Error::UnnamedParameter { index: param_index })?;

        stmt.raw_bind_parameter(param_index, value)?;
        used[field_index] = true;
    }

    let unused: Vec<String> = field_names
        .iter()
        .enumerate()
        .filter_map(|(idx, name)| (!used[idx]).then_some(name.clone()))
        .collect();
    if !unused.is_empty() {
        return Err(Error::UnusedParamFields { fields: unused });
    }

    Ok(())
}

fn bind_list_like_params(stmt: &mut Statement<'_>, peek: Peek<'_, '_>) -> Result<()> {
    let list_like = peek.into_list_like().map_err(Error::Reflect)?;
    let mut values = Vec::with_capacity(list_like.len());
    for value in list_like.iter() {
        values.push(peek_to_sql_value(value, "positional_param")?);
    }

    let mut used = vec![false; values.len()];
    let mut positional_cursor = 0usize;
    for param_index in 1..=stmt.parameter_count() {
        let value_index = if let Some(name) = stmt.parameter_name(param_index) {
            if let Some(stripped) = name.strip_prefix('?') {
                if stripped.is_empty() {
                    let idx = positional_cursor;
                    positional_cursor += 1;
                    idx
                } else {
                    let raw = stripped
                        .parse::<usize>()
                        .map_err(|_| Error::MissingNamedParam {
                            parameter: name.to_string(),
                        })?;
                    raw.saturating_sub(1)
                }
            } else {
                return Err(Error::MissingNamedParam {
                    parameter: name.to_string(),
                });
            }
        } else {
            let idx = positional_cursor;
            positional_cursor += 1;
            idx
        };

        let value = values
            .get(value_index)
            .ok_or(Error::UnnamedParameter { index: param_index })?;
        stmt.raw_bind_parameter(param_index, value)?;
        used[value_index] = true;
    }

    let used_count = used.iter().filter(|v| **v).count();
    if used_count != values.len() {
        return Err(Error::UnusedPositionalParams {
            provided: values.len(),
            used: used_count,
        });
    }

    Ok(())
}

fn peek_to_sql_value(peek: Peek<'_, '_>, field_name: &str) -> Result<SqlValue> {
    if let Ok(option) = peek.into_option() {
        let Some(inner) = option.value() else {
            return Ok(SqlValue::Null);
        };
        return peek_to_sql_value(inner, field_name);
    }

    let peek = peek.innermost_peek();
    if peek.shape() == bool::SHAPE {
        return Ok(SqlValue::Integer(i64::from(*peek.get::<bool>()?)));
    }
    if peek.shape() == i8::SHAPE {
        return Ok(SqlValue::Integer(i64::from(*peek.get::<i8>()?)));
    }
    if peek.shape() == i16::SHAPE {
        return Ok(SqlValue::Integer(i64::from(*peek.get::<i16>()?)));
    }
    if peek.shape() == i32::SHAPE {
        return Ok(SqlValue::Integer(i64::from(*peek.get::<i32>()?)));
    }
    if peek.shape() == i64::SHAPE {
        return Ok(SqlValue::Integer(*peek.get::<i64>()?));
    }
    if peek.shape() == u8::SHAPE {
        return Ok(SqlValue::Integer(i64::from(*peek.get::<u8>()?)));
    }
    if peek.shape() == u16::SHAPE {
        return Ok(SqlValue::Integer(i64::from(*peek.get::<u16>()?)));
    }
    if peek.shape() == u32::SHAPE {
        return Ok(SqlValue::Integer(i64::from(*peek.get::<u32>()?)));
    }
    if peek.shape() == u64::SHAPE {
        let value = *peek.get::<u64>()?;
        let value = i64::try_from(value).map_err(|_| Error::OutOfRange {
            field: field_name.to_string(),
            source: value as i128,
            target: "i64",
        })?;
        return Ok(SqlValue::Integer(value));
    }
    if peek.shape() == f32::SHAPE {
        return Ok(SqlValue::Real(f64::from(*peek.get::<f32>()?)));
    }
    if peek.shape() == f64::SHAPE {
        return Ok(SqlValue::Real(*peek.get::<f64>()?));
    }
    if peek.shape() == String::SHAPE {
        return Ok(SqlValue::Text(peek.get::<String>()?.clone()));
    }
    if peek.shape() == <Vec<u8>>::SHAPE {
        return Ok(SqlValue::Blob(peek.get::<Vec<u8>>()?.clone()));
    }
    if let Some(text) = peek.as_str() {
        return Ok(SqlValue::Text(text.to_string()));
    }

    Err(Error::UnsupportedParamType {
        field: field_name.to_string(),
        shape: peek.shape(),
    })
}

fn deserialize_row_into(
    row: &Row<'_>,
    mut partial: Partial<'static, false>,
    shape: &'static Shape,
) -> Result<Partial<'static, false>> {
    let struct_def = match &shape.ty {
        Type::User(UserType::Struct(s)) if s.kind == StructKind::Struct => s,
        _ => return Err(Error::NotAStruct { shape }),
    };

    for field in struct_def.fields {
        let column_name = field.rename.unwrap_or(field.name);
        let column_idx =
            find_column_index(row, column_name).ok_or_else(|| Error::MissingColumn {
                column: column_name.to_string(),
            })?;

        partial = partial.begin_field(field.name)?;
        partial = deserialize_column(row, column_idx, column_name, partial, field.shape())?;
        partial = partial.end()?;
    }

    Ok(partial)
}

fn find_column_index(row: &Row<'_>, column_name: &str) -> Option<usize> {
    let stmt = row.as_ref();
    (0..stmt.column_count()).find(|idx| {
        stmt.column_name(*idx)
            .map(|name| name == column_name)
            .unwrap_or(false)
    })
}

fn deserialize_column(
    row: &Row<'_>,
    column_idx: usize,
    field_name: &str,
    mut partial: Partial<'static, false>,
    shape: &'static Shape,
) -> Result<Partial<'static, false>> {
    if shape.decl_id == Option::<()>::SHAPE.decl_id {
        let value_ref = row.get_ref(column_idx)?;
        if matches!(value_ref, ValueRef::Null) {
            partial = partial.set_default()?;
            return Ok(partial);
        }

        let inner = shape.inner.expect("Option shape must have inner");
        partial = partial.begin_some()?;
        partial = deserialize_column(row, column_idx, field_name, partial, inner)?;
        partial = partial.end()?;
        return Ok(partial);
    }

    let value_ref = row.get_ref(column_idx)?;
    if matches!(value_ref, ValueRef::Null) {
        return Err(Error::TypeMismatch {
            field: field_name.to_string(),
            expected: shape,
            actual: SqlType::Null,
        });
    }

    if shape == bool::SHAPE {
        partial = partial.set(row.get::<_, bool>(column_idx)?)?;
    } else if shape == i8::SHAPE {
        partial = partial.set(row.get::<_, i8>(column_idx)?)?;
    } else if shape == i16::SHAPE {
        partial = partial.set(row.get::<_, i16>(column_idx)?)?;
    } else if shape == i32::SHAPE {
        partial = partial.set(row.get::<_, i32>(column_idx)?)?;
    } else if shape == i64::SHAPE {
        partial = partial.set(row.get::<_, i64>(column_idx)?)?;
    } else if shape == u8::SHAPE {
        partial = partial.set(checked_unsigned::<u8>(
            row.get::<_, i64>(column_idx)?,
            field_name,
        )?)?;
    } else if shape == u16::SHAPE {
        partial = partial.set(checked_unsigned::<u16>(
            row.get::<_, i64>(column_idx)?,
            field_name,
        )?)?;
    } else if shape == u32::SHAPE {
        partial = partial.set(checked_unsigned::<u32>(
            row.get::<_, i64>(column_idx)?,
            field_name,
        )?)?;
    } else if shape == u64::SHAPE {
        partial = partial.set(checked_unsigned::<u64>(
            row.get::<_, i64>(column_idx)?,
            field_name,
        )?)?;
    } else if shape == f32::SHAPE {
        partial = partial.set(row.get::<_, f32>(column_idx)?)?;
    } else if shape == f64::SHAPE {
        partial = partial.set(row.get::<_, f64>(column_idx)?)?;
    } else if shape == String::SHAPE {
        partial = partial.set(row.get::<_, String>(column_idx)?)?;
    } else if shape == <Vec<u8>>::SHAPE {
        partial = partial.set(row.get::<_, Vec<u8>>(column_idx)?)?;
    } else if shape.vtable.has_parse() {
        let raw: String = row.get(column_idx)?;
        partial = partial.parse_from_str(&raw)?;
    } else {
        return Err(Error::UnsupportedRowType {
            field: field_name.to_string(),
            shape,
        });
    }

    Ok(partial)
}

fn checked_unsigned<T>(value: i64, field_name: &str) -> Result<T>
where
    T: TryFrom<i64>,
{
    T::try_from(value).map_err(|_| Error::OutOfRange {
        field: field_name.to_string(),
        source: value as i128,
        target: core::any::type_name::<T>(),
    })
}

#[cfg(test)]
mod tests {
    use super::{ConnectionFacetExt, Error, StatementFacetExt};
    use facet::Facet;
    use rusqlite::Connection;

    #[derive(Debug, Facet, PartialEq)]
    struct InsertConn {
        conn_id: i64,
        label: String,
    }

    #[derive(Debug, Facet, PartialEq)]
    struct RowConn {
        conn_id: u64,
        label: String,
    }

    #[derive(Debug, Facet)]
    struct QueryConn {
        conn_id: i64,
    }

    #[derive(Debug, Facet, PartialEq)]
    struct MaybeConn {
        conn_id: i64,
        label: Option<String>,
    }

    #[test]
    fn facet_execute_and_query_named_params() {
        let conn = Connection::open_in_memory().unwrap();
        conn.execute(
            "CREATE TABLE connections (conn_id INTEGER NOT NULL, label TEXT)",
            (),
        )
        .unwrap();

        let mut insert = conn
            .prepare("INSERT INTO connections (conn_id, label) VALUES (:conn_id, :label)")
            .unwrap();
        insert
            .facet_execute(InsertConn {
                conn_id: 42,
                label: "alpha".to_string(),
            })
            .unwrap();

        let mut query = conn
            .prepare("SELECT conn_id, label FROM connections WHERE conn_id = :conn_id")
            .unwrap();
        let rows = query
            .facet_query::<RowConn, _>(QueryConn { conn_id: 42 })
            .unwrap();
        assert_eq!(
            rows,
            vec![RowConn {
                conn_id: 42,
                label: "alpha".to_string()
            }]
        );
    }

    #[test]
    fn facet_query_positional_params_and_option() {
        let conn = Connection::open_in_memory().unwrap();
        conn.execute(
            "CREATE TABLE items (conn_id INTEGER NOT NULL, label TEXT)",
            (),
        )
        .unwrap();
        conn.execute("INSERT INTO items (conn_id, label) VALUES (1, NULL)", ())
            .unwrap();

        #[derive(Facet)]
        struct Positional {
            conn_id: i64,
        }

        let mut stmt = conn
            .prepare("SELECT conn_id, label FROM items WHERE conn_id = ?1")
            .unwrap();
        let rows = stmt
            .facet_query::<MaybeConn, _>(Positional { conn_id: 1 })
            .unwrap();
        assert_eq!(
            rows,
            vec![MaybeConn {
                conn_id: 1,
                label: None
            }]
        );
    }

    #[test]
    fn facet_query_accepts_array_params() {
        let conn = Connection::open_in_memory().unwrap();
        conn.execute(
            "CREATE TABLE pairs (left_id INTEGER NOT NULL, right_id INTEGER NOT NULL)",
            (),
        )
        .unwrap();
        conn.execute("INSERT INTO pairs (left_id, right_id) VALUES (10, 20)", ())
            .unwrap();

        #[derive(Debug, Facet, PartialEq)]
        struct PairRow {
            left_id: i64,
            right_id: i64,
        }

        let mut stmt = conn
            .prepare("SELECT left_id, right_id FROM pairs WHERE left_id = ?1 AND right_id = ?2")
            .unwrap();
        let rows = stmt.facet_query::<PairRow, _>([10_i64, 20_i64]).unwrap();
        assert_eq!(
            rows,
            vec![PairRow {
                left_id: 10,
                right_id: 20
            }]
        );
    }

    #[test]
    fn facet_query_ref_accepts_slice_params() {
        let conn = Connection::open_in_memory().unwrap();
        conn.execute("CREATE TABLE ids (id INTEGER NOT NULL)", ())
            .unwrap();
        conn.execute("INSERT INTO ids (id) VALUES (7)", ()).unwrap();

        #[derive(Debug, Facet, PartialEq)]
        struct IdRow {
            id: i64,
        }

        let values = [7_i64];
        let mut stmt = conn.prepare("SELECT id FROM ids WHERE id = ?1").unwrap();
        let rows = stmt.facet_query_ref::<IdRow, [i64]>(&values[..]).unwrap();
        assert_eq!(rows, vec![IdRow { id: 7 }]);
    }

    #[test]
    fn facet_query_iter_streams_rows() {
        let conn = Connection::open_in_memory().unwrap();
        conn.execute("CREATE TABLE nums (n INTEGER NOT NULL)", ())
            .unwrap();
        conn.execute("INSERT INTO nums (n) VALUES (1)", ()).unwrap();
        conn.execute("INSERT INTO nums (n) VALUES (2)", ()).unwrap();

        #[derive(Debug, Facet, PartialEq)]
        struct NumRow {
            n: i64,
        }

        let mut stmt = conn.prepare("SELECT n FROM nums ORDER BY n ASC").unwrap();
        let mut iter = stmt.facet_query_iter::<NumRow, _>(()).unwrap();
        assert_eq!(iter.next().unwrap().unwrap(), NumRow { n: 1 });
        assert_eq!(iter.next().unwrap().unwrap(), NumRow { n: 2 });
        assert!(iter.next().is_none());
    }

    #[test]
    fn facet_query_iter_ref_streams_slice_params() {
        let conn = Connection::open_in_memory().unwrap();
        conn.execute("CREATE TABLE ids (id INTEGER NOT NULL)", ())
            .unwrap();
        conn.execute("INSERT INTO ids (id) VALUES (7)", ()).unwrap();

        #[derive(Debug, Facet, PartialEq)]
        struct IdRow {
            id: i64,
        }

        let values = [7_i64];
        let mut stmt = conn.prepare("SELECT id FROM ids WHERE id = ?1").unwrap();
        let mut iter = stmt
            .facet_query_iter_ref::<IdRow, [i64]>(&values[..])
            .unwrap();
        assert_eq!(iter.next().unwrap().unwrap(), IdRow { id: 7 });
        assert!(iter.next().is_none());
    }

    #[test]
    fn facet_query_optional_returns_none_for_no_rows() {
        let conn = Connection::open_in_memory().unwrap();
        conn.execute("CREATE TABLE ids (id INTEGER NOT NULL)", ())
            .unwrap();

        #[derive(Facet)]
        struct QueryId {
            id: i64,
        }

        #[derive(Debug, Facet, PartialEq)]
        struct IdRow {
            id: i64,
        }

        let mut stmt = conn.prepare("SELECT id FROM ids WHERE id = :id").unwrap();
        let row = stmt
            .facet_query_optional::<IdRow, _>(QueryId { id: 99 })
            .unwrap();
        assert_eq!(row, None);
    }

    #[test]
    fn facet_query_optional_errors_on_multiple_rows() {
        let conn = Connection::open_in_memory().unwrap();
        conn.execute("CREATE TABLE ids (id INTEGER NOT NULL)", ())
            .unwrap();
        conn.execute("INSERT INTO ids (id) VALUES (1)", ()).unwrap();
        conn.execute("INSERT INTO ids (id) VALUES (1)", ()).unwrap();

        #[derive(Facet)]
        struct QueryId {
            id: i64,
        }

        #[derive(Debug, Facet, PartialEq)]
        struct IdRow {
            id: i64,
        }

        let mut stmt = conn.prepare("SELECT id FROM ids WHERE id = :id").unwrap();
        let err = stmt
            .facet_query_optional::<IdRow, _>(QueryId { id: 1 })
            .unwrap_err();
        match err {
            Error::TooManyRows {
                expected,
                actual_at_least,
            } => {
                assert_eq!(expected, 1);
                assert_eq!(actual_at_least, 2);
            }
            _ => panic!("unexpected error: {err}"),
        }
    }

    #[test]
    fn facet_query_one_errors_on_no_rows() {
        let conn = Connection::open_in_memory().unwrap();
        conn.execute("CREATE TABLE ids (id INTEGER NOT NULL)", ())
            .unwrap();

        #[derive(Facet)]
        struct QueryId {
            id: i64,
        }

        #[derive(Debug, Facet, PartialEq)]
        struct IdRow {
            id: i64,
        }

        let mut stmt = conn.prepare("SELECT id FROM ids WHERE id = :id").unwrap();
        let err = stmt
            .facet_query_one::<IdRow, _>(QueryId { id: 1 })
            .unwrap_err();
        match err {
            Error::Sql(rusqlite::Error::QueryReturnedNoRows) => {}
            _ => panic!("unexpected error: {err}"),
        }
    }

    #[test]
    fn connection_ext_execute_and_query() {
        let conn = Connection::open_in_memory().unwrap();
        conn.execute(
            "CREATE TABLE users (id INTEGER NOT NULL, name TEXT NOT NULL)",
            (),
        )
        .unwrap();

        #[derive(Facet)]
        struct InsertUser {
            id: i64,
            name: String,
        }

        #[derive(Facet)]
        struct QueryUser {
            id: i64,
        }

        #[derive(Debug, Facet, PartialEq)]
        struct UserRow {
            id: i64,
            name: String,
        }

        conn.facet_execute(
            "INSERT INTO users (id, name) VALUES (:id, :name)",
            InsertUser {
                id: 11,
                name: "alice".to_string(),
            },
        )
        .unwrap();

        let row = conn
            .facet_query_one::<UserRow, _>(
                "SELECT id, name FROM users WHERE id = :id",
                QueryUser { id: 11 },
            )
            .unwrap();
        assert_eq!(
            row,
            UserRow {
                id: 11,
                name: "alice".to_string()
            }
        );
    }

    #[test]
    fn connection_ext_query_ref_accepts_slice() {
        let conn = Connection::open_in_memory().unwrap();
        conn.execute("CREATE TABLE ids (id INTEGER NOT NULL)", ())
            .unwrap();
        conn.execute("INSERT INTO ids (id) VALUES (3)", ()).unwrap();

        #[derive(Debug, Facet, PartialEq)]
        struct IdRow {
            id: i64,
        }

        let values = [3_i64];
        let rows = conn
            .facet_query_ref::<IdRow, [i64]>("SELECT id FROM ids WHERE id = ?1", &values[..])
            .unwrap();
        assert_eq!(rows, vec![IdRow { id: 3 }]);
    }

    #[test]
    fn connection_ext_errors_include_sql_context() {
        let conn = Connection::open_in_memory().unwrap();
        conn.execute("CREATE TABLE ids (id INTEGER NOT NULL)", ())
            .unwrap();

        #[derive(Facet)]
        struct QueryId {
            id: i64,
        }

        #[derive(Debug, Facet, PartialEq)]
        struct IdRow {
            id: i64,
        }

        let sql = "SELECT id FROM ids WHERE id = :id";
        let err = conn
            .facet_query_one::<IdRow, _>(sql, QueryId { id: 1 })
            .unwrap_err();
        match err {
            Error::WithSqlContext {
                sql: actual,
                source,
            } => {
                assert_eq!(actual, sql.to_string());
                match *source {
                    Error::Sql(rusqlite::Error::QueryReturnedNoRows) => {}
                    _ => panic!("unexpected nested source"),
                }
            }
            _ => panic!("expected SQL context wrapper"),
        }
    }
}
