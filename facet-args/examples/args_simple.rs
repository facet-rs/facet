use facet::Facet;
use facet_args as args;

#[derive(Facet)]
struct Args {
    #[facet(args::positional)]
    path: String,

    #[facet(args::named, args::short = 'v')]
    verbose: bool,
}

fn main() {
    args::builder::<Args>();
}
