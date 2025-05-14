#![cfg(test)]

// use std::path::PathBuf;

use facet::Facet;

use facet_testhelpers::test;

#[test]
fn test_arg_with_subcommand_parse_unit() {
    #[derive(Facet)]
    struct Args {
        #[facet(positional)]
        commands: Commands,
        #[facet(named, short = 'h')]
        help: bool,
    }

    #[derive(Facet, Debug, PartialEq)]
    #[repr(u8)]
    enum Commands {
        Test,
    }

    let args: Args = facet_args::from_slice(&["test"])?;
    assert_eq!(args.commands, Commands::Test {});
    assert!(!args.help);
}

// #[test]
// fn test_non_unit_like_enums_are_unsupported() {
//     #[derive(Facet)]
//     struct Args {
//         #[facet(positional)]
//         commands: Commands,
//     }

//     #[derive(Facet, Debug, PartialEq)]
//     #[repr(u8)]
//     enum Commands {
//         Test { path: PathBuf },
//     }

//     let args: Args = facet_args::from_slice(&["test", "/path/"]).unwrap();
//     assert_eq!(
//         args.commands,
//         Commands::Test {
//             path: PathBuf::from("/path/")
//         }
//     );
// }
