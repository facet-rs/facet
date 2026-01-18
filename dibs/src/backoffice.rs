//! Squel service implementation - the data plane.
//!
//! Provides generic CRUD operations for any registered table.

use crate::query::{Db, Expr, SortDir, Value as QueryValue};
use crate::schema::Schema;
use dibs_proto::{
    CreateRequest, DeleteRequest, DibsError, Filter, FilterOp, GetRequest, ListRequest,
    ListResponse, Row, RowField, SchemaInfo, SortDir as ProtoSortDir, SquelService, UpdateRequest,
    Value as ProtoValue,
};

/// Default implementation of SquelService.
#[derive(Clone)]
pub struct SquelServiceImpl;

impl SquelServiceImpl {
    pub fn new() -> Self {
        Self
    }
}

impl Default for SquelServiceImpl {
    fn default() -> Self {
        Self::new()
    }
}

// =============================================================================
// Type conversions
// =============================================================================

fn proto_value_to_query(v: &ProtoValue) -> QueryValue {
    match v {
        ProtoValue::Null => QueryValue::Null,
        ProtoValue::Bool(b) => QueryValue::Bool(*b),
        ProtoValue::I16(n) => QueryValue::I16(*n),
        ProtoValue::I32(n) => QueryValue::I32(*n),
        ProtoValue::I64(n) => QueryValue::I64(*n),
        ProtoValue::F32(n) => QueryValue::F32(*n),
        ProtoValue::F64(n) => QueryValue::F64(*n),
        ProtoValue::String(s) => QueryValue::String(s.clone()),
        ProtoValue::Bytes(b) => QueryValue::Bytes(b.clone()),
    }
}

fn query_value_to_proto(v: &QueryValue) -> ProtoValue {
    match v {
        QueryValue::Null => ProtoValue::Null,
        QueryValue::Bool(b) => ProtoValue::Bool(*b),
        QueryValue::I16(n) => ProtoValue::I16(*n),
        QueryValue::I32(n) => ProtoValue::I32(*n),
        QueryValue::I64(n) => ProtoValue::I64(*n),
        QueryValue::F32(n) => ProtoValue::F32(*n),
        QueryValue::F64(n) => ProtoValue::F64(*n),
        QueryValue::String(s) => ProtoValue::String(s.clone()),
        QueryValue::Bytes(b) => ProtoValue::Bytes(b.clone()),
    }
}

fn query_row_to_proto(row: crate::query::Row) -> Row {
    Row {
        fields: row
            .into_iter()
            .map(|(name, value)| RowField {
                name,
                value: query_value_to_proto(&value),
            })
            .collect(),
    }
}

fn proto_row_to_query(row: &Row) -> Vec<(String, QueryValue)> {
    row.fields
        .iter()
        .map(|f| (f.name.clone(), proto_value_to_query(&f.value)))
        .collect()
}

fn filter_to_expr(filter: &Filter) -> Expr {
    let col = filter.field.clone();
    let val = proto_value_to_query(&filter.value);

    match filter.op {
        FilterOp::Eq => Expr::Eq(col, val),
        FilterOp::Ne => Expr::Ne(col, val),
        FilterOp::Lt => Expr::Lt(col, val),
        FilterOp::Lte => Expr::Lte(col, val),
        FilterOp::Gt => Expr::Gt(col, val),
        FilterOp::Gte => Expr::Gte(col, val),
        FilterOp::Like => {
            if let QueryValue::String(s) = val {
                Expr::Like(col, s)
            } else {
                Expr::Like(col, String::new())
            }
        }
        FilterOp::ILike => {
            if let QueryValue::String(s) = val {
                Expr::ILike(col, s)
            } else {
                Expr::ILike(col, String::new())
            }
        }
        FilterOp::IsNull => Expr::IsNull(col),
        FilterOp::IsNotNull => Expr::IsNotNull(col),
    }
}

fn proto_sort_to_query(dir: ProtoSortDir) -> SortDir {
    match dir {
        ProtoSortDir::Asc => SortDir::Asc,
        ProtoSortDir::Desc => SortDir::Desc,
    }
}

fn schema_to_info(schema: &Schema) -> SchemaInfo {
    use dibs_proto::{ColumnInfo, ForeignKeyInfo, IndexInfo, TableInfo};

    SchemaInfo {
        tables: schema
            .tables
            .iter()
            .map(|t| TableInfo {
                name: t.name.clone(),
                columns: t
                    .columns
                    .iter()
                    .map(|c| ColumnInfo {
                        name: c.name.clone(),
                        sql_type: c.pg_type.to_string(),
                        nullable: c.nullable,
                        default: c.default.clone(),
                        primary_key: c.primary_key,
                        unique: c.unique,
                    })
                    .collect(),
                foreign_keys: t
                    .foreign_keys
                    .iter()
                    .map(|fk| ForeignKeyInfo {
                        columns: fk.columns.clone(),
                        references_table: fk.references_table.clone(),
                        references_columns: fk.references_columns.clone(),
                    })
                    .collect(),
                indices: t
                    .indices
                    .iter()
                    .map(|idx| IndexInfo {
                        name: idx.name.clone(),
                        columns: idx.columns.clone(),
                        unique: idx.unique,
                    })
                    .collect(),
                source_file: t.source.file.clone(),
                source_line: t.source.line,
                doc: t.doc.clone(),
            })
            .collect(),
    }
}

// =============================================================================
// Service implementation
// =============================================================================

impl SquelService for SquelServiceImpl {
    async fn schema(&self) -> SchemaInfo {
        let schema = Schema::collect();
        schema_to_info(&schema)
    }

    async fn list(&self, request: ListRequest) -> Result<ListResponse, DibsError> {
        // Connect to database
        let (client, connection) =
            tokio_postgres::connect(&request.database_url, tokio_postgres::NoTls)
                .await
                .map_err(|e| DibsError::ConnectionFailed(e.to_string()))?;

        tokio::spawn(async move {
            if let Err(e) = connection.await {
                eprintln!("Database connection error: {}", e);
            }
        });

        let db = Db::new(&client);

        // Build the query
        let mut builder = db
            .select(&request.table)
            .map_err(|e| DibsError::UnknownTable(e.to_string()))?;

        // Apply filters
        for filter in &request.filters {
            builder = builder.filter(filter_to_expr(filter));
        }

        // Apply sorting
        for sort in &request.sort {
            builder = builder.order_by(&sort.field, proto_sort_to_query(sort.dir));
        }

        // Apply pagination
        if let Some(limit) = request.limit {
            builder = builder.limit(limit);
        }
        if let Some(offset) = request.offset {
            builder = builder.offset(offset);
        }

        // Execute
        let rows = builder
            .all()
            .await
            .map_err(|e| DibsError::QueryError(e.to_string()))?;

        Ok(ListResponse {
            rows: rows.into_iter().map(query_row_to_proto).collect(),
            total: None, // TODO: implement count
        })
    }

    async fn get(&self, request: GetRequest) -> Result<Option<Row>, DibsError> {
        // Connect to database
        let (client, connection) =
            tokio_postgres::connect(&request.database_url, tokio_postgres::NoTls)
                .await
                .map_err(|e| DibsError::ConnectionFailed(e.to_string()))?;

        tokio::spawn(async move {
            if let Err(e) = connection.await {
                eprintln!("Database connection error: {}", e);
            }
        });

        let db = Db::new(&client);

        // Find the primary key column
        let table = db
            .table(&request.table)
            .ok_or_else(|| DibsError::UnknownTable(request.table.clone()))?;

        let pk_col = table
            .columns
            .iter()
            .find(|c| c.primary_key)
            .ok_or_else(|| {
                DibsError::InvalidRequest(format!("Table {} has no primary key", request.table))
            })?;

        // Query by primary key
        let row = db
            .select(&request.table)
            .map_err(|e| DibsError::UnknownTable(e.to_string()))?
            .filter(Expr::Eq(
                pk_col.name.clone(),
                proto_value_to_query(&request.pk),
            ))
            .one()
            .await
            .map_err(|e| DibsError::QueryError(e.to_string()))?;

        Ok(row.map(query_row_to_proto))
    }

    async fn create(&self, request: CreateRequest) -> Result<Row, DibsError> {
        // Connect to database
        let (client, connection) =
            tokio_postgres::connect(&request.database_url, tokio_postgres::NoTls)
                .await
                .map_err(|e| DibsError::ConnectionFailed(e.to_string()))?;

        tokio::spawn(async move {
            if let Err(e) = connection.await {
                eprintln!("Database connection error: {}", e);
            }
        });

        let db = Db::new(&client);

        let data = proto_row_to_query(&request.data);

        let row = db
            .insert(&request.table)
            .map_err(|e| DibsError::UnknownTable(e.to_string()))?
            .values(data)
            .returning()
            .await
            .map_err(|e| DibsError::QueryError(e.to_string()))?
            .ok_or_else(|| DibsError::QueryError("Insert did not return a row".to_string()))?;

        Ok(query_row_to_proto(row))
    }

    async fn update(&self, request: UpdateRequest) -> Result<Row, DibsError> {
        // Connect to database
        let (client, connection) =
            tokio_postgres::connect(&request.database_url, tokio_postgres::NoTls)
                .await
                .map_err(|e| DibsError::ConnectionFailed(e.to_string()))?;

        tokio::spawn(async move {
            if let Err(e) = connection.await {
                eprintln!("Database connection error: {}", e);
            }
        });

        let db = Db::new(&client);

        // Find the primary key column
        let table = db
            .table(&request.table)
            .ok_or_else(|| DibsError::UnknownTable(request.table.clone()))?;

        let pk_col = table
            .columns
            .iter()
            .find(|c| c.primary_key)
            .ok_or_else(|| {
                DibsError::InvalidRequest(format!("Table {} has no primary key", request.table))
            })?;

        let data = proto_row_to_query(&request.data);

        let row = db
            .update(&request.table)
            .map_err(|e| DibsError::UnknownTable(e.to_string()))?
            .set(data)
            .filter(Expr::Eq(
                pk_col.name.clone(),
                proto_value_to_query(&request.pk),
            ))
            .returning()
            .await
            .map_err(|e| DibsError::QueryError(e.to_string()))?
            .ok_or_else(|| DibsError::QueryError("Update did not return a row".to_string()))?;

        Ok(query_row_to_proto(row))
    }

    async fn delete(&self, request: DeleteRequest) -> Result<u64, DibsError> {
        // Connect to database
        let (client, connection) =
            tokio_postgres::connect(&request.database_url, tokio_postgres::NoTls)
                .await
                .map_err(|e| DibsError::ConnectionFailed(e.to_string()))?;

        tokio::spawn(async move {
            if let Err(e) = connection.await {
                eprintln!("Database connection error: {}", e);
            }
        });

        let db = Db::new(&client);

        // Find the primary key column
        let table = db
            .table(&request.table)
            .ok_or_else(|| DibsError::UnknownTable(request.table.clone()))?;

        let pk_col = table
            .columns
            .iter()
            .find(|c| c.primary_key)
            .ok_or_else(|| {
                DibsError::InvalidRequest(format!("Table {} has no primary key", request.table))
            })?;

        let affected = db
            .delete(&request.table)
            .map_err(|e| DibsError::UnknownTable(e.to_string()))?
            .filter(Expr::Eq(
                pk_col.name.clone(),
                proto_value_to_query(&request.pk),
            ))
            .execute()
            .await
            .map_err(|e| DibsError::QueryError(e.to_string()))?;

        Ok(affected)
    }
}
