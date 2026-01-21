//! Snapshot tests for SQL rendering.

use dibs_sql::*;

#[test]
fn test_simple_select() {
    let stmt = SelectStmt::new()
        .columns([
            SelectColumn::expr(Expr::column("id")),
            SelectColumn::expr(Expr::column("name")),
            SelectColumn::expr(Expr::column("email")),
        ])
        .from(FromClause::table("users"));

    let result = render(&stmt);
    insta::assert_snapshot!(result.sql);
}

#[test]
fn test_select_with_where_and_order() {
    let stmt = SelectStmt::new()
        .columns([
            SelectColumn::expr(Expr::column("id")),
            SelectColumn::expr(Expr::column("name")),
        ])
        .from(FromClause::table("users"))
        .where_(
            Expr::column("active")
                .eq(Expr::Bool(true))
                .and(Expr::column("deleted_at").is_null()),
        )
        .order_by(OrderBy::desc(Expr::column("created_at")))
        .limit(Expr::Int(10))
        .offset(Expr::Int(20));

    let result = render(&stmt);
    insta::assert_snapshot!(result.sql);
}

#[test]
fn test_select_with_params() {
    let stmt = SelectStmt::new()
        .columns([
            SelectColumn::expr(Expr::column("id")),
            SelectColumn::expr(Expr::column("handle")),
            SelectColumn::expr(Expr::column("status")),
        ])
        .from(FromClause::table("products"))
        .where_(
            Expr::column("handle")
                .eq(Expr::param("handle"))
                .and(Expr::column("status").eq(Expr::param("status"))),
        );

    let result = render(&stmt);
    insta::assert_snapshot!(result.sql);
    assert_eq!(result.params, vec!["handle", "status"]);
}

#[test]
fn test_select_with_join() {
    let stmt = SelectStmt::new()
        .columns([
            SelectColumn::expr(Expr::qualified_column("t0", "id")),
            SelectColumn::expr(Expr::qualified_column("t0", "handle")),
            SelectColumn::expr(Expr::qualified_column("t1", "title")),
            SelectColumn::expr(Expr::qualified_column("t1", "description")),
        ])
        .from(FromClause::aliased("products", "t0"))
        .join(Join {
            kind: JoinKind::Left,
            table: "product_translations".into(),
            alias: Some("t1".into()),
            on: Expr::qualified_column("t1", "product_id")
                .eq(Expr::qualified_column("t0", "id"))
                .and(Expr::qualified_column("t1", "locale").eq(Expr::param("locale"))),
        })
        .where_(Expr::qualified_column("t0", "handle").eq(Expr::param("handle")));

    let result = render(&stmt);
    insta::assert_snapshot!(result.sql);
    assert_eq!(result.params, vec!["locale", "handle"]);
}

#[test]
fn test_insert_simple() {
    let stmt = InsertStmt::new("products")
        .column("handle", Expr::param("handle"))
        .column("status", Expr::param("status"))
        .column("created_at", Expr::Now)
        .returning(["id", "handle", "status"]);

    let result = render(&stmt);
    insta::assert_snapshot!(result.sql);
    assert_eq!(result.params, vec!["handle", "status"]);
}

#[test]
fn test_insert_with_default() {
    let stmt = InsertStmt::new("products")
        .column("handle", Expr::param("handle"))
        .column("status", Expr::Default)
        .column("created_at", Expr::Now)
        .returning(["id"]);

    let result = render(&stmt);
    insta::assert_snapshot!(result.sql);
    assert_eq!(result.params, vec!["handle"]);
}

#[test]
fn test_upsert() {
    let stmt = InsertStmt::new("products")
        .column("handle", Expr::param("handle"))
        .column("status", Expr::param("status"))
        .column("created_at", Expr::Now)
        .on_conflict(OnConflict {
            columns: vec!["handle".into()],
            action: ConflictAction::DoUpdate(vec![
                UpdateAssignment::new("status", Expr::param("status")),
                UpdateAssignment::new("updated_at", Expr::Now),
            ]),
        })
        .returning(["id", "handle", "status"]);

    let result = render(&stmt);
    insta::assert_snapshot!(result.sql);
    // Key: params should be deduped - status appears once
    assert_eq!(result.params, vec!["handle", "status"]);
}

#[test]
fn test_upsert_do_nothing() {
    let stmt = InsertStmt::new("products")
        .column("handle", Expr::param("handle"))
        .column("status", Expr::param("status"))
        .on_conflict(OnConflict {
            columns: vec!["handle".into()],
            action: ConflictAction::DoNothing,
        });

    let result = render(&stmt);
    insta::assert_snapshot!(result.sql);
}

#[test]
fn test_update_simple() {
    let stmt = UpdateStmt::new("products")
        .set("status", Expr::param("status"))
        .set("updated_at", Expr::Now)
        .where_(Expr::column("handle").eq(Expr::param("handle")))
        .returning(["id", "handle", "status"]);

    let result = render(&stmt);
    insta::assert_snapshot!(result.sql);
    assert_eq!(result.params, vec!["status", "handle"]);
}

#[test]
fn test_update_multiple_conditions() {
    let stmt = UpdateStmt::new("products")
        .set("deleted_at", Expr::Now)
        .where_(
            Expr::column("handle")
                .eq(Expr::param("handle"))
                .and(Expr::column("deleted_at").is_null()),
        )
        .returning(["id"]);

    let result = render(&stmt);
    insta::assert_snapshot!(result.sql);
}

#[test]
fn test_delete_simple() {
    let stmt = DeleteStmt::new("products")
        .where_(Expr::column("id").eq(Expr::param("id")))
        .returning(["id", "handle"]);

    let result = render(&stmt);
    insta::assert_snapshot!(result.sql);
    assert_eq!(result.params, vec!["id"]);
}

#[test]
fn test_delete_no_returning() {
    let stmt = DeleteStmt::new("products").where_(Expr::column("deleted_at").is_not_null());

    let result = render(&stmt);
    insta::assert_snapshot!(result.sql);
}

#[test]
fn test_ilike_search() {
    let stmt = SelectStmt::new()
        .columns([
            SelectColumn::expr(Expr::column("id")),
            SelectColumn::expr(Expr::column("handle")),
        ])
        .from(FromClause::table("products"))
        .where_(Expr::column("handle").ilike(Expr::param("pattern")))
        .order_by(OrderBy::asc(Expr::column("handle")))
        .limit(Expr::param("limit"));

    let result = render(&stmt);
    insta::assert_snapshot!(result.sql);
    assert_eq!(result.params, vec!["pattern", "limit"]);
}

#[test]
fn test_pretty_select() {
    let stmt = SelectStmt::new()
        .columns([
            SelectColumn::expr(Expr::column("id")),
            SelectColumn::expr(Expr::column("handle")),
            SelectColumn::expr(Expr::column("status")),
        ])
        .from(FromClause::table("products"))
        .where_(
            Expr::column("status")
                .eq(Expr::String("active".into()))
                .and(Expr::column("deleted_at").is_null()),
        )
        .order_by(OrderBy::desc(Expr::column("created_at")))
        .limit(Expr::Int(10));

    let result = render_pretty(&stmt);
    insta::assert_snapshot!(result.sql);
}

#[test]
fn test_pretty_insert() {
    let stmt = InsertStmt::new("products")
        .column("handle", Expr::param("handle"))
        .column("status", Expr::param("status"))
        .column("created_at", Expr::Now)
        .returning(["id", "handle", "status"]);

    let result = render_pretty(&stmt);
    insta::assert_snapshot!(result.sql);
}

#[test]
fn test_pretty_upsert() {
    let stmt = InsertStmt::new("products")
        .column("handle", Expr::param("handle"))
        .column("status", Expr::param("status"))
        .column("created_at", Expr::Now)
        .on_conflict(OnConflict {
            columns: vec!["handle".into()],
            action: ConflictAction::DoUpdate(vec![
                UpdateAssignment::new("status", Expr::param("status")),
                UpdateAssignment::new("updated_at", Expr::Now),
            ]),
        })
        .returning(["id", "handle", "status"]);

    let result = render_pretty(&stmt);
    insta::assert_snapshot!(result.sql);
}
