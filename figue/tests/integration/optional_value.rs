use facet::Facet;
use figue::{self as args};

#[derive(Facet, Debug, PartialEq)]
struct OptionalArgs {
    #[facet(args::named, args::short = 'p')]
    parallel: Option<Option<usize>>,

    #[facet(args::named)]
    dry_run: bool,
}

#[test]
fn optional_value_long_forms() {
    let args: OptionalArgs = figue::from_slice::<OptionalArgs>(&[]).unwrap();
    assert_eq!(args.parallel, None);
    assert!(!args.dry_run);

    let args: OptionalArgs = figue::from_slice(&["--parallel"]).unwrap();
    assert_eq!(args.parallel, Some(None));

    let args: OptionalArgs = figue::from_slice(&["--parallel", "12"]).unwrap();
    assert_eq!(args.parallel, Some(Some(12)));

    let args: OptionalArgs = figue::from_slice(&["--parallel=12"]).unwrap();
    assert_eq!(args.parallel, Some(Some(12)));

    let args: OptionalArgs = figue::from_slice(&["--parallel", "--dry-run"]).unwrap();
    assert_eq!(args.parallel, Some(None));
    assert!(args.dry_run);
}

#[test]
fn optional_value_duplicate_non_multiple_errors() {
    let err = figue::from_slice::<OptionalArgs>(&["--parallel", "--parallel=2"]).unwrap_err();
    assert!(
        err.to_string().contains("provided multiple times"),
        "unexpected error: {err}"
    );
}

#[test]
fn optional_value_short_forms() {
    let args: OptionalArgs = figue::from_slice(&["-p"]).unwrap();
    assert_eq!(args.parallel, Some(None));

    let args: OptionalArgs = figue::from_slice(&["-p12"]).unwrap();
    assert_eq!(args.parallel, Some(Some(12)));

    let args: OptionalArgs = figue::from_slice(&["-p=12"]).unwrap();
    assert_eq!(args.parallel, Some(Some(12)));
}

#[derive(Facet, Debug, PartialEq)]
struct OptionalShortCluster {
    #[facet(args::named, args::short = 'p')]
    parallel: Option<Option<String>>,

    #[facet(args::named, args::short = 'v')]
    verbose: bool,
}

#[test]
fn optional_value_short_cluster_trailing_chars_are_value() {
    let args: OptionalShortCluster = figue::from_slice(&["-vp12"]).unwrap();
    assert!(args.verbose);
    assert_eq!(args.parallel, Some(Some("12".to_string())));

    let args: OptionalShortCluster = figue::from_slice(&["-pv"]).unwrap();
    assert!(!args.verbose);
    assert_eq!(args.parallel, Some(Some("v".to_string())));
}

#[derive(Facet, Debug, PartialEq)]
struct OptionalNegative {
    #[facet(args::named)]
    limit: Option<Option<isize>>,
}

#[test]
fn optional_value_dash_prefixed_values_use_equals() {
    let args: OptionalNegative = figue::from_slice(&["--limit=-3"]).unwrap();
    assert_eq!(args.limit, Some(Some(-3)));

    let err = figue::from_slice::<OptionalNegative>(&["--limit", "-3"]).unwrap_err();
    assert!(
        err.to_string().contains("unknown flag: -3"),
        "unexpected error: {err}"
    );
}

#[derive(Facet, Debug, PartialEq)]
struct App {
    #[facet(args::named)]
    parallel: Option<Option<usize>>,

    #[facet(args::subcommand)]
    command: Command,
}

#[derive(Facet, Debug, PartialEq)]
#[repr(u8)]
enum Command {
    Run {
        #[facet(args::named)]
        dry_run: bool,
    },
}

#[test]
fn optional_value_global_adoption_after_subcommand() {
    let app: App = figue::from_slice(&["run", "--parallel", "--dry-run"]).unwrap();
    assert_eq!(app.parallel, Some(None));
    assert_eq!(app.command, Command::Run { dry_run: true });
}

#[derive(Facet, Debug, PartialEq)]
struct RequiredValueArgs {
    #[facet(args::named, args::short = 'j')]
    concurrency: usize,
}

#[test]
fn required_value_flags_still_error_when_bare() {
    let err = figue::from_slice::<RequiredValueArgs>(&["--concurrency"]).unwrap_err();
    assert!(err.to_string().contains("requires a value"));

    let err = figue::from_slice::<RequiredValueArgs>(&["-j"]).unwrap_err();
    assert!(err.to_string().contains("requires a value"));
}

#[derive(Facet, Debug, PartialEq)]
struct OptionalBoolArgs {
    #[facet(args::named)]
    flag: Option<Option<bool>>,
}

#[test]
fn nested_optional_bool_stays_bool_flag() {
    let args: OptionalBoolArgs = figue::from_slice::<OptionalBoolArgs>(&[]).unwrap();
    assert_eq!(args.flag, None);

    let args: OptionalBoolArgs = figue::from_slice(&["--flag"]).unwrap();
    assert_eq!(args.flag, Some(Some(true)));

    let args: OptionalBoolArgs = figue::from_slice(&["--no-flag"]).unwrap();
    assert_eq!(args.flag, Some(Some(false)));
}

#[derive(Facet, Debug, PartialEq)]
struct SurfaceArgs {
    /// Set parallelism
    #[facet(args::named)]
    parallel: Option<Option<usize>>,

    /// Set jobs
    #[facet(args::named)]
    jobs: Option<usize>,
}

#[test]
fn optional_value_help_and_completions_render_optional_placeholder() {
    let help = figue::generate_help::<SurfaceArgs>(&figue::HelpConfig {
        program_name: Some("app".to_string()),
        ..Default::default()
    });
    assert!(
        help.contains("--parallel[=<USIZE>]"),
        "help should show optional value placeholder:\n{help}"
    );
    assert!(
        help.contains("--jobs <USIZE>"),
        "ordinary option should keep required value placeholder:\n{help}"
    );

    let zsh = figue::generate_completions_for_shape(SurfaceArgs::SHAPE, figue::Shell::Zsh, "app");
    assert!(zsh.contains("--parallel[Set parallelism]::value:_default"));
    assert!(zsh.contains("--jobs[Set jobs]:value:_default"));

    let bash = figue::generate_completions_for_shape(SurfaceArgs::SHAPE, figue::Shell::Bash, "app");
    assert!(bash.contains("--parallel"));
    assert!(!bash.contains("--parallel)\n            # Value expected"));
}


