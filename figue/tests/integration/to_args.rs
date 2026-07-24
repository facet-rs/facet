use facet::Facet;
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

#[derive(Facet, Debug, PartialEq)]
struct AcquireArgs {
    #[facet(default, args::positional)]
    target: Option<String>,

    #[facet(default = false, args::named)]
    all: bool,
}

#[derive(Facet, Debug, PartialEq)]
struct AttemptArgs {
    #[facet(default, args::positional)]
    attempts: Option<u16>,

    #[facet(default = false, args::named)]
    all: bool,
}

#[derive(Facet, Debug, PartialEq)]
struct FilterArgs {
    #[facet(default, args::positional)]
    target: Option<String>,

    #[facet(args::named)]
    log_filter: Option<String>,
}

fn to_strings(args: Vec<std::ffi::OsString>) -> Vec<String> {
    args.into_iter()
        .map(|arg| arg.to_string_lossy().into_owned())
        .collect()
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
fn optional_string_positional_none_omits_the_token_and_roundtrips() {
    let parsed: AcquireArgs = figue::from_slice(&["--all"])
        .into_result()
        .expect("omitted optional positional should parse")
        .get_silent();
    assert_eq!(
        parsed,
        AcquireArgs {
            target: None,
            all: true,
        }
    );

    let original = AcquireArgs {
        target: None,
        all: true,
    };
    let rendered = to_strings(
        original
            .to_args()
            .expect("none optional positional should serialize"),
    );
    assert_eq!(rendered, vec!["--all"]);

    let arg_refs = rendered.iter().map(String::as_str).collect::<Vec<_>>();
    let reparsed: AcquireArgs = figue::from_slice(&arg_refs)
        .into_result()
        .expect("rendered arguments should parse")
        .get_silent();
    assert_eq!(reparsed, original);
}

#[test]
fn optional_string_positional_some_emits_a_normal_token_and_roundtrips() {
    let original = AcquireArgs {
        target: Some("cc-tweaked".to_string()),
        all: true,
    };
    let rendered = to_strings(
        original
            .to_args()
            .expect("some optional positional should serialize"),
    );
    assert_eq!(rendered, vec!["--all", "cc-tweaked"]);

    let arg_refs = rendered.iter().map(String::as_str).collect::<Vec<_>>();
    let reparsed: AcquireArgs = figue::from_slice(&arg_refs)
        .into_result()
        .expect("rendered arguments should parse")
        .get_silent();
    assert_eq!(reparsed, original);
}

#[test]
fn optional_integer_positional_none_and_some_roundtrip() {
    let none = AttemptArgs {
        attempts: None,
        all: true,
    };
    let none_rendered = to_strings(
        none.to_args()
            .expect("none optional integer positional should serialize"),
    );
    assert_eq!(none_rendered, vec!["--all"]);
    let none_refs = none_rendered.iter().map(String::as_str).collect::<Vec<_>>();
    let none_reparsed: AttemptArgs = figue::from_slice(&none_refs)
        .into_result()
        .expect("rendered arguments should parse")
        .get_silent();
    assert_eq!(none_reparsed, none);

    let some = AttemptArgs {
        attempts: Some(42),
        all: true,
    };
    let some_rendered = to_strings(
        some.to_args()
            .expect("some optional integer positional should serialize"),
    );
    assert_eq!(some_rendered, vec!["--all", "42"]);
    let some_refs = some_rendered.iter().map(String::as_str).collect::<Vec<_>>();
    let some_reparsed: AttemptArgs = figue::from_slice(&some_refs)
        .into_result()
        .expect("rendered arguments should parse")
        .get_silent();
    assert_eq!(some_reparsed, some);
}

#[test]
fn optional_named_value_renders_when_the_positional_is_absent() {
    let original = FilterArgs {
        target: None,
        log_filter: Some("debug".to_string()),
    };
    let rendered = to_strings(
        original
            .to_args()
            .expect("optional named value should serialize"),
    );
    assert_eq!(rendered, vec!["--log-filter", "debug"]);

    let arg_refs = rendered.iter().map(String::as_str).collect::<Vec<_>>();
    let reparsed: FilterArgs = figue::from_slice(&arg_refs)
        .into_result()
        .expect("rendered arguments should parse")
        .get_silent();
    assert_eq!(reparsed, original);
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
    let args_string = args_string.to_string_lossy().to_string();
    let full_command = full_command.to_string_lossy().to_string();
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

#[derive(Facet, Debug, PartialEq)]
struct NestedOptionalArgs {
    /// `--level` (bare) = Some(None), `--level 3` = Some(Some(3)), absent = None.
    #[facet(default, args::named)]
    level: Option<Option<u32>>,
}

#[test]
fn optional_value_flag_some_none_emits_bare_flag_and_roundtrips() {
    let original = NestedOptionalArgs { level: Some(None) };

    let rendered = to_strings(
        original
            .to_args()
            .expect("Some(None) optional-value flag should serialize"),
    );
    assert_eq!(rendered, vec!["--level"]);

    let arg_refs = rendered.iter().map(String::as_str).collect::<Vec<_>>();
    let reparsed: NestedOptionalArgs = figue::from_slice(&arg_refs)
        .into_result()
        .expect("rendered arguments should parse")
        .get_silent();
    assert_eq!(reparsed, original);
}

#[test]
fn optional_value_flag_some_some_emits_value_and_roundtrips() {
    let original = NestedOptionalArgs {
        level: Some(Some(3)),
    };

    let rendered = to_strings(
        original
            .to_args()
            .expect("Some(Some) optional-value flag should serialize"),
    );
    assert_eq!(rendered, vec!["--level", "3"]);

    let arg_refs = rendered.iter().map(String::as_str).collect::<Vec<_>>();
    let reparsed: NestedOptionalArgs = figue::from_slice(&arg_refs)
        .into_result()
        .expect("rendered arguments should parse")
        .get_silent();
    assert_eq!(reparsed, original);
}
