use std::path::PathBuf;

use facet::Facet;

use eyre::{Ok, Result};

#[test]
fn test_arg_with_subcommand_parse_unit() -> Result<()> {
    facet_testhelpers::setup();
    #[derive(Facet)]
    #[facet(rename_all = "snake_case")]
    struct Args {
        #[facet(positional)]
        commands: Commands,
        #[facet(named, short = 'h')]
        help: bool,
    }

    #[derive(Facet, Debug, PartialEq)]
    #[repr(u8)]
    enum Commands {
        #[facet(renamerule)]
        Test,
    }

    let args: Args = facet_args::from_slice(&["test", "/path/"])?;
    assert_eq!(args.commands, Commands::Test {});
    assert!(!args.help);
    Ok(())
}

#[test]
#[should_panic]
fn test_non_unit_like_enums_are_unsupported() {
    facet_testhelpers::setup();
    #[derive(Facet)]
    struct Args {
        #[facet(positional)]
        commands: Commands,
    }

    #[derive(Facet, Debug, PartialEq)]
    #[repr(u8)]
    enum Commands {
        Test { path: PathBuf },
    }

    let args: Args = facet_args::from_slice(&["test", "/path/"]).unwrap();
    assert_eq!(
        args.commands,
        Commands::Test {
            path: PathBuf::from("/path/")
        }
    );
}
