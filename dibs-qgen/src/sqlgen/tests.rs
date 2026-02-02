// use super::*;
// use crate::parse::parse_query_file;
// use dibs_query_schema::Decl;

// /// Helper to extract first Query from QueryFile
// fn get_first_query(file: &dibs_query_schema::QueryFile) -> &dibs_query_schema::Query {
//     file.0
//         .values()
//         .find_map(|decl| match decl {
//             Decl::Query(q) => Some(q),
//             _ => None,
//         })
//         .expect("no query found")
// }

// /// Helper to extract first Insert from QueryFile
// fn get_first_insert(file: &dibs_query_schema::QueryFile) -> &dibs_query_schema::Insert {
//     file.0
//         .values()
//         .find_map(|decl| match decl {
//             Decl::Insert(i) => Some(i),
//             _ => None,
//         })
//         .expect("no insert found")
// }

// /// Helper to extract first Upsert from QueryFile
// fn get_first_upsert(file: &dibs_query_schema::QueryFile) -> &dibs_query_schema::Upsert {
//     file.0
//         .values()
//         .find_map(|decl| match decl {
//             Decl::Upsert(u) => Some(u),
//             _ => None,
//         })
//         .expect("no upsert found")
// }

// /// Helper to extract first Update from QueryFile
// fn get_first_update(file: &dibs_query_schema::QueryFile) -> &dibs_query_schema::Update {
//     file.0
//         .values()
//         .find_map(|decl| match decl {
//             Decl::Update(u) => Some(u),
//             _ => None,
//         })
//         .expect("no update found")
// }

// /// Helper to extract first Delete from QueryFile
// fn get_first_delete(file: &dibs_query_schema::QueryFile) -> &dibs_query_schema::Delete {
//     file.0
//         .values()
//         .find_map(|decl| match decl {
//             Decl::Delete(d) => Some(d),
//             _ => None,
//         })
//         .expect("no delete found")
// }

// /// Helper to extract first InsertMany from QueryFile
// fn get_first_insert_many(file: &dibs_query_schema::QueryFile) -> &dibs_query_schema::InsertMany {
//     file.0
//         .values()
//         .find_map(|decl| match decl {
//             Decl::InsertMany(im) => Some(im),
//             _ => None,
//         })
//         .expect("no insert-many found")
// }

// /// Helper to extract first UpsertMany from QueryFile
// fn get_first_upsert_many(file: &dibs_query_schema::QueryFile) -> &dibs_query_schema::UpsertMany {
//     file.0
//         .values()
//         .find_map(|decl| match decl {
//             Decl::UpsertMany(um) => Some(um),
//             _ => None,
//         })
//         .expect("no upsert-many found")
// }

// #[test]
// fn test_simple_select() {
//     let source = r#"
// AllProducts @select{
//   from product
//   select { id, handle, status }
// }
// "#;
//     let file = parse_query_file("<test>", source).unwrap();
//     let sql = generate_simple_sql(get_first_query(&file));

//     // TODO: These tests are absolute trash because we use index map, we don't use hash map and also
//     // they're just like checking we contain thing. What we should do in fact is just have snapshot
//     // testing honestly.
//     //
//     // Column order is non-deterministic due to HashMap iteration
//     assert!(sql.sql.starts_with("SELECT "));
//     assert!(sql.sql.contains(r#""id""#));
//     assert!(sql.sql.contains(r#""handle""#));
//     assert!(sql.sql.contains(r#""status""#));
//     assert!(sql.sql.ends_with(r#" FROM "product""#));
//     assert!(sql.param_order.is_empty());
// }

// #[test]
// fn test_select_with_where() {
//     let source = r#"
// ActiveProducts @select{
//   from product
//   where { status "published", active true }
//   select { id, handle }
// }
// "#;
//     let file = parse_query_file("<test>", source).unwrap();
//     let sql = generate_simple_sql(get_first_query(&file));

//     // Check structure without depending on order
//     assert!(sql.sql.contains("SELECT "));
//     assert!(sql.sql.contains(r#""id""#));
//     assert!(sql.sql.contains(r#""handle""#));
//     assert!(sql.sql.contains("WHERE"));
//     assert!(sql.sql.contains(r#""status" = 'published'"#));
//     assert!(sql.sql.contains(r#""active" = true"#));
//     assert!(sql.param_order.is_empty()); // No params for literals
// }

// #[test]
// fn test_select_with_params() {
//     let source = r#"
// ProductByHandle @select{
//   params { handle @string }
//   from product
//   where { handle $handle }
//   select { id, handle }
// }
// "#;
//     let file = parse_query_file("<test>", source).unwrap();
//     let sql = generate_simple_sql(get_first_query(&file));

//     assert!(sql.sql.contains(r#""handle" = $1"#));
//     assert_eq!(sql.param_order, vec!["handle"]);
// }

// #[test]
// fn test_select_with_order_and_limit() {
//     let source = r#"
// RecentProducts @select{
//   from product
//   order-by {created_at desc}
//   limit 20
//   select {id, handle}
// }
// "#;
//     let file = parse_query_file("<test>", source).unwrap();
//     let sql = generate_simple_sql(get_first_query(&file));

//     assert!(sql.sql.contains(r#"ORDER BY "created_at" DESC"#));
//     assert!(sql.sql.contains("LIMIT 20"));
// }

// #[test]
// fn test_null_filter() {
//     let source = r#"
// ActiveProducts @select{
//   from product
//   where { deleted_at @null }
//   select { id }
// }
// "#;
//     let file = parse_query_file("<test>", source).unwrap();
//     let sql = generate_simple_sql(get_first_query(&file));

//     assert!(sql.sql.contains(r#""deleted_at" IS NULL"#));
// }

// #[test]
// fn test_ilike_filter() {
//     let source = r#"
// SearchProducts @select{
//   params { q @string }
//   from product
//   where { handle @ilike($q) }
//   select { id, handle }
// }
// "#;
//     let file = parse_query_file("<test>", source).unwrap();
//     let sql = generate_simple_sql(get_first_query(&file));

//     assert!(sql.sql.contains(r#""handle" ILIKE $1"#));
//     assert_eq!(sql.param_order, vec!["q"]);
// }

// #[test]
// fn test_not_null_filter() {
//     let source = r#"
// PublishedProducts @select{
//   from product
//   where { published_at @not-null }
//   select { id }
// }
// "#;
//     let file = parse_query_file("<test>", source).unwrap();
//     let sql = generate_simple_sql(get_first_query(&file));

//     assert!(
//         sql.sql.contains(r#""published_at" IS NOT NULL"#),
//         "SQL: {}",
//         sql.sql
//     );
// }

// #[test]
// fn test_gte_filter() {
//     let source = r#"
// FilteredProducts @select{
//   params { min_price @int }
//   from product
//   where { price @gte($min_price) }
//   select { id, price }
// }
// "#;
//     let file = parse_query_file("<test>", source).unwrap();
//     let sql = generate_simple_sql(get_first_query(&file));

//     assert!(sql.sql.contains(r#""price" >= $1"#), "SQL: {}", sql.sql);
//     assert_eq!(sql.param_order, vec!["min_price"]);
// }

// #[test]
// fn test_lte_filter() {
//     let source = r#"
// FilteredProducts @select{
//   params { max_price @int }
//   from product
//   where { price @lte($max_price) }
//   select { id, price }
// }
// "#;
//     let file = parse_query_file("<test>", source).unwrap();
//     let sql = generate_simple_sql(get_first_query(&file));

//     assert!(sql.sql.contains(r#""price" <= $1"#), "SQL: {}", sql.sql);
//     assert_eq!(sql.param_order, vec!["max_price"]);
// }

// #[test]
// fn test_ne_filter() {
//     let source = r#"
// NonDraftProducts @select{
//   params { excluded_status @string }
//   from product
//   where { status @ne($excluded_status) }
//   select { id, status }
// }
// "#;
//     let file = parse_query_file("<test>", source).unwrap();
//     let sql = generate_simple_sql(get_first_query(&file));

//     assert!(sql.sql.contains(r#""status" != $1"#), "SQL: {}", sql.sql);
//     assert_eq!(sql.param_order, vec!["excluded_status"]);
// }

// #[test]
// fn test_in_filter() {
//     let source = r#"
// ProductsByStatus @select{
//   params { statuses @string }
//   from product
//   where { status @in($statuses) }
//   select { id, status }
// }
// "#;
//     let file = parse_query_file("<test>", source).unwrap();
//     let sql = generate_simple_sql(get_first_query(&file));

//     assert!(
//         sql.sql.contains(r#""status" = ANY($1)"#),
//         "SQL: {}",
//         sql.sql
//     );
//     assert_eq!(sql.param_order, vec!["statuses"]);
// }

// #[test]
// fn test_json_get_operator() {
//     let source = r#"
// ProductWithMetadata @select{
//   params { key @string }
//   from product
//   where { metadata @json-get($key) }
//   select { id, metadata }
// }
// "#;
//     let file = parse_query_file("<test>", source).unwrap();
//     let sql = generate_simple_sql(get_first_query(&file));

//     assert!(sql.sql.contains(r#""metadata" -> $1"#), "SQL: {}", sql.sql);
//     assert_eq!(sql.param_order, vec!["key"]);
// }

// #[test]
// fn test_json_get_operator_literal() {
//     let source = r#"
// ProductWithSettings @select{
//   from product
//   where { metadata @json-get("settings") }
//   select { id, metadata }
// }
// "#;
//     let file = parse_query_file("<test>", source).unwrap();
//     let sql = generate_simple_sql(get_first_query(&file));

//     assert!(
//         sql.sql.contains(r#""metadata" -> 'settings'"#),
//         "SQL: {}",
//         sql.sql
//     );
//     assert!(sql.param_order.is_empty());
// }

// #[test]
// fn test_json_get_text_operator() {
//     let source = r#"
// ProductWithJsonValue @select{
//   params { key @string }
//   from product
//   where { metadata @json-get-text($key) }
//   select { id, metadata }
// }
// "#;
//     let file = parse_query_file("<test>", source).unwrap();
//     let sql = generate_simple_sql(get_first_query(&file));

//     assert!(sql.sql.contains(r#""metadata" ->> $1"#), "SQL: {}", sql.sql);
//     assert_eq!(sql.param_order, vec!["key"]);
// }

// #[test]
// fn test_json_contains_operator() {
//     let source = r#"
// ProductWithMetadataContains @select{
//   params { json_value @string }
//   from product
//   where { metadata @contains($json_value) }
//   select { id, metadata }
// }
// "#;
//     let file = parse_query_file("<test>", source).unwrap();
//     let sql = generate_simple_sql(get_first_query(&file));

//     assert!(sql.sql.contains(r#""metadata" @> $1"#), "SQL: {}", sql.sql);
//     assert_eq!(sql.param_order, vec!["json_value"]);
// }

// #[test]
// fn test_json_key_exists_operator() {
//     let source = r#"
// ProductWithMetadataKey @select{
//   params { key @string }
//   from product
//   where { metadata @key-exists($key) }
//   select { id, metadata }
// }
// "#;
//     let file = parse_query_file("<test>", source).unwrap();
//     let sql = generate_simple_sql(get_first_query(&file));

//     assert!(sql.sql.contains(r#""metadata" ? $1"#), "SQL: {}", sql.sql);
//     assert_eq!(sql.param_order, vec!["key"]);
// }

// #[test]
// fn test_json_key_exists_operator_literal() {
//     let source = r#"
// ProductWithLocale @select{
//   from product
//   where { metadata @key-exists("locale") }
//   select { id, metadata }
// }
// "#;
//     let file = parse_query_file("<test>", source).unwrap();
//     let sql = generate_simple_sql(get_first_query(&file));

//     assert!(
//         sql.sql.contains(r#""metadata" ? 'locale'"#),
//         "SQL: {}",
//         sql.sql
//     );
//     assert!(sql.param_order.is_empty());
// }

// #[test]
// fn test_pagination_literals() {
//     let source = r#"
// PaginatedProducts @select{
//   from product
//   order_by { created_at desc }
//   limit 20
//   offset 40
//   select { id, handle }
// }
// "#;
//     let file = parse_query_file("<test>", source).unwrap();
//     let sql = generate_simple_sql(get_first_query(&file));

//     assert!(sql.sql.contains("LIMIT 20"), "SQL: {}", sql.sql);
//     assert!(sql.sql.contains("OFFSET 40"), "SQL: {}", sql.sql);
//     assert!(sql.param_order.is_empty());
// }

// #[test]
// fn test_distinct() {
//     let source = r#"
// UniqueStatuses @select{
//   from product
//   distinct true
//   select { status }
// }
// "#;
//     let file = parse_query_file("<test>", source).unwrap();
//     let sql = generate_simple_sql(get_first_query(&file));

//     assert!(sql.sql.contains("SELECT DISTINCT"), "SQL: {}", sql.sql);
//     assert!(sql.sql.contains("\"status\""), "SQL: {}", sql.sql);
// }

// #[test]
// fn test_distinct_on() {
//     let source = r#"
// LatestPerCategory @select{
//   from product
//   distinct-on (category_id)
//   order-by {category_id asc, created_at desc}
//   select {id, category_id, handle}
// }
// "#;
//     let file = parse_query_file("<test>", source).unwrap();
//     let query = get_first_query(&file);
//     eprintln!("distinct_on: {:?}", query.distinct_on);
//     eprintln!("order_by: {:?}", query.order_by);
//     let sql = generate_simple_sql(query);
//     eprintln!("Generated SQL: {}", sql.sql);

//     assert!(
//         sql.sql.contains("SELECT DISTINCT ON (\"category_id\")"),
//         "SQL: {}",
//         sql.sql
//     );
//     assert!(
//         sql.sql
//             .contains("ORDER BY \"category_id\" ASC, \"created_at\" DESC"),
//         "SQL: {}",
//         sql.sql
//     );
// }

// #[test]
// fn test_distinct_on_multiple_columns() {
//     let source = r#"
// DistinctProducts @select{
//   from product
//   distinct-on (brand category)
//   order-by {brand asc, category asc, created_at desc}
//   select {id, brand, category, handle}
// }
// "#;
//     let file = parse_query_file("<test>", source).unwrap();
//     let query = get_first_query(&file);
//     eprintln!("distinct_on: {:?}", query.distinct_on);
//     eprintln!("order_by: {:?}", query.order_by);
//     let sql = generate_simple_sql(query);
//     eprintln!("Generated SQL: {}", sql.sql);

//     assert!(
//         sql.sql
//             .contains("SELECT DISTINCT ON (\"brand\", \"category\")"),
//         "SQL: {}",
//         sql.sql
//     );
// }

// #[test]
// fn test_pagination_params() {
//     let source = r#"
// PaginatedProducts @select{
//   params { page_size @int, page_offset @int }
//   from product
//   order_by { created_at desc }
//   limit $page_size
//   offset $page_offset
//   select { id, handle }
// }
// "#;
//     let file = parse_query_file("<test>", source).unwrap();
//     let sql = generate_simple_sql(get_first_query(&file));

//     assert!(sql.sql.contains("LIMIT $1"));
//     assert!(sql.sql.contains("OFFSET $2"));
//     assert_eq!(sql.param_order, vec!["page_size", "page_offset"]);
// }

// #[test]
// fn test_sql_with_joins() {
//     use crate::planner::{ForeignKey, Table};, Schema

//     let source = r#"
// ProductWithTranslation @select{
//   params { handle @string }
//   from product
//   where { handle $handle }
//   select {
//     id
//     handle
//     translation @rel{
//       from product_translation
//       first true
//       select { title, description }
//     }
//   }
// }
// "#;
//     let file = parse_query_file("<test>", source).unwrap();

//     // Build test schema
//     let mut schema = Schema::default();
//     schema.tables.insert(
//         "product".to_string(),
//         Table {
//             name: "product".to_string(),
//             columns: vec!["id".to_string(), "handle".to_string()],
//             foreign_keys: vec![],
//         },
//     );
//     schema.tables.insert(
//         "product_translation".to_string(),
//         Table {
//             name: "product_translation".to_string(),
//             columns: vec![
//                 "id".to_string(),
//                 "product_id".to_string(),
//                 "title".to_string(),
//                 "description".to_string(),
//             ],
//             foreign_keys: vec![ForeignKey {
//                 columns: vec!["product_id".to_string()],
//                 references_table: "product".to_string(),
//                 references_columns: vec!["id".to_string()],
//             }],
//         },
//     );

//     let sql = generate_sql(get_first_query(&file), Some(&schema)).unwrap();

//     // Check SELECT
//     assert!(sql.sql.contains("\"t0\".\"id\""));
//     assert!(sql.sql.contains("\"t0\".\"handle\""));
//     assert!(sql.sql.contains("\"t1\".\"title\""));
//     assert!(sql.sql.contains("\"t1\".\"description\""));

//     // Check FROM with JOIN
//     assert!(sql.sql.contains("FROM \"product\" AS \"t0\""));
//     assert!(
//         sql.sql
//             .contains("LEFT JOIN \"product_translation\" AS \"t1\"")
//     );
//     assert!(sql.sql.contains("ON t0.id = t1.product_id"));

//     // Check WHERE
//     assert!(sql.sql.contains("\"t0\".\"handle\" = $1"));

//     // Check param order
//     assert_eq!(sql.param_order, vec!["handle"]);

//     // Check plan exists
//     assert!(sql.plan.is_some());
// }

// #[test]
// fn test_sql_with_relation_where_literal() {
//     use crate::planner::{ForeignKey, Table};, Schema

//     let source = r#"
// ProductWithEnglishTranslation @select{
//   from product
//   select {
//     id
//     translation @rel{
//       from product_translation
//       where { locale en }
//       first true
//       select { title }
//     }
//   }
// }
// "#;
//     let file = parse_query_file("<test>", source).unwrap();

//     let mut schema = Schema::default();
//     schema.tables.insert(
//         "product".to_string(),
//         Table {
//             name: "product".to_string(),
//             columns: vec!["id".to_string()],
//             foreign_keys: vec![],
//         },
//     );
//     schema.tables.insert(
//         "product_translation".to_string(),
//         Table {
//             name: "product_translation".to_string(),
//             columns: vec![
//                 "id".to_string(),
//                 "product_id".to_string(),
//                 "locale".to_string(),
//                 "title".to_string(),
//             ],
//             foreign_keys: vec![ForeignKey {
//                 columns: vec!["product_id".to_string()],
//                 references_table: "product".to_string(),
//                 references_columns: vec!["id".to_string()],
//             }],
//         },
//     );

//     let sql = generate_sql(get_first_query(&file), Some(&schema)).unwrap();

//     // Check that relation filter is in the ON clause
//     assert!(
//         sql.sql
//             .contains("ON t0.id = t1.product_id AND \"t1\".\"locale\" = 'en'"),
//         "Expected relation filter in ON clause, got: {}",
//         sql.sql
//     );
// }

// #[test]
// fn test_sql_with_relation_where_param() {
//     use crate::planner::{ForeignKey, Table};, Schema

//     let source = r#"
// ProductWithTranslation @select{
//   params { locale @string }
//   from product
//   select {
//     id
//     translation @rel{
//       from product_translation
//       where { locale $locale }
//       first true
//       select { title }
//     }
//   }
// }
// "#;
//     let file = parse_query_file("<test>", source).unwrap();

//     let mut schema = Schema::default();
//     schema.tables.insert(
//         "product".to_string(),
//         Table {
//             name: "product".to_string(),
//             columns: vec!["id".to_string()],
//             foreign_keys: vec![],
//         },
//     );
//     schema.tables.insert(
//         "product_translation".to_string(),
//         Table {
//             name: "product_translation".to_string(),
//             columns: vec![
//                 "id".to_string(),
//                 "product_id".to_string(),
//                 "locale".to_string(),
//                 "title".to_string(),
//             ],
//             foreign_keys: vec![ForeignKey {
//                 columns: vec!["product_id".to_string()],
//                 references_table: "product".to_string(),
//                 references_columns: vec!["id".to_string()],
//             }],
//         },
//     );

//     let sql = generate_sql(get_first_query(&file), Some(&schema)).unwrap();

//     // Check that relation filter is in the ON clause with param placeholder
//     assert!(
//         sql.sql
//             .contains("ON t0.id = t1.product_id AND \"t1\".\"locale\" = $1"),
//         "Expected relation filter with param in ON clause, got: {}",
//         sql.sql
//     );

//     // Check param order includes the relation param
//     assert_eq!(sql.param_order, vec!["locale"]);
// }

// #[test]
// fn test_sql_with_relation_where_and_base_where() {
//     use crate::planner::{ForeignKey, Table};, Schema

//     let source = r#"
// ProductWithTranslation @select{
//   params { handle @string, locale @string }
//   from product
//   where { handle $handle }
//   select {
//     id
//     translation @rel{
//       from product_translation
//       where { locale $locale }
//       first true
//       select { title }
//     }
//   }
// }
// "#;
//     let file = parse_query_file("<test>", source).unwrap();

//     let mut schema = Schema::default();
//     schema.tables.insert(
//         "product".to_string(),
//         Table {
//             name: "product".to_string(),
//             columns: vec!["id".to_string(), "handle".to_string()],
//             foreign_keys: vec![],
//         },
//     );
//     schema.tables.insert(
//         "product_translation".to_string(),
//         Table {
//             name: "product_translation".to_string(),
//             columns: vec![
//                 "id".to_string(),
//                 "product_id".to_string(),
//                 "locale".to_string(),
//                 "title".to_string(),
//             ],
//             foreign_keys: vec![ForeignKey {
//                 columns: vec!["product_id".to_string()],
//                 references_table: "product".to_string(),
//                 references_columns: vec!["id".to_string()],
//             }],
//         },
//     );

//     let sql = generate_sql(get_first_query(&file), Some(&schema)).unwrap();

//     // Relation filter should be $1 (comes first in FROM clause)
//     assert!(
//         sql.sql.contains("\"t1\".\"locale\" = $1"),
//         "Expected relation filter as $1, got: {}",
//         sql.sql
//     );

//     // Base WHERE filter should be $2 (comes after FROM clause)
//     assert!(
//         sql.sql.contains("\"t0\".\"handle\" = $2"),
//         "Expected base filter as $2, got: {}",
//         sql.sql
//     );

//     // Check param order: relation params first, then base WHERE params
//     assert_eq!(sql.param_order, vec!["locale", "handle"]);
// }

// #[test]
// fn test_insert_sql() {
//     let source = r#"
// CreateUser @insert{
//   params {
//     name @string
//     email @string
//   }
//   into users
//   values {
//     name $name
//     email $email
//     created_at @now
//   }
//   returning { id, name, email, created_at }
// }
// "#;
//     let file = parse_query_file("<test>", source).unwrap();
//     let sql = generate_insert_sql(get_first_insert(&file));

//     assert!(sql.sql.contains("INSERT INTO \"users\""));
//     assert!(sql.sql.contains("\"name\""));
//     assert!(sql.sql.contains("\"email\""));
//     assert!(sql.sql.contains("\"created_at\""));
//     assert!(sql.sql.contains("NOW()"));
//     assert!(sql.sql.contains("RETURNING"));
//     assert_eq!(sql.param_order.len(), 2);
// }

// #[test]
// fn test_upsert_sql() {
//     let source = r#"
// UpsertProduct @upsert{
//   params {
//     id @uuid
//     name @string
//     price @decimal
//   }
//   into products
//   on-conflict {
//     target { id }
//     update { name, price, updated_at @now }
//   }
//   values {
//     id $id
//     name $name
//     price $price
//   }
//   returning { id, name, price, updated_at }
// }
// "#;
//     let file = parse_query_file("<test>", source).unwrap();
//     let sql = generate_upsert_sql(get_first_upsert(&file));

//     assert!(sql.sql.contains("INSERT INTO \"products\""));
//     assert!(sql.sql.contains("ON CONFLICT (\"id\")"));
//     assert!(sql.sql.contains("DO UPDATE SET"));
//     // id should NOT be in the update set
//     assert!(!sql.sql.contains("\"id\" = $"));
//     assert!(sql.sql.contains("\"name\" ="));
//     assert!(sql.sql.contains("\"price\" ="));
//     assert!(sql.sql.contains("RETURNING"));
// }

// #[test]
// fn test_update_sql() {
//     let source = r#"
// UpdateUserEmail @update{
//   params {
//     id @uuid
//     email @string
//   }
//   table users
//   set {
//     email $email
//     updated_at @now
//   }
//   where { id $id }
//   returning { id, email, updated_at }
// }
// "#;
//     let file = parse_query_file("<test>", source).unwrap();
//     let sql = generate_update_sql(get_first_update(&file));

//     assert!(sql.sql.contains("UPDATE \"users\" SET"));
//     assert!(sql.sql.contains("\"email\" = $1"));
//     assert!(sql.sql.contains("\"updated_at\" = NOW()"));
//     assert!(sql.sql.contains("WHERE \"id\" = $2"));
//     assert!(sql.sql.contains("RETURNING"));
//     assert_eq!(sql.param_order, vec!["email", "id"]);
// }

// #[test]
// fn test_delete_sql() {
//     let source = r#"
// DeleteUser @delete{
//   params {
//     id @uuid
//   }
//   from users
//   where { id $id }
//   returning { id }
// }
// "#;
//     let file = parse_query_file("<test>", source).unwrap();
//     let sql = generate_delete_sql(get_first_delete(&file));

//     assert!(sql.sql.contains("DELETE FROM \"users\""));
//     assert!(sql.sql.contains("WHERE \"id\" = $1"));
//     assert!(sql.sql.contains("RETURNING \"id\""));
//     assert_eq!(sql.param_order, vec!["id"]);
// }

// #[test]
// fn test_relation_order_by_lateral() {
//     use crate::planner::{ForeignKey, Table};, Schema

//     let source = r#"
// ProductWithLatestTranslation @select{
//   from product
//   select {
//     id
//     translation @rel{
//       from product_translation
//       order-by {updated_at desc}
//       first true
//       select {title, description}
//     }
//   }
// }
// "#;
//     let file = parse_query_file("<test>", source).unwrap();

//     let mut schema = Schema::default();
//     schema.tables.insert(
//         "product".to_string(),
//         Table {
//             name: "product".to_string(),
//             columns: vec!["id".to_string()],
//             foreign_keys: vec![],
//         },
//     );
//     schema.tables.insert(
//         "product_translation".to_string(),
//         Table {
//             name: "product_translation".to_string(),
//             columns: vec![
//                 "id".to_string(),
//                 "product_id".to_string(),
//                 "title".to_string(),
//                 "description".to_string(),
//                 "updated_at".to_string(),
//             ],
//             foreign_keys: vec![ForeignKey {
//                 columns: vec!["product_id".to_string()],
//                 references_table: "product".to_string(),
//                 references_columns: vec!["id".to_string()],
//             }],
//         },
//     );

//     let sql = generate_sql(get_first_query(&file), Some(&schema)).unwrap();

//     // Should use LATERAL join for first:true with order_by
//     assert!(
//         sql.sql.contains("LEFT JOIN LATERAL"),
//         "Expected LATERAL join, got: {}",
//         sql.sql
//     );

//     // Should have ORDER BY in the subquery
//     assert!(
//         sql.sql.contains("ORDER BY \"updated_at\" DESC"),
//         "Expected ORDER BY in LATERAL subquery, got: {}",
//         sql.sql
//     );

//     // Should have LIMIT 1
//     assert!(
//         sql.sql.contains("LIMIT 1"),
//         "Expected LIMIT 1 in LATERAL subquery, got: {}",
//         sql.sql
//     );

//     // Should join ON true (LATERAL handles the correlation)
//     assert!(
//         sql.sql.contains("ON true"),
//         "Expected ON true for LATERAL join, got: {}",
//         sql.sql
//     );
// }

// #[test]
// fn test_relation_order_by_with_filter() {
//     use crate::planner::{ForeignKey, Table};, Schema

//     let source = r#"
// ProductWithLatestEnglishTranslation @select{
//   params {locale @string}
//   from product
//   select {
//     id
//     translation @rel{
//       from product_translation
//       where {locale $locale}
//       order-by {updated_at desc}
//       first true
//       select {title}
//     }
//   }
// }
// "#;
//     let file = parse_query_file("<test>", source).unwrap();

//     let mut schema = Schema::default();
//     schema.tables.insert(
//         "product".to_string(),
//         Table {
//             name: "product".to_string(),
//             columns: vec!["id".to_string()],
//             foreign_keys: vec![],
//         },
//     );
//     schema.tables.insert(
//         "product_translation".to_string(),
//         Table {
//             name: "product_translation".to_string(),
//             columns: vec![
//                 "id".to_string(),
//                 "product_id".to_string(),
//                 "locale".to_string(),
//                 "title".to_string(),
//                 "updated_at".to_string(),
//             ],
//             foreign_keys: vec![ForeignKey {
//                 columns: vec!["product_id".to_string()],
//                 references_table: "product".to_string(),
//                 references_columns: vec!["id".to_string()],
//             }],
//         },
//     );

//     let sql = generate_sql(get_first_query(&file), Some(&schema)).unwrap();

//     // Should use LATERAL
//     assert!(
//         sql.sql.contains("LEFT JOIN LATERAL"),
//         "Expected LATERAL join, got: {}",
//         sql.sql
//     );

//     // Should have locale filter in the subquery with $1
//     assert!(
//         sql.sql.contains("\"locale\" = $1"),
//         "Expected locale filter in LATERAL subquery, got: {}",
//         sql.sql
//     );

//     // Param should be tracked
//     assert_eq!(sql.param_order, vec!["locale"]);
// }

// #[test]
// fn test_insert_many_sql() {
//     let source = r#"
// BulkCreateProducts @insert-many{
//   params {
//     handle @string
//     status @string
//   }
//   into products
//   values {
//     handle $handle
//     status $status
//     created_at @now
//   }
//   returning { id, handle, status }
// }
// "#;
//     let file = parse_query_file("<test>", source).unwrap();
//     let sql = generate_insert_many_sql(get_first_insert_many(&file));

//     // Check INSERT INTO with correct columns
//     assert!(
//         sql.sql.contains("INSERT INTO \"products\""),
//         "SQL: {}",
//         sql.sql
//     );
//     assert!(sql.sql.contains("\"handle\""), "SQL: {}", sql.sql);
//     assert!(sql.sql.contains("\"status\""), "SQL: {}", sql.sql);
//     assert!(sql.sql.contains("\"created_at\""), "SQL: {}", sql.sql);

//     // Check SELECT FROM UNNEST pattern
//     assert!(sql.sql.contains("SELECT"), "SQL: {}", sql.sql);
//     assert!(sql.sql.contains("FROM UNNEST("), "SQL: {}", sql.sql);
//     assert!(sql.sql.contains("$1::text[]"), "SQL: {}", sql.sql);
//     assert!(sql.sql.contains("$2::text[]"), "SQL: {}", sql.sql);
//     assert!(sql.sql.contains("AS t(handle, status)"), "SQL: {}", sql.sql);

//     // Check NOW() function is in select
//     assert!(sql.sql.contains("NOW()"), "SQL: {}", sql.sql);

//     // Check RETURNING
//     assert!(sql.sql.contains("RETURNING"), "SQL: {}", sql.sql);

//     // Check params
//     assert_eq!(sql.param_order, vec!["handle", "status"]);
// }

// #[test]
// fn test_upsert_many_sql() {
//     let source = r#"
// BulkUpsertProducts @upsert-many{
//   params {
//     handle @string
//     status @string
//   }
//   into products
//   on-conflict {
//     target { handle }
//     update { status, updated_at @now }
//   }
//   values {
//     handle $handle
//     status $status
//     created_at @now
//   }
//   returning { id, handle, status }
// }
// "#;
//     let file = parse_query_file("<test>", source).unwrap();
//     let sql = generate_upsert_many_sql(get_first_upsert_many(&file));

//     // Check INSERT INTO
//     assert!(
//         sql.sql.contains("INSERT INTO \"products\""),
//         "SQL: {}",
//         sql.sql
//     );

//     // Check SELECT FROM UNNEST
//     assert!(sql.sql.contains("SELECT"), "SQL: {}", sql.sql);
//     assert!(sql.sql.contains("FROM UNNEST("), "SQL: {}", sql.sql);

//     // Check ON CONFLICT
//     assert!(
//         sql.sql.contains("ON CONFLICT (\"handle\")"),
//         "SQL: {}",
//         sql.sql
//     );
//     assert!(sql.sql.contains("DO UPDATE SET"), "SQL: {}", sql.sql);

//     // Check that handle is NOT in update (it's the conflict target)
//     let update_part = sql.sql.split("DO UPDATE SET").nth(1).unwrap();
//     assert!(
//         !update_part.contains("\"handle\" ="),
//         "handle should not be in UPDATE SET: {}",
//         sql.sql
//     );

//     // Check that status uses EXCLUDED
//     assert!(sql.sql.contains("EXCLUDED.\"status\""), "SQL: {}", sql.sql);

//     // Check that updated_at uses NOW()
//     assert!(
//         update_part.contains("NOW()"),
//         "updated_at should use NOW(): {}",
//         sql.sql
//     );

//     // Check RETURNING
//     assert!(sql.sql.contains("RETURNING"), "SQL: {}", sql.sql);

//     // Check params
//     assert_eq!(sql.param_order, vec!["handle", "status"]);
// }
