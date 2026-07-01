use facet::Facet;
use facet_testhelpers::test;
use figue::{self as args, ToArgs};

#[derive(Facet, Debug, PartialEq)]


#[repr(u8)]
enum Command {
    Build {
        #[facet(args::named, args::short = 'r')]
        release: bool,

        #[facet(args::positional)]
        target: Option<String>,
    },
    Clean,
}

#[derive(Facet, Debug, PartialEq)]


struct Cli {
    #[facet(args::named, args::short = 'v')]
    verbose: bool,

    #[facet(args::subcommand)]
    command: Command,
}

#[test]
fn test_to_args_roundtrip() {
    let original = Cli {
        verbose: true,
        command: Command::Build {
            release: true,
            target: Some("app".to_string()),
        },
    };

    let args = original
        .to_args()
        .expect("to_args should serialize CLI value");
    let arg_strings = args
        .iter()
        .map(|arg| arg.to_string_lossy().to_string())
        .collect::<Vec<_>>();
    let arg_refs = arg_strings.iter().map(String::as_str).collect::<Vec<_>>();

    let parsed: Cli = figue::from_slice(&arg_refs)
        .into_result()
        .expect("roundtrip parse should succeed")
        .get_silent();

    assert_eq!(original, parsed);
}

#[test]
fn test_to_args_deterministic() {
    let cli = Cli {
        verbose: true,
        command: Command::Build {
            release: false,
            target: Some("worker".to_string()),
        },
    };

    let args1 = cli.to_args().expect("first conversion should succeed");
    let args2 = cli.to_args().expect("second conversion should succeed");

    assert_eq!(args1, args2);
}

#[test]
fn test_clean_command_hint_includes_full_invocation() {
    let cli = Cli {
        verbose: false,
        command: Command::Clean,
    };

    let args_string = cli
        .to_args_string()
        .expect("clean to_args_string should succeed");
    let full_command = cli
        .to_args_string_with_current_exe()
        .expect("clean to_args_string_with_current_exe should succeed");
    let exe_display = std::env::current_exe()
        .expect("current_exe should resolve during tests")
        .to_string_lossy()
        .to_string();

    let hint = format!(
        "If you're having trouble with your builds, run the clean command: {}",
        full_command
    );

    assert!(
        hint.contains("If you're having trouble with your builds"),
        "hint should include support message"
    );
    assert!(
        hint.contains("clean"),
        "hint should include clean subcommand"
    );
    assert_eq!(args_string, "clean", "args_string should only include args");
    assert!(
        hint.contains(&exe_display),
        "hint should include executable path"
    );
}


