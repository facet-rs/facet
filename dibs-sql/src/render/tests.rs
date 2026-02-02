use crate::*;

#[test]
fn test_expr_fncall() {
    let stmt = SelectStmt::new()
        .columns([SelectColumn::expr(Expr::FnCall {
            name: "COALESCE".into(),
            args: vec![
                Expr::qualified_column("u".into(), "nickname".into()),
                Expr::qualified_column("u".into(), "name".into()),
                Expr::String("Anonymous".into()),
            ],
        })])
        .from(FromClause::aliased("users".into(), "u".into()));

    let result = render(&stmt);
    insta::assert_snapshot!(result.sql);
}

#[test]
fn test_expr_count() {
    let stmt = SelectStmt::new()
        .columns([SelectColumn::expr(Expr::Count {
            table: "users".into(),
        })])
        .from(FromClause::table("users".into()));

    let result = render(&stmt);
    insta::assert_snapshot!(result.sql);
}

#[test]
fn test_expr_raw() {
    let stmt = SelectStmt::new()
        .columns([SelectColumn::expr(Expr::Raw("1 + 1 AS computed".into()))])
        .from(FromClause::table("users".into()));

    let result = render(&stmt);
    insta::assert_snapshot!(result.sql);
}

#[test]
fn test_order_by_nulls_first() {
    let stmt = SelectStmt::new()
        .columns([SelectColumn::expr(Expr::column("name".into()))])
        .from(FromClause::table("users".into()))
        .order_by(OrderBy {
            expr: Expr::column("name".into()),
            desc: false,
            nulls: Some(NullsOrder::First),
        });

    let result = render(&stmt);
    insta::assert_snapshot!(result.sql);
}

#[test]
fn test_order_by_nulls_last() {
    let stmt = SelectStmt::new()
        .columns([SelectColumn::expr(Expr::column("name".into()))])
        .from(FromClause::table("users".into()))
        .order_by(OrderBy {
            expr: Expr::column("score".into()),
            desc: true,
            nulls: Some(NullsOrder::Last),
        });

    let result = render(&stmt);
    insta::assert_snapshot!(result.sql);
}

#[test]
fn test_select_column_alias() {
    let stmt = SelectStmt::new()
        .columns([
            SelectColumn::aliased(Expr::column("first_name".into()), "name".into()),
            SelectColumn::aliased(
                Expr::FnCall {
                    name: "COUNT".into(),
                    args: vec![Expr::column("id".into())],
                },
                "total".into(),
            ),
        ])
        .from(FromClause::table("users".into()));

    let result = render(&stmt);
    insta::assert_snapshot!(result.sql);
}

#[test]
fn test_select_all_from_table() {
    let stmt = SelectStmt::new()
        .columns([
            SelectColumn::all_from("users".into()),
            SelectColumn::expr(Expr::qualified_column("posts".into(), "title".into())),
        ])
        .from(FromClause::aliased("users".into(), "users".into()))
        .join(Join {
            kind: JoinKind::Left,
            table: "posts".into(),
            alias: Some("posts".into()),
            on: Expr::qualified_column("posts".into(), "user_id".into())
                .eq(Expr::qualified_column("users".into(), "id".into())),
        });

    let result = render(&stmt);
    insta::assert_snapshot!(result.sql);
}

#[test]
fn test_order_by_multiple_with_comma() {
    let stmt = SelectStmt::new()
        .columns([SelectColumn::expr(Expr::column("name".into()))])
        .from(FromClause::table("users".into()))
        .order_by(OrderBy::desc(Expr::column("created_at".into())))
        .order_by(OrderBy::asc(Expr::column("name".into())));

    let result = render(&stmt);
    insta::assert_snapshot!(result.sql);
}

#[test]
fn test_simple_select() {
    let stmt = SelectStmt::new()
        .columns([
            SelectColumn::expr(Expr::column("id".into())),
            SelectColumn::expr(Expr::column("name".into())),
            SelectColumn::expr(Expr::column("email".into())),
        ])
        .from(FromClause::table("users".into()));

    let result = render(&stmt);
    insta::assert_snapshot!(result.sql);
}

#[test]
fn test_select_with_where_and_order() {
    let stmt = SelectStmt::new()
        .columns([
            SelectColumn::expr(Expr::column("id".into())),
            SelectColumn::expr(Expr::column("name".into())),
        ])
        .from(FromClause::table("users".into()))
        .where_(
            Expr::column("active".into())
                .eq(Expr::Bool(true))
                .and(Expr::column("deleted_at".into()).is_null()),
        )
        .order_by(OrderBy::desc(Expr::column("created_at".into())))
        .limit(Expr::Int(10))
        .offset(Expr::Int(20));

    let result = render(&stmt);
    insta::assert_snapshot!(result.sql);
}

#[test]
fn test_select_with_params() {
    let stmt = SelectStmt::new()
        .columns([
            SelectColumn::expr(Expr::column("id".into())),
            SelectColumn::expr(Expr::column("handle".into())),
            SelectColumn::expr(Expr::column("status".into())),
        ])
        .from(FromClause::table("products".into()))
        .where_(
            Expr::column("handle".into())
                .eq(Expr::param("handle".into()))
                .and(Expr::column("status".into()).eq(Expr::param("status".into()))),
        );

    let result = render(&stmt);
    insta::assert_snapshot!(result.sql);
    assert_eq!(
        result.params,
        vec![ParamName::from("handle"), ParamName::from("status")]
    );
}

#[test]
fn test_select_with_join() {
    let stmt = SelectStmt::new()
        .columns([
            SelectColumn::expr(Expr::qualified_column("t0".into(), "id".into())),
            SelectColumn::expr(Expr::qualified_column("t0".into(), "handle".into())),
            SelectColumn::expr(Expr::qualified_column("t1".into(), "title".into())),
            SelectColumn::expr(Expr::qualified_column("t1".into(), "description".into())),
        ])
        .from(FromClause::aliased("products".into(), "t0".into()))
        .join(Join {
            kind: JoinKind::Left,
            table: "product_translations".into(),
            alias: Some("t1".into()),
            on: Expr::qualified_column("t1".into(), "product_id".into())
                .eq(Expr::qualified_column("t0".into(), "id".into()))
                .and(
                    Expr::qualified_column("t1".into(), "locale".into())
                        .eq(Expr::param("locale".into())),
                ),
        })
        .where_(
            Expr::qualified_column("t0".into(), "handle".into()).eq(Expr::param("handle".into())),
        );

    let result = render(&stmt);
    insta::assert_snapshot!(result.sql);
    assert_eq!(
        result.params,
        vec![ParamName::from("locale"), ParamName::from("handle")]
    );
}

#[test]
fn test_insert_simple() {
    let stmt = InsertStmt::new("products".into())
        .column("handle".into(), Expr::param("handle".into()))
        .column("status".into(), Expr::param("status".into()))
        .column("created_at".into(), Expr::Now)
        .returning(["id".into(), "handle".into(), "status".into()]);

    let result = render(&stmt);
    insta::assert_snapshot!(result.sql);
    assert_eq!(
        result.params,
        vec![ParamName::from("handle"), ParamName::from("status")]
    );
}

#[test]
fn test_insert_with_default() {
    let stmt = InsertStmt::new("products".into())
        .column("handle".into(), Expr::param("handle".into()))
        .column("status".into(), Expr::Default)
        .column("created_at".into(), Expr::Now)
        .returning(["id".into()]);

    let result = render(&stmt);
    insta::assert_snapshot!(result.sql);
    assert_eq!(result.params, vec![ParamName::from("handle")]);
}

#[test]
fn test_upsert() {
    let stmt = InsertStmt::new("products".into())
        .column("handle".into(), Expr::param("handle".into()))
        .column("status".into(), Expr::param("status".into()))
        .column("created_at".into(), Expr::Now)
        .on_conflict(OnConflict {
            columns: vec!["handle".into()],
            action: ConflictAction::DoUpdate(vec![
                UpdateAssignment::new("status".into(), Expr::param("status".into())),
                UpdateAssignment::new("updated_at".into(), Expr::Now),
            ]),
        })
        .returning(["id".into(), "handle".into(), "status".into()]);

    let result = render(&stmt);
    insta::assert_snapshot!(result.sql);
    // Key: params should be deduped - status appears once
    assert_eq!(
        result.params,
        vec![ParamName::from("handle"), ParamName::from("status")]
    );
}

#[test]
fn test_upsert_do_nothing() {
    let stmt = InsertStmt::new("products".into())
        .column("handle".into(), Expr::param("handle".into()))
        .column("status".into(), Expr::param("status".into()))
        .on_conflict(OnConflict {
            columns: vec!["handle".into()],
            action: ConflictAction::DoNothing,
        });

    let result = render(&stmt);
    insta::assert_snapshot!(result.sql);
}

#[test]
fn test_update_simple() {
    let stmt = UpdateStmt::new("products".into())
        .set("status".into(), Expr::param("status".into()))
        .set("updated_at".into(), Expr::Now)
        .where_(Expr::column("handle".into()).eq(Expr::param("handle".into())))
        .returning(["id".into(), "handle".into(), "status".into()]);

    let result = render(&stmt);
    insta::assert_snapshot!(result.sql);
    assert_eq!(
        result.params,
        vec![ParamName::from("status"), ParamName::from("handle")]
    );
}

#[test]
fn test_update_multiple_conditions() {
    let stmt = UpdateStmt::new("products".into())
        .set("deleted_at".into(), Expr::Now)
        .where_(
            Expr::column("handle".into())
                .eq(Expr::param("handle".into()))
                .and(Expr::column("deleted_at".into()).is_null()),
        )
        .returning(["id".into()]);

    let result = render(&stmt);
    insta::assert_snapshot!(result.sql);
}

#[test]
fn test_delete_simple() {
    let stmt = DeleteStmt::new("products".into())
        .where_(Expr::column("id".into()).eq(Expr::param("id".into())))
        .returning(["id".into(), "handle".into()]);

    let result = render(&stmt);
    insta::assert_snapshot!(result.sql);
    assert_eq!(result.params, vec![ParamName::from("id")]);
}

#[test]
fn test_delete_no_returning() {
    let stmt =
        DeleteStmt::new("products".into()).where_(Expr::column("deleted_at".into()).is_not_null());

    let result = render(&stmt);
    insta::assert_snapshot!(result.sql);
}

#[test]
fn test_ilike_search() {
    let stmt = SelectStmt::new()
        .columns([
            SelectColumn::expr(Expr::column("id".into())),
            SelectColumn::expr(Expr::column("handle".into())),
        ])
        .from(FromClause::table("products".into()))
        .where_(Expr::column("handle".into()).ilike(Expr::param("pattern".into())))
        .order_by(OrderBy::asc(Expr::column("handle".into())))
        .limit(Expr::param("limit".into()));

    let result = render(&stmt);
    insta::assert_snapshot!(result.sql);
    assert_eq!(
        result.params,
        vec![ParamName::from("pattern"), ParamName::from("limit")]
    );
}

#[test]
fn test_like_search() {
    let stmt = SelectStmt::new()
        .columns([
            SelectColumn::expr(Expr::column("id".into())),
            SelectColumn::expr(Expr::column("handle".into())),
        ])
        .from(FromClause::table("products".into()))
        .where_(Expr::column("handle".into()).like(Expr::param("pattern".into())));

    let result = render(&stmt);
    insta::assert_snapshot!(result.sql);
    assert_eq!(result.params, vec![ParamName::from("pattern")]);
}

#[test]
fn test_any_in_array() {
    let stmt = SelectStmt::new()
        .columns([
            SelectColumn::expr(Expr::column("id".into())),
            SelectColumn::expr(Expr::column("status".into())),
        ])
        .from(FromClause::table("products".into()))
        .where_(Expr::column("status".into()).any(Expr::param("statuses".into())));

    let result = render(&stmt);
    insta::assert_snapshot!(result.sql);
    assert_eq!(result.params, vec![ParamName::from("statuses")]);
}

#[test]
fn test_jsonb_get() {
    let stmt = SelectStmt::new()
        .columns([SelectColumn::expr(
            Expr::column("metadata".into()).json_get(Expr::String("key".into())),
        )])
        .from(FromClause::table("products".into()));

    let result = render(&stmt);
    insta::assert_snapshot!(result.sql);
}

#[test]
fn test_jsonb_get_text() {
    let stmt = SelectStmt::new()
        .columns([SelectColumn::expr(
            Expr::column("metadata".into()).json_get_text(Expr::String("name".into())),
        )])
        .from(FromClause::table("products".into()));

    let result = render(&stmt);
    insta::assert_snapshot!(result.sql);
}

#[test]
fn test_jsonb_contains() {
    let stmt = SelectStmt::new()
        .columns([SelectColumn::expr(Expr::column("id".into()))])
        .from(FromClause::table("products".into()))
        .where_(Expr::column("metadata".into()).contains(Expr::param("filter".into())));

    let result = render(&stmt);
    insta::assert_snapshot!(result.sql);
    assert_eq!(result.params, vec![ParamName::from("filter")]);
}

#[test]
fn test_jsonb_key_exists() {
    let stmt = SelectStmt::new()
        .columns([SelectColumn::expr(Expr::column("id".into()))])
        .from(FromClause::table("products".into()))
        .where_(Expr::column("metadata".into()).key_exists(Expr::String("featured".into())));

    let result = render(&stmt);
    insta::assert_snapshot!(result.sql);
}

#[test]
fn test_select_distinct() {
    let stmt = SelectStmt::new()
        .distinct()
        .columns([
            SelectColumn::expr(Expr::column("category".into())),
            SelectColumn::expr(Expr::column("status".into())),
        ])
        .from(FromClause::table("products".into()));

    let result = render(&stmt);
    insta::assert_snapshot!(result.sql);
}

#[test]
fn test_select_distinct_on() {
    let stmt = SelectStmt::new()
        .distinct_on([Expr::qualified_column("t0".into(), "user_id".into())])
        .columns([
            SelectColumn::expr(Expr::qualified_column("t0".into(), "id".into())),
            SelectColumn::expr(Expr::qualified_column("t0".into(), "user_id".into())),
            SelectColumn::expr(Expr::qualified_column("t0".into(), "created_at".into())),
        ])
        .from(FromClause::aliased("orders".into(), "t0".into()))
        .order_by(OrderBy::asc(Expr::qualified_column(
            "t0".into(),
            "user_id".into(),
        )))
        .order_by(OrderBy::desc(Expr::qualified_column(
            "t0".into(),
            "created_at".into(),
        )));

    let result = render(&stmt);
    insta::assert_snapshot!(result.sql);
}

#[test]
fn test_select_distinct_on_multiple() {
    let stmt = SelectStmt::new()
        .distinct_on([
            Expr::column("shop_id".into()),
            Expr::column("category_id".into()),
        ])
        .columns([
            SelectColumn::expr(Expr::column("id".into())),
            SelectColumn::expr(Expr::column("shop_id".into())),
            SelectColumn::expr(Expr::column("category_id".into())),
            SelectColumn::expr(Expr::column("name".into())),
        ])
        .from(FromClause::table("products".into()))
        .order_by(OrderBy::asc(Expr::column("shop_id".into())))
        .order_by(OrderBy::asc(Expr::column("category_id".into())))
        .order_by(OrderBy::desc(Expr::column("created_at".into())));

    let result = render(&stmt);
    insta::assert_snapshot!(result.sql);
}

#[test]
fn test_type_cast() {
    let stmt = SelectStmt::new()
        .columns([SelectColumn::expr(
            Expr::param("ids".into()).cast("bigint[]".into()),
        )])
        .from(FromClause::table("products".into()));

    let result = render(&stmt);
    insta::assert_snapshot!(result.sql);
    assert_eq!(result.params, vec![ParamName::from("ids")]);
}

#[test]
fn test_excluded_reference() {
    let stmt = InsertStmt::new("products".into())
        .column("handle".into(), Expr::param("handle".into()))
        .column("status".into(), Expr::param("status".into()))
        .column("updated_at".into(), Expr::Now)
        .on_conflict(OnConflict {
            columns: vec!["handle".into()],
            action: ConflictAction::DoUpdate(vec![
                UpdateAssignment::new("status".into(), Expr::excluded("status".into())),
                UpdateAssignment::new("updated_at".into(), Expr::Now),
            ]),
        })
        .returning(["id".into(), "handle".into()]);

    let result = render(&stmt);
    insta::assert_snapshot!(result.sql);
}

#[test]
fn test_insert_select_unnest() {
    let unnest = Unnest::new("t".into())
        .param("handle".into(), "text[]".into())
        .param("status".into(), "text[]".into());

    let stmt = InsertSelectStmt::new("products".into(), unnest)
        .column("handle".into(), Expr::column("handle".into()))
        .column("status".into(), Expr::column("status".into()))
        .column("created_at".into(), Expr::Now)
        .returning(["id".into(), "handle".into(), "status".into()]);

    let result = render(&stmt);
    insta::assert_snapshot!(result.sql);
    assert_eq!(
        result.params,
        vec![ParamName::from("handle"), ParamName::from("status")]
    );
}

#[test]
fn test_upsert_many_unnest() {
    let unnest = Unnest::new("t".into())
        .param("handle".into(), "text[]".into())
        .param("status".into(), "text[]".into());

    let stmt = InsertSelectStmt::new("products".into(), unnest)
        .column("handle".into(), Expr::column("handle".into()))
        .column("status".into(), Expr::column("status".into()))
        .column("created_at".into(), Expr::Now)
        .on_conflict(OnConflict {
            columns: vec!["handle".into()],
            action: ConflictAction::DoUpdate(vec![
                UpdateAssignment::new("status".into(), Expr::excluded("status".into())),
                UpdateAssignment::new("updated_at".into(), Expr::Now),
            ]),
        })
        .returning(["id".into(), "handle".into(), "status".into()]);

    let result = render(&stmt);
    insta::assert_snapshot!(result.sql);
}
