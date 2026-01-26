use facet::Facet;
use facet_pretty::FacetPretty;
use figue as args;

#[derive(Facet)]
struct Args {
    #[facet(args::named, args::short = 'V')]
    verbose: bool,

    #[facet(args::subcommand)]
    sub: Subcommand,
}

#[derive(Facet)]
#[repr(u8)]
#[allow(dead_code)]
enum Subcommand {
    Init {
        /// Show help for init command.
        #[facet(args::named, args::short = 'h')]
        help: bool,

        /// Name of the project to initialize.
        #[facet(args::named, args::short = 'n')]
        name: Option<String>,

        /// Force overwrite existing files.
        #[facet(args::named, args::short = 'f')]
        force: bool,

        /// Directory path for initialization.
        #[facet(args::positional)]
        path: Option<String>,
    },
    Run {
        /// Port number to listen on.
        #[facet(args::named, args::short = 'p')]
        port: u16,

        /// Run as a background daemon.
        #[facet(args::named)]
        daemon: bool,

        /// Number of worker threads.
        #[facet(args::named)]
        workers: Option<usize>,

        /// Verbose logging level.
        #[facet(args::counted, args::short = 'v')]
        verbose: u8,
    },
}

fn main() {
    let result: Args = figue::from_std_args().unwrap();
    println!("{}", result.pretty());
}
