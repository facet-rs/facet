+++
title = "Your first query"
description = "Write Styx queries with LSP support"
weight = 7
+++

Now let's write a query to fetch users from the database.

## Write the query

Add this to `.dibs-queries/queries.styx`:

```styx
@schema {id crate:dibs-queries@1, cli dibs}

UserByEmail @query{
    params {email @string}
    from users
    where {email $email}
    first true
    select {id, email, display_name}
}

CreateUser @insert{
    params {email @string, name @string}
    into users
    values {email $email, display_name $name}
    returning {id, email, display_name}
}
```

This defines two queries:
- `UserByEmail`: fetch a single user by email (returns `Option<UserByEmailResult>`)
- `CreateUser`: insert a new user and return the inserted row

## Generate the Rust code

```bash
cargo build -p my-app-queries
```

This runs `build.rs`, which generates typed Rust functions from your Styx queries.

## Use the generated API

In your application code:

```rust
use my_app_queries::{user_by_email, create_user};
use tokio_postgres::NoTls;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let (client, connection) = tokio_postgres::connect(
        &std::env::var("DATABASE_URL")?,
        NoTls,
    ).await?;

    tokio::spawn(async move {
        if let Err(e) = connection.await {
            eprintln!("connection error: {}", e);
        }
    });

    // Create a user
    let user = create_user(&client, "alice@example.com", "Alice").await?;
    println!("Created user: {:?}", user);

    // Fetch by email
    if let Some(user) = user_by_email(&client, "alice@example.com").await? {
        println!("Found user: {} ({})", user.display_name, user.email);
    }

    Ok(())
}
```

## LSP support

dibs provides an LSP extension for Styx query files that gives you completions, diagnostics, and go-to-definition when editing `.dibs-queries/queries.styx`.

Set up Styx for your code editor. When you open `.dibs-queries/queries.styx`, Styx will detect the `@schema` declaration and offer to enable the dibs LSP extension via a code action. Accept the prompt to enable completions and diagnostics for your queries.

## What gets generated

The generated code includes:

- Result structs with `#[derive(Debug, Clone, Facet)]`
- Async functions that take `&impl GenericClient`
- SQL with placeholders already built
- Proper error handling

Example generated code:

```rust
#[derive(Debug, Clone, Facet)]
pub struct UserByEmailResult {
    pub id: i64,
    pub email: String,
    pub display_name: String,
}

pub async fn user_by_email<C>(
    client: &C,
    email: &str,
) -> Result<Option<UserByEmailResult>, QueryError>
where
    C: tokio_postgres::GenericClient,
{
    const SQL: &str = r#"SELECT "id", "email", "display_name"
                         FROM "users"
                         WHERE "email" = $1"#;

    let rows = client.query(SQL, &[&email]).await?;
    match rows.into_iter().next() {
        Some(row) => Ok(Some(from_row(&row)?)),
        None => Ok(None),
    }
}
```

