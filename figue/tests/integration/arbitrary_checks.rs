use arbitrary::Arbitrary;
use facet::Facet;
use figue::{
    self as args, TestToArgsConsistencyConfig, TestToArgsRoundTrip,
    assert_to_args_consistency, assert_to_args_roundtrip,
};

#[derive(Facet, Arbitrary, Debug, PartialEq)]
#[repr(u8)]
enum Command {
    Build {
        #[facet(args::named)]
        release: bool,

        #[facet(args::positional)]
        target: Option<String>,
    },
    Clean,
}

#[derive(Facet, Arbitrary, Debug, PartialEq)]
struct Cli {
    #[facet(args::named)]
    verbose: bool,

    #[facet(args::subcommand)]
    command: Command,
}

#[test]
fn exported_consistency_helper_smoke_test() {
    assert_to_args_consistency::<Cli>(TestToArgsConsistencyConfig {
        success_count: 8,
        max_attempts: 256,
        ..Default::default()
    })
    .expect("consistency helper should succeed");
}

#[test]
fn exported_roundtrip_helper_smoke_test() {
    assert_to_args_roundtrip::<Cli>(TestToArgsRoundTrip {
        success_count_per_leaf: 2,
        success_count_global: 2,
        max_attempts_per_leaf: 256,
        max_attempts_global: 256,
        ..Default::default()
    })
    .expect("roundtrip helper should succeed");
}
