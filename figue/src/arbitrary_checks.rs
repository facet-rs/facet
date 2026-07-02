#![cfg(feature = "arbitrary")]

use std::collections::BTreeMap;
use std::ffi::OsString;

use arbitrary::Arbitrary;
use facet_core::Facet;
use heck::ToKebabCase;
use rand::{RngCore, SeedableRng};
use rand_chacha::ChaCha8Rng;

use crate::ToArgs;
use crate::config_value_parser::from_config_value;
use crate::layers::cli::{CliConfigBuilder, parse_cli};
use crate::schema::{ArgKind, ArgLevelSchema, ArgSchema, Schema};

/// Error returned by arbitrary-based CLI roundtrip checks.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ArbitraryCheckError {
    /// Number of successful arbitrary samples processed.
    pub successful_samples: usize,
    /// Number of total attempts made.
    pub attempts: usize,
    /// Human-readable failure description.
    pub message: String,
}

impl core::fmt::Display for ArbitraryCheckError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        write!(
            f,
            "{message} (successful_samples={successful}, attempts={attempts})",
            message = self.message,
            successful = self.successful_samples,
            attempts = self.attempts
        )
    }
}

impl std::error::Error for ArbitraryCheckError {}

/// Configuration for `to_args()` consistency checks.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TestToArgsConsistencyConfig {
    /// Number of successful arbitrary samples required.
    pub success_count: usize,
    /// Maximum number of attempts before failing the test.
    pub max_attempts: usize,
    /// Size of the random byte buffer used to seed arbitrary generation.
    pub random_data_len: usize,
    /// Number of samples worth of random bytes to generate per refill.
    pub prefill_sample_count: usize,
    /// Root seed for the deterministic RNG used during this run.
    ///
    /// When `None`, a fresh seed is chosen at the start of the check so
    /// repeated test runs explore different random streams.
    pub root_seed: Option<u64>,
}

impl Default for TestToArgsConsistencyConfig {
    fn default() -> Self {
        Self {
            success_count: 500,
            max_attempts: 10_000,
            random_data_len: 1024,
            prefill_sample_count: 256,
            root_seed: None,
        }
    }
}

/// Configuration for `to_args()` roundtrip checks.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TestToArgsRoundTrip {
    /// Number of successful arbitrary samples required per command leaf.
    pub success_count_per_leaf: usize,
    /// Number of successful arbitrary samples required for CLIs without subcommands.
    pub success_count_global: usize,
    /// Maximum number of attempts allowed per command leaf.
    pub max_attempts_per_leaf: usize,
    /// Maximum number of attempts allowed for CLIs without subcommands.
    pub max_attempts_global: usize,
    /// Size of the random byte buffer used to seed arbitrary generation.
    pub random_data_len: usize,
    /// Number of samples worth of random bytes to generate per refill.
    pub prefill_sample_count: usize,
    /// Root seed for the deterministic RNG used during this run.
    ///
    /// When `None`, a fresh seed is chosen at the start of the check so
    /// repeated test runs explore different random streams.
    pub root_seed: Option<u64>,
}

impl Default for TestToArgsRoundTrip {
    fn default() -> Self {
        Self {
            success_count_per_leaf: 4,
            success_count_global: 4,
            max_attempts_per_leaf: 4 * 4_000,
            max_attempts_global: 4 * 80,
            random_data_len: 1024,
            prefill_sample_count: 256,
            root_seed: None,
        }
    }
}

#[derive(Debug)]
struct EntropyPool {
    sample_len: usize,
    prefill_sample_count: usize,
    root_seed: u64,
    next_offset: usize,
    bytes: Vec<u8>,
    rng: ChaCha8Rng,
}

impl EntropyPool {
    fn new(sample_len: usize, prefill_sample_count: usize, root_seed: Option<u64>) -> Self {
        let sample_len = sample_len.max(1);
        let prefill_sample_count = prefill_sample_count.max(1);
        let root_seed = root_seed.unwrap_or_else(rand::random::<u64>);
        let byte_len = sample_len.saturating_mul(prefill_sample_count);
        let mut pool = Self {
            sample_len,
            prefill_sample_count,
            root_seed,
            next_offset: 0,
            bytes: vec![0u8; byte_len],
            rng: ChaCha8Rng::seed_from_u64(root_seed),
        };
        pool.refill();
        pool
    }

    fn next_sample(&mut self) -> &[u8] {
        if self.next_offset + self.sample_len > self.bytes.len() {
            self.refill();
        }

        let start = self.next_offset;
        let end = start + self.sample_len;
        self.next_offset = end;
        &self.bytes[start..end]
    }

    fn context_suffix(&self) -> String {
        format!(
            "root_seed={} random_data_len={} prefill_sample_count={}",
            self.root_seed, self.sample_len, self.prefill_sample_count
        )
    }

    fn refill(&mut self) {
        self.rng.fill_bytes(&mut self.bytes);
        self.next_offset = 0;
    }
}

/// Assert that `to_args()` is deterministic for arbitrary-generated values.
///
/// This validates that repeated calls for the same value produce identical
/// argument vectors.
pub fn assert_to_args_consistency<T>(
    config: TestToArgsConsistencyConfig,
) -> Result<(), ArbitraryCheckError>
where
    T: Facet<'static> + for<'a> Arbitrary<'a> + core::fmt::Debug,
{
    let mut entropy_pool = EntropyPool::new(
        config.random_data_len,
        config.prefill_sample_count,
        config.root_seed,
    );

    let mut successful_samples = 0usize;
    let mut attempts = 0usize;

    while successful_samples < config.success_count && attempts < config.max_attempts {
        attempts += 1;

        let mut rng = arbitrary::Unstructured::new(entropy_pool.next_sample());
        let Ok(instance) = T::arbitrary(&mut rng) else {
            continue;
        };

        let args1 = instance.to_args().map_err(|error| ArbitraryCheckError {
            successful_samples,
            attempts,
            message: format!("first to_args() call failed: {error}"),
        })?;

        let args2 = instance.to_args().map_err(|error| ArbitraryCheckError {
            successful_samples,
            attempts,
            message: format!("second to_args() call failed: {error}"),
        })?;

        if args1 != args2 {
            return Err(ArbitraryCheckError {
                successful_samples,
                attempts,
                message: format!(
                    "to_args() is non-deterministic for generated value: {instance:?}\nfirst={args1:?}\nsecond={args2:?}\n{}",
                    entropy_pool.context_suffix()
                ),
            });
        }

        successful_samples += 1;
    }

    if successful_samples < config.success_count {
        return Err(ArbitraryCheckError {
            successful_samples,
            attempts,
            message: format!(
                "insufficient arbitrary coverage for consistency test\n{}",
                entropy_pool.context_suffix()
            ),
        });
    }

    Ok(())
}

/// Assert that arbitrary-generated values roundtrip via `to_args()` and figue parsing.
///
/// This validates:
/// 1. value -> `to_args()`
/// 2. args -> parse with `Driver` (strict CLI mode)
/// 3. parsed value equals original value
pub fn assert_to_args_roundtrip<T>(config: TestToArgsRoundTrip) -> Result<(), ArbitraryCheckError>
where
    T: Facet<'static> + for<'a> Arbitrary<'a> + PartialEq + core::fmt::Debug,
{
    let schema = Schema::from_shape(T::SHAPE).map_err(|error| ArbitraryCheckError {
        successful_samples: 0,
        attempts: 0,
        message: format!("failed to build schema: {error}"),
    })?;

    let mut command_tree = command_node_from_arg_level(schema.args());
    let command_paths = collect_command_paths(&mut command_tree);

    if command_paths.is_empty() {
        return assert_to_args_roundtrip_global::<T>(config);
    }

    let mut entropy_pool = EntropyPool::new(
        config.random_data_len,
        config.prefill_sample_count,
        config.root_seed,
    );

    let mut matched_samples_by_path = vec![0usize; command_paths.len()];
    let mut remaining_paths = matched_samples_by_path.len();
    let max_attempts_total = remaining_paths.saturating_mul(config.max_attempts_per_leaf);

    let mut total_successful_samples = 0usize;
    let mut total_attempts = 0usize;

    while remaining_paths > 0 && total_attempts < max_attempts_total {
        total_attempts += 1;

        let mut rng = arbitrary::Unstructured::new(entropy_pool.next_sample());
        let Ok(instance) = T::arbitrary(&mut rng) else {
            continue;
        };

        let args = instance.to_args().map_err(|error| ArbitraryCheckError {
            successful_samples: total_successful_samples,
            attempts: total_attempts,
            message: format!("to_args() failed: {error}"),
        })?;

        let Some(leaf_id) = extract_subcommand_leaf_id_from_args(&args, &command_tree) else {
            continue;
        };

        let matched_samples = &mut matched_samples_by_path[leaf_id];

        if *matched_samples >= config.success_count_per_leaf {
            continue;
        }

        let parsed = parse_from_os_args_with_schema::<T>(&schema, &args).map_err(|message| {
            ArbitraryCheckError {
            successful_samples: total_successful_samples,
            attempts: total_attempts,
            message: format!(
                "failed to parse generated args for path {:?}\nargs={args:?}\nvalue={instance:?}\nerror={message}\n{}",
                command_paths[leaf_id],
                entropy_pool.context_suffix()
            ),
        }
        })?;

        if instance != parsed {
            return Err(ArbitraryCheckError {
                successful_samples: total_successful_samples,
                attempts: total_attempts,
                message: format!(
                    "roundtrip mismatch for path {:?}\noriginal={instance:?}\nparsed={parsed:?}\nargs={args:?}\n{}",
                    command_paths[leaf_id],
                    entropy_pool.context_suffix()
                ),
            });
        }

        *matched_samples += 1;
        total_successful_samples += 1;

        if *matched_samples == config.success_count_per_leaf {
            remaining_paths -= 1;
        }
    }

    if remaining_paths > 0 {
        let missing_paths = command_paths
            .iter()
            .zip(&matched_samples_by_path)
            .filter(|(_, matched_samples)| **matched_samples < config.success_count_per_leaf)
            .map(|(path, matched_samples)| {
                format!(
                    "{path:?}: matched {matched_samples} samples after {total_attempts} total attempts"
                )
            })
            .collect::<Vec<_>>()
            .join(", ");

        return Err(ArbitraryCheckError {
            successful_samples: total_successful_samples,
            attempts: total_attempts,
            message: format!(
                "insufficient coverage for command paths: {missing_paths}\n{}",
                entropy_pool.context_suffix()
            ),
        });
    }

    Ok(())
}

fn assert_to_args_roundtrip_global<T>(
    config: TestToArgsRoundTrip,
) -> Result<(), ArbitraryCheckError>
where
    T: Facet<'static> + for<'a> Arbitrary<'a> + PartialEq + core::fmt::Debug,
{
    let schema = Schema::from_shape(T::SHAPE).map_err(|error| ArbitraryCheckError {
        successful_samples: 0,
        attempts: 0,
        message: format!("failed to build schema: {error}"),
    })?;

    let mut entropy_pool = EntropyPool::new(
        config.random_data_len,
        config.prefill_sample_count,
        config.root_seed,
    );

    let mut successful_samples = 0usize;
    let mut attempts = 0usize;

    while successful_samples < config.success_count_global && attempts < config.max_attempts_global
    {
        attempts += 1;

        let mut rng = arbitrary::Unstructured::new(entropy_pool.next_sample());
        let Ok(instance) = T::arbitrary(&mut rng) else {
            continue;
        };

        let args = instance.to_args().map_err(|error| ArbitraryCheckError {
            successful_samples,
            attempts,
            message: format!("to_args() failed: {error}"),
        })?;

        let parsed = parse_from_os_args_with_schema::<T>(&schema, &args).map_err(|message| {
            ArbitraryCheckError {
            successful_samples,
            attempts,
            message: format!(
                "failed to parse generated args\nargs={args:?}\nvalue={instance:?}\nerror={message}\n{}",
                entropy_pool.context_suffix()
            ),
        }
        })?;

        if instance != parsed {
            return Err(ArbitraryCheckError {
                successful_samples,
                attempts,
                message: format!(
                    "roundtrip mismatch\noriginal={instance:?}\nparsed={parsed:?}\nargs={args:?}\n{}",
                    entropy_pool.context_suffix()
                ),
            });
        }

        successful_samples += 1;
    }

    if successful_samples < config.success_count_global {
        return Err(ArbitraryCheckError {
            successful_samples,
            attempts,
            message: format!(
                "insufficient arbitrary coverage for roundtrip test\n{}",
                entropy_pool.context_suffix()
            ),
        });
    }

    Ok(())
}

#[derive(Clone, Debug)]
struct CommandBranch {
    cli_name: String,
    effective_name: String,
    node: CommandNode,
}

#[derive(Clone, Debug, Default)]
struct CommandNode {
    positional_count: usize,
    leaf_id: Option<usize>,
    named_flag_consumes_value: BTreeMap<String, bool>,
    subcommands: Vec<CommandBranch>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum NamedArgValueMode {
    CountedFlag,
    BoolFlag,
    RequiredValue,
}

fn named_arg_value_mode(schema: &ArgSchema) -> NamedArgValueMode {
    match schema.kind() {
        ArgKind::Named { counted: true, .. } => NamedArgValueMode::CountedFlag,
        ArgKind::Named { counted: false, .. } if schema.value().inner_if_option().is_bool() => {
            NamedArgValueMode::BoolFlag
        }
        ArgKind::Named { counted: false, .. } => NamedArgValueMode::RequiredValue,
        ArgKind::Positional => panic!("named_arg_value_mode called for a positional argument"),
    }
}

fn command_node_from_arg_level(level: &ArgLevelSchema) -> CommandNode {
    let mut node = CommandNode::default();

    for (name, schema) in level.args() {
        match schema.kind() {
            ArgKind::Positional => {
                node.positional_count += 1;
            }
            ArgKind::Named { counted, .. } => {
                let consumes_value = !counted
                    && matches!(named_arg_value_mode(schema), NamedArgValueMode::RequiredValue);
                node.named_flag_consumes_value
                    .insert(name.to_kebab_case(), consumes_value);
            }
        }
    }

    for subcommand in level.subcommands().values() {
        node.subcommands.push(CommandBranch {
            cli_name: subcommand.cli_name().to_string(),
            effective_name: subcommand.effective_name().to_string(),
            node: command_node_from_arg_level(subcommand.args()),
        });
    }

    node
}

fn collect_command_paths(root: &mut CommandNode) -> Vec<Vec<String>> {
    fn visit(node: &mut CommandNode, current: &mut Vec<String>, output: &mut Vec<Vec<String>>) {
        if node.subcommands.is_empty() {
            if !current.is_empty() {
                node.leaf_id = Some(output.len());
                output.push(current.clone());
            }
            return;
        }

        for branch in &mut node.subcommands {
            current.push(branch.effective_name.clone());
            visit(&mut branch.node, current, output);
            let _ = current.pop();
        }
    }

    let mut output = Vec::new();
    let mut current = Vec::new();
    visit(root, &mut current, &mut output);
    output
}

fn extract_subcommand_leaf_id_from_args(args: &[OsString], root: &CommandNode) -> Option<usize> {
    fn walk(node: &CommandNode, tokens: &[OsString], index: &mut usize) -> Option<usize> {
        let mut positionals_seen = 0usize;

        while *index < tokens.len() {
            let token = tokens[*index].to_string_lossy();
            let token = token.as_ref();

            if token.starts_with("--") {
                let flag_name = token.trim_start_matches("--");
                if let Some(consumes_value) = node.named_flag_consumes_value.get(flag_name) {
                    if *consumes_value {
                        *index = (*index + 2).min(tokens.len());
                    } else {
                        *index += 1;
                    }
                } else {
                    *index += 1;
                    if *index < tokens.len() && !tokens[*index].to_string_lossy().starts_with('-') {
                        *index += 1;
                    }
                }
                continue;
            }

            if token.starts_with('-') {
                *index += 1;
                continue;
            }

            if positionals_seen < node.positional_count {
                positionals_seen += 1;
                *index += 1;
                continue;
            }

            if let Some(branch) = node
                .subcommands
                .iter()
                .find(|branch| command_token_matches_cli_name(token, &branch.cli_name))
            {
                *index += 1;
                return walk(&branch.node, tokens, index);
            }

            return node.leaf_id;
        }

        node.leaf_id
    }

    let mut index = 0usize;
    walk(root, args, &mut index)
}

fn command_token_matches_cli_name(token: &str, cli_name: &str) -> bool {
    let mut cli_name_chars = cli_name.chars();

    for token_char in token.chars() {
        let normalized = if token_char == '_' {
            '-'
        } else {
            token_char.to_ascii_lowercase()
        };

        if Some(normalized) != cli_name_chars.next() {
            return false;
        }
    }

    cli_name_chars.next().is_none()
}

fn parse_from_os_args_with_schema<T>(schema: &Schema, args: &[OsString]) -> Result<T, String>
where
    T: Facet<'static>,
{
    let cli_config = CliConfigBuilder::new().args_os(args).strict().build();
    let layer_output = parse_cli(schema, &cli_config);

    if !layer_output.diagnostics.is_empty() {
        let diagnostics = layer_output
            .diagnostics
            .iter()
            .map(|diagnostic| diagnostic.message.as_str())
            .collect::<Vec<_>>()
            .join("; ");
        return Err(format!("cli parse failed: {diagnostics}"));
    }

    let Some(value) = layer_output.value else {
        return Err("cli parse returned no value".to_string());
    };

    from_config_value(&value).map_err(|error| format!("config value parse failed: {error}"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate as args;
    use crate::FigueBuiltins;
    use facet::Facet;
    use std::time::Duration;
    use std::time::Instant;

    #[derive(Facet, arbitrary::Arbitrary, Debug, PartialEq)]
    #[repr(u8)]
    enum Command {
        Build {
            #[facet(args::named)]
            release: bool,
        },
        Clean,
    }

    #[derive(Facet, arbitrary::Arbitrary, Debug, PartialEq)]
    #[repr(u8)]
    enum NestedAction {
        Set {
            #[facet(args::positional)]
            file: String,
        },
        Get,
    }

    #[derive(Facet, arbitrary::Arbitrary, Debug, PartialEq)]
    #[repr(u8)]
    enum NestedCommand {
        Output {
            #[facet(args::subcommand)]
            path: NestedAction,
        },
    }

    #[derive(Facet, arbitrary::Arbitrary, Debug, PartialEq)]
    struct NestedCli {
        #[facet(args::subcommand)]
        command: NestedCommand,
    }

    #[derive(Facet, arbitrary::Arbitrary, Debug, PartialEq)]
    struct Cli {
        #[facet(args::named)]
        verbose: bool,

        #[facet(args::subcommand)]
        command: Command,
    }

    #[derive(Facet, arbitrary::Arbitrary, Debug, Default, PartialEq)]
    #[facet(rename_all = "kebab-case")]
    struct VendoredGlobalArgs {
        #[facet(args::named, default)]
        debug: bool,

        #[facet(args::named)]
        log_filter: Option<String>,

        #[facet(args::named)]
        log_file: Option<String>,
    }

    #[derive(Facet, arbitrary::Arbitrary, Debug)]
    struct VendoredDiscordArchiveCli {
        #[facet(flatten)]
        global_args: VendoredGlobalArgs,

        #[facet(flatten)]
        #[arbitrary(default)]
        builtins: FigueBuiltins,

        #[facet(args::subcommand)]
        command: VendoredCommand,
    }

    impl PartialEq for VendoredDiscordArchiveCli {
        fn eq(&self, other: &Self) -> bool {
            self.global_args == other.global_args && self.command == other.command
        }
    }

    #[derive(Facet, arbitrary::Arbitrary, Debug, PartialEq)]
    #[repr(u8)]
    enum VendoredCommand {
        BotToken(VendoredBotTokenArgs),
        Cache(VendoredCacheArgs),
        Home(VendoredHomeArgs),
        Invite(VendoredInviteArgs),
        Live(VendoredLiveArgs),
        OutputDir(VendoredOutputDirArgs),
        Sync(VendoredSyncArgs),
    }

    #[derive(Facet, arbitrary::Arbitrary, Debug, PartialEq)]
    struct VendoredBotTokenArgs {
        #[facet(args::subcommand)]
        command: VendoredBotTokenCommand,
    }

    #[derive(Facet, arbitrary::Arbitrary, Debug, PartialEq)]
    #[repr(u8)]
    enum VendoredBotTokenCommand {
        Clear(VendoredBotTokenClearArgs),
        Set(VendoredBotTokenSetArgs),
        ShowSource(VendoredBotTokenShowSourceArgs),
        Validate(VendoredBotTokenValidateArgs),
    }

    #[derive(Facet, arbitrary::Arbitrary, Debug, PartialEq)]
    struct VendoredBotTokenClearArgs;

    #[derive(Facet, arbitrary::Arbitrary, Debug, PartialEq)]
    struct VendoredBotTokenSetArgs {
        #[facet(args::positional)]
        token: Option<String>,
    }

    #[derive(Facet, arbitrary::Arbitrary, Debug, PartialEq)]
    struct VendoredBotTokenShowSourceArgs {
        #[facet(args::named)]
        token: Option<String>,
    }

    #[derive(Facet, arbitrary::Arbitrary, Debug, PartialEq)]
    struct VendoredBotTokenValidateArgs {
        #[facet(args::named)]
        token: Option<String>,
    }

    #[derive(Facet, arbitrary::Arbitrary, Debug, PartialEq)]
    struct VendoredCacheArgs {
        #[facet(args::subcommand)]
        command: VendoredCacheCommand,
    }

    #[derive(Facet, arbitrary::Arbitrary, Debug, PartialEq)]
    #[repr(u8)]
    enum VendoredCacheCommand {
        Clean(VendoredCacheCleanArgs),
        Open(VendoredCacheOpenArgs),
        Show(VendoredCacheShowArgs),
    }

    #[derive(Facet, arbitrary::Arbitrary, Debug, PartialEq)]
    struct VendoredCacheCleanArgs;

    #[derive(Facet, arbitrary::Arbitrary, Debug, PartialEq)]
    struct VendoredCacheOpenArgs;

    #[derive(Facet, arbitrary::Arbitrary, Debug, PartialEq)]
    struct VendoredCacheShowArgs;

    #[derive(Facet, arbitrary::Arbitrary, Debug, PartialEq)]
    struct VendoredHomeArgs {
        #[facet(args::subcommand)]
        command: VendoredHomeCommand,
    }

    #[derive(Facet, arbitrary::Arbitrary, Debug, PartialEq)]
    #[repr(u8)]
    enum VendoredHomeCommand {
        Open(VendoredHomeOpenArgs),
        Show(VendoredHomeShowArgs),
    }

    #[derive(Facet, arbitrary::Arbitrary, Debug, PartialEq)]
    struct VendoredHomeOpenArgs;

    #[derive(Facet, arbitrary::Arbitrary, Debug, PartialEq)]
    struct VendoredHomeShowArgs;

    #[derive(Facet, arbitrary::Arbitrary, Debug, PartialEq)]
    #[facet(rename_all = "kebab-case")]
    struct VendoredInviteArgs {
        #[facet(args::named)]
        token: Option<String>,

        #[facet(args::named, default)]
        no_open: bool,
    }

    #[derive(Facet, arbitrary::Arbitrary, Debug, PartialEq)]
    #[facet(rename_all = "kebab-case")]
    struct VendoredLiveArgs {
        #[facet(args::named)]
        token: Option<String>,

        #[facet(args::subcommand)]
        command: VendoredLiveCommand,
    }

    #[derive(Facet, arbitrary::Arbitrary, Debug, PartialEq)]
    #[repr(u8)]
    enum VendoredLiveCommand {
        Attachment(VendoredLiveAttachmentArgs),
        Channel(VendoredLiveChannelArgs),
        Guild(VendoredLiveGuildArgs),
        Message(VendoredLiveMessageArgs),
        Thread(VendoredLiveThreadArgs),
        User(VendoredLiveUserArgs),
    }

    #[derive(Facet, arbitrary::Arbitrary, Debug, PartialEq)]
    struct VendoredLiveAttachmentArgs {
        #[facet(args::subcommand)]
        command: VendoredLiveAttachmentCommand,
    }

    #[derive(Facet, arbitrary::Arbitrary, Debug, PartialEq)]
    #[repr(u8)]
    enum VendoredLiveAttachmentCommand {
        List(VendoredLiveAttachmentListArgs),
    }

    #[derive(Facet, arbitrary::Arbitrary, Debug, PartialEq)]
    #[facet(rename_all = "kebab-case")]
    struct VendoredLiveAttachmentListArgs {
        #[facet(args::named)]
        channel_id: Option<u64>,

        #[facet(args::named)]
        thread_id: Option<u64>,

        #[facet(args::named)]
        before: Option<String>,

        #[facet(args::named)]
        limit: Option<u8>,
    }

    #[derive(Facet, arbitrary::Arbitrary, Debug, PartialEq)]
    struct VendoredLiveChannelArgs {
        #[facet(args::subcommand)]
        command: VendoredLiveChannelCommand,
    }

    #[derive(Facet, arbitrary::Arbitrary, Debug, PartialEq)]
    #[repr(u8)]
    enum VendoredLiveChannelCommand {
        List(VendoredLiveChannelListArgs),
    }

    #[derive(Facet, arbitrary::Arbitrary, Debug, PartialEq)]
    #[facet(rename_all = "kebab-case")]
    struct VendoredLiveChannelListArgs {
        #[facet(args::named)]
        guild_id: u64,
    }

    #[derive(Facet, arbitrary::Arbitrary, Debug, PartialEq)]
    struct VendoredLiveGuildArgs {
        #[facet(args::subcommand)]
        command: VendoredLiveGuildCommand,
    }

    #[derive(Facet, arbitrary::Arbitrary, Debug, PartialEq)]
    #[repr(u8)]
    enum VendoredLiveGuildCommand {
        List(VendoredLiveGuildListArgs),
    }

    #[derive(Facet, arbitrary::Arbitrary, Debug, PartialEq)]
    struct VendoredLiveGuildListArgs;

    #[derive(Facet, arbitrary::Arbitrary, Debug, PartialEq)]
    struct VendoredLiveMessageArgs {
        #[facet(args::subcommand)]
        command: VendoredLiveMessageCommand,
    }

    #[derive(Facet, arbitrary::Arbitrary, Debug, PartialEq)]
    #[repr(u8)]
    enum VendoredLiveMessageCommand {
        List(VendoredLiveMessageListArgs),
    }

    #[derive(Facet, arbitrary::Arbitrary, Debug, PartialEq)]
    #[facet(rename_all = "kebab-case")]
    struct VendoredLiveMessageListArgs {
        #[facet(args::named)]
        channel_id: Option<u64>,

        #[facet(args::named)]
        thread_id: Option<u64>,

        #[facet(args::named)]
        before: Option<String>,

        #[facet(args::named)]
        limit: Option<u8>,
    }

    #[derive(Facet, arbitrary::Arbitrary, Debug, PartialEq)]
    struct VendoredLiveThreadArgs {
        #[facet(args::subcommand)]
        command: VendoredLiveThreadCommand,
    }

    #[derive(Facet, arbitrary::Arbitrary, Debug, PartialEq)]
    #[repr(u8)]
    enum VendoredLiveThreadCommand {
        List(VendoredLiveThreadListArgs),
    }

    #[derive(Facet, arbitrary::Arbitrary, Debug, PartialEq)]
    #[facet(rename_all = "kebab-case")]
    struct VendoredLiveThreadListArgs {
        #[facet(args::named)]
        guild_id: u64,
    }

    #[derive(Facet, arbitrary::Arbitrary, Debug, PartialEq)]
    struct VendoredLiveUserArgs {
        #[facet(args::subcommand)]
        command: VendoredLiveUserCommand,
    }

    #[derive(Facet, arbitrary::Arbitrary, Debug, PartialEq)]
    #[repr(u8)]
    enum VendoredLiveUserCommand {
        List(VendoredLiveUserListArgs),
    }

    #[derive(Facet, arbitrary::Arbitrary, Debug, PartialEq)]
    #[facet(rename_all = "kebab-case")]
    struct VendoredLiveUserListArgs {
        #[facet(args::named)]
        guild_id: u64,

        #[facet(args::named)]
        after_user_id: Option<u64>,

        #[facet(args::named)]
        limit: Option<u64>,
    }

    #[derive(Facet, arbitrary::Arbitrary, Debug, PartialEq)]
    struct VendoredOutputDirArgs {
        #[facet(args::subcommand)]
        command: VendoredOutputDirCommand,
    }

    #[derive(Facet, arbitrary::Arbitrary, Debug, PartialEq)]
    #[repr(u8)]
    enum VendoredOutputDirCommand {
        Open(VendoredOutputDirOpenArgs),
        Set(VendoredOutputDirSetArgs),
        Show(VendoredOutputDirShowArgs),
    }

    #[derive(Facet, arbitrary::Arbitrary, Debug, PartialEq)]
    struct VendoredOutputDirOpenArgs;

    #[derive(Facet, arbitrary::Arbitrary, Debug, PartialEq)]
    struct VendoredOutputDirSetArgs {
        #[facet(args::positional)]
        path: String,
    }

    #[derive(Facet, arbitrary::Arbitrary, Debug, PartialEq)]
    struct VendoredOutputDirShowArgs;

    #[derive(Facet, arbitrary::Arbitrary, Debug, PartialEq)]
    #[facet(rename_all = "kebab-case")]
    struct VendoredSyncArgs {
        #[facet(args::named)]
        output_dir: Option<String>,
    }

    #[test]
    fn arbitrary_consistency_smoke_test() {
        assert_to_args_consistency::<Cli>(TestToArgsConsistencyConfig::default())
            .expect("consistency check should pass");
    }

    #[test]
    fn arbitrary_roundtrip_smoke_test() {
        assert_to_args_roundtrip::<Cli>(TestToArgsRoundTrip::default())
            .expect("roundtrip check should pass");
    }

    #[test]
    fn configurable_roundtrip_stress_test_completes_quickly() {
        let start = Instant::now();
        assert_to_args_roundtrip::<Cli>(TestToArgsRoundTrip::default())
            .expect("roundtrip check should pass");
        assert!(
            start.elapsed() < Duration::from_secs(3),
            "roundtrip stress test took {:?}",
            start.elapsed()
        );
    }

    #[test]
    fn collects_nested_command_paths() {
        let schema = Schema::from_shape(NestedCli::SHAPE).expect("schema should be valid");
        let mut tree = command_node_from_arg_level(schema.args());
        let paths = collect_command_paths(&mut tree);

        assert!(
            paths.contains(&vec!["Output".to_string(), "Set".to_string()]),
            "expected Output -> Set path"
        );
        assert!(
            paths.contains(&vec!["Output".to_string(), "Get".to_string()]),
            "expected Output -> Get path"
        );
    }

    #[test]
    fn vendored_discord_archive_leaf_count_matches_fixture() {
        let schema = Schema::from_shape(VendoredDiscordArchiveCli::SHAPE)
            .expect("vendored schema should be valid");
        let mut tree = command_node_from_arg_level(schema.args());
        let paths = collect_command_paths(&mut tree);

        assert_eq!(paths.len(), 20, "vendored fixture should expose 20 leaves");
    }

    #[test]
    fn vendored_discord_archive_roundtrip_stress_test_completes_quickly() {
        let start = Instant::now();
        assert_to_args_roundtrip::<VendoredDiscordArchiveCli>(TestToArgsRoundTrip::default())
            .expect("vendored discord archive roundtrip should pass");
        assert!(
            start.elapsed() < Duration::from_secs(2),
            "vendored discord archive roundtrip took {:?}",
            start.elapsed()
        );
    }
}


