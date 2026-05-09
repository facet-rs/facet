use facet::Facet;
use facet_pretty::FacetPretty;
use figue::{self as args, Driver};

/// Figue kitchen sink example.
///
/// Shows **CLI args**, **environment variables**, **config files**, nested config,
/// defaults, subcommands, enum-backed config, and Markdown-ish doc comments.
///
/// Kitchen Sink is a deliberately over-specified service runner used to exercise
/// every layer of Figue's generated documentation. Imagine a small operations
/// team using this utility to run a local development copy of an API service,
/// print the fully merged runtime configuration, and validate that production
/// defaults can be overridden from the command line without editing files.
///
/// The app has four configuration sources. CLI flags win over environment
/// variables, environment variables win over config files, config files win over
/// Rust defaults, and missing required fields stay visible until the user
/// supplies them. The generated HTML should make that precedence obvious without
/// forcing users to read the source code.
///
/// The `settings` root is intentionally nested. It contains server listener
/// values, database pool values, logging controls, an enum-backed storage
/// backend, and optional TLS credentials. That shape is useful for testing
/// collapsible config schemas, search, default value rendering, and CLI override
/// examples such as `--settings.server.port 9090`.
///
/// The storage backend demonstrates enum variants in configuration files. A
/// user can choose `local`, `s3`, or `memory`; the `s3` variant has its own
/// nested fields like `bucket`, `region`, and `endpoint`. This is meant to be
/// deep enough that the HTML needs navigation and search to stay comfortable.
///
/// The command layer is intentionally separate from configuration. `serve`
/// starts the service and has its own command-specific option, while
/// `print-config` renders the merged configuration. The HTML help should show
/// these command-level arguments near the command, not leave the user guessing
/// that they need to run another help command.
///
/// This comment is also intentionally long. It is here to stress the page layout:
/// the opening description should be readable, but it should not push the
/// options and config schema several screens away. The generated page clamps this
/// introduction and lets the reader expand it when they actually want the full
/// narrative.
#[derive(Debug, Facet)]
struct App {
    /// Increase logging verbosity. Repeat with `-vv` for extra detail.
    #[facet(args::named, args::short = 'v', args::counted, default)]
    verbose: u8,

    /// Standard `--help`, `--html-help`, `--version`, and completion flags.
    #[facet(flatten)]
    builtins: args::FigueBuiltins,

    /// Application settings loaded from file/env/CLI.
    #[facet(args::config, args::env_prefix = "KITCHEN")]
    settings: Settings,

    /// Action to run after configuration is loaded.
    #[facet(args::subcommand)]
    command: Command,
}

/// Runtime settings.
///
/// Environment variables use `KITCHEN__...`, for example
/// `KITCHEN__SERVER__PORT=9090`.
#[derive(Debug, Facet)]
struct Settings {
    /// Service name shown in logs and metrics.
    #[facet(default = "figue-kitchen")]
    service_name: String,

    /// Server listener configuration.
    server: Server,

    /// Database and pool configuration.
    database: Database,

    /// Logging output and filtering.
    logging: Logging,

    /// Storage backend. Try searching for `s3` in HTML help.
    storage: Storage,

    /// Optional TLS settings. `null` means plain HTTP.
    #[facet(default)]
    tls: Option<Tls>,
}

#[derive(Debug, Facet)]
struct Server {
    /// Interface to bind, for example `127.0.0.1`.
    #[facet(default = "127.0.0.1")]
    host: String,

    /// TCP port to listen on.
    #[facet(default = 8080)]
    port: u16,

    /// Number of worker threads.
    #[facet(default = 4)]
    workers: usize,
}

#[derive(Debug, Facet)]
struct Database {
    /// Database URL. This is intentionally required.
    url: String,

    /// Maximum open connections in the pool.
    #[facet(default = 16)]
    max_connections: u32,

    /// Connection timeout in seconds.
    #[facet(default = 30)]
    timeout_secs: u64,
}

#[derive(Debug, Facet)]
struct Logging {
    /// Log level: `trace`, `debug`, `info`, `warn`, or `error`.
    #[facet(default = "info")]
    level: String,

    /// Emit structured JSON logs.
    #[facet(default)]
    json: bool,
}

#[derive(Debug, Facet)]
#[facet(rename_all = "kebab-case")]
#[repr(u8)]
#[allow(dead_code)]
enum Storage {
    /// Store files on local disk.
    Local {
        /// Directory used for local storage.
        path: String,
    },

    /// Store files in S3-compatible object storage.
    S3 {
        /// Bucket name.
        bucket: String,

        /// Region name.
        #[facet(default = "us-east-1")]
        region: String,

        /// Optional custom endpoint for S3-compatible providers.
        #[facet(default)]
        endpoint: Option<String>,
    },

    /// Store files only in memory.
    Memory,
}

#[derive(Debug, Facet)]
struct Tls {
    /// Certificate path.
    cert_path: String,

    /// Private key path.
    key_path: String,
}

#[derive(Debug, Facet)]
#[repr(u8)]
enum Command {
    /// Start the service.
    Serve {
        /// Run migrations before accepting traffic.
        #[facet(args::named, default)]
        migrate: bool,
    },

    /// Print the merged configuration.
    PrintConfig,
}

fn main() {
    let config = args::builder::<App>()
        .unwrap()
        .cli(|cli| cli.args(std::env::args().skip(1)))
        .env(|env| env)
        .file(|file| {
            file.format(args::JsoncFormat).default_paths([
                "crates/figue/examples/kitchen_sink.jsonc",
                "crates/figue/examples/kitchen_sink.json",
                "kitchen_sink.jsonc",
                "kitchen_sink.json",
            ])
        })
        .help(|help| {
            help.program_name("kitchen-sink")
                .version(env!("CARGO_PKG_VERSION"))
                .description(
                    "Search for `database`, `s3`, `default`, `--settings.server.port`, or `KITCHEN__SERVER__PORT`.",
                )
        })
        .build();

    let app = Driver::new(config).run().unwrap();

    match app.command {
        Command::Serve { migrate } => {
            println!(
                "serving {} with migrate={migrate}",
                app.settings.service_name
            );
            println!("{}", app.pretty());
        }
        Command::PrintConfig => {
            println!("{}", app.pretty());
        }
    }
}
