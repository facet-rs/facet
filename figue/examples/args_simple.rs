use facet::Facet;
use figue as args;

#[derive(Debug, Facet)]
struct Args {
    #[facet(args::positional)]
    path: String,

    #[facet(args::named, args::short = 'v')]
    verbose: bool,

    #[facet(flatten)]
    builtins: args::FigueBuiltins,
}

fn main() {
    let args: Args = args::from_std_args().unwrap();
    println!("{args:?}");
}
