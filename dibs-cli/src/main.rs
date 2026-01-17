use facet::Facet;
use facet_args as args;
use jiff::Zoned;

// Example table definition for testing
#[derive(Facet)]
#[facet(derive(dibs::Table), dibs::table = "users")]
struct User {
    #[facet(dibs::pk)]
    id: i64,

    #[facet(dibs::unique)]
    email: String,

    name: String,

    bio: Option<String>,

    #[facet(dibs::fk = "tenants.id")]
    tenant_id: i64,
}

/// Postgres toolkit for Rust, powered by facet reflection.
#[derive(Facet, Debug)]
struct Cli {
    /// Show version information
    #[facet(args::named, args::short = 'V')]
    version: bool,

    /// Command to run
    #[facet(default, args::subcommand)]
    command: Option<Commands>,
}

/// Available commands
#[derive(Facet, Debug)]
#[repr(u8)]
enum Commands {
    /// Run pending migrations
    Migrate {
        /// Database connection URL
        #[facet(default, args::named)]
        database_url: Option<String>,
    },
    /// Show migration status
    Status {
        /// Database connection URL
        #[facet(default, args::named)]
        database_url: Option<String>,
    },
    /// Compare schema to database
    Diff {
        /// Database connection URL
        #[facet(default, args::named)]
        database_url: Option<String>,
    },
    /// Generate a migration skeleton
    Generate {
        /// Migration name (e.g., "add-users-table")
        #[facet(args::positional)]
        name: String,
    },
    /// Dump the current schema
    Schema {
        /// Database connection URL
        #[facet(default, args::named)]
        database_url: Option<String>,
    },
}

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let args_ref: Vec<&str> = args.iter().map(|s| s.as_str()).collect();

    let result: Result<Cli, _> = args::from_slice(&args_ref);

    match result {
        Ok(cli) => run(cli),
        Err(err) if err.is_help_request() => {
            print!("{}", err.help_text().unwrap_or(""));
        }
        Err(err) => {
            eprintln!("{}", err);
            std::process::exit(1);
        }
    }
}

fn run(cli: Cli) {
    if cli.version {
        println!("dibs {}", env!("CARGO_PKG_VERSION"));
        return;
    }

    match cli.command {
        Some(Commands::Migrate { database_url }) => {
            println!("dibs migrate");
            if let Some(url) = database_url {
                println!("  database: {}", mask_password(&url));
            }
            println!("  (not yet implemented)");
        }
        Some(Commands::Status { database_url }) => {
            println!("dibs status");
            if let Some(url) = database_url {
                println!("  database: {}", mask_password(&url));
            }
            println!("  (not yet implemented)");
        }
        Some(Commands::Diff { database_url }) => {
            println!("dibs diff");
            if let Some(url) = database_url {
                println!("  database: {}", mask_password(&url));
            }
            println!("  (not yet implemented)");
        }
        Some(Commands::Generate { name }) => {
            let date = Zoned::now().date();
            println!("dibs generate {}", name);
            println!("  Would create: migrations/{}-{}.rs", date, name);
            println!("  (not yet implemented)");
        }
        Some(Commands::Schema { database_url: _ }) => {
            let schema = dibs::Schema::collect();

            if schema.tables.is_empty() {
                println!("No tables registered.");
                println!();
                println!("Define tables using #[facet(dibs::table = \"name\")] on Facet structs,");
                println!(
                    "then register them with: inventory::submit!(dibs::TableDef::new::<YourType>());"
                );
            } else {
                println!("Schema ({} tables):", schema.tables.len());
                println!();
                for table in &schema.tables {
                    println!("  {} ({} columns)", table.name, table.columns.len());
                    for col in &table.columns {
                        let mut attrs = Vec::new();
                        if col.primary_key {
                            attrs.push("PK");
                        }
                        if col.unique {
                            attrs.push("UNIQUE");
                        }
                        if !col.nullable {
                            attrs.push("NOT NULL");
                        }
                        if let Some(default) = &col.default {
                            attrs.push(default);
                        }

                        let attrs_str = if attrs.is_empty() {
                            String::new()
                        } else {
                            format!(" [{}]", attrs.join(", "))
                        };

                        println!("    {}: {}{}", col.name, col.pg_type, attrs_str);
                    }

                    for fk in &table.foreign_keys {
                        println!(
                            "    FK: {} -> {}.{}",
                            fk.columns.join(", "),
                            fk.references_table,
                            fk.references_columns.join(", ")
                        );
                    }
                    println!();
                }
            }
        }
        None => {
            let config = args::HelpConfig {
                program_name: Some("dibs".to_string()),
                version: Some(env!("CARGO_PKG_VERSION").to_string()),
                ..Default::default()
            };
            print!("{}", args::generate_help::<Cli>(&config));
        }
    }
}

/// Mask password in database URL for display
fn mask_password(url: &str) -> String {
    // Simple masking: replace password between :// and @
    if let Some(start) = url.find("://") {
        if let Some(at) = url.find('@') {
            let prefix = &url[..start + 3];
            let suffix = &url[at..];
            if let Some(colon) = url[start + 3..at].find(':') {
                let user = &url[start + 3..start + 3 + colon];
                return format!("{}{}:***{}", prefix, user, suffix);
            }
        }
    }
    url.to_string()
}
