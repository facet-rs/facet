//! xtask: Development tasks for roam
//!
//! Run with: `cargo xtask <command>`

use std::process::ExitCode;

use facet::Facet;
use figue as args;
use xshell::{Shell, cmd};

/// Development tasks for roam
#[derive(Facet)]
struct Cli {
    #[facet(args::subcommand)]
    command: Commands,
}

#[derive(Facet)]
#[repr(u8)]
enum Commands {
    /// Run all CI checks locally (test, clippy, fmt, doc, coverage, miri)
    Ci,
    /// Run all tests (workspace)
    Test,
    /// Run clippy on all code
    Clippy,
    /// Check formatting
    Fmt {
        /// Fix formatting issues instead of just checking
        #[facet(args::named, default)]
        fix: bool,
    },
    /// Build documentation with warnings as errors
    Doc,
    /// Generate code coverage report (requires cargo-llvm-cov)
    Coverage,
    /// Run miri for undefined behavior detection (requires nightly)
    Miri,
    /// Generate spec/spec-tests/tests/spec_matrix.rs from the combo definition
    GenerateSpecMatrix,
    /// Generate language bindings from the canonical spec-proto crate
    Codegen {
        /// Generate TypeScript bindings into `typescript/generated/`
        #[facet(args::named, default)]
        typescript: bool,
        /// Generate Swift bindings into `swift/generated/`
        #[facet(args::named, default)]
        swift: bool,
        /// Generate Swift client-only bindings
        #[facet(args::named, default)]
        swift_client: bool,
        /// Generate Swift server-only bindings
        #[facet(args::named, default)]
        swift_server: bool,
        /// Generate Swift wire protocol types (WireV7.swift)
        #[facet(args::named, default)]
        swift_wire: bool,
    },
}

fn main() -> ExitCode {
    if let Err(e) = run() {
        eprintln!("Error: {e}");
        ExitCode::FAILURE
    } else {
        ExitCode::SUCCESS
    }
}

fn run() -> Result<(), Box<dyn std::error::Error>> {
    let cli: Cli = args::from_std_args().unwrap();
    let sh = Shell::new()?;

    // Find workspace root (where Cargo.toml with [workspace] lives)
    let workspace_root = std::env::var("CARGO_MANIFEST_DIR")
        .map(std::path::PathBuf::from)
        .unwrap_or_else(|_| std::env::current_dir().unwrap())
        .parent()
        .unwrap()
        .to_path_buf();
    sh.change_dir(&workspace_root);

    match cli.command {
        Commands::Test => {
            println!("\n=== Running workspace tests ===");

            // Try nextest first, fall back to cargo test
            if cmd!(sh, "cargo nextest --version").quiet().run().is_ok() {
                println!("Using cargo-nextest");
                // Use CI profile for longer timeouts when in CI
                if std::env::var("CI").is_ok() {
                    cmd!(sh, "cargo nextest run --workspace --profile ci").run()?;
                } else {
                    cmd!(sh, "cargo nextest run --workspace").run()?;
                }
            } else {
                println!("cargo-nextest not found, using cargo test");
                cmd!(sh, "cargo test --workspace").run()?;
            }

            println!("\n=== All tests passed ===");
        }
        Commands::Clippy => {
            println!("=== Running clippy ===");
            // Exclude wasm-browser-tests which only compiles for wasm32
            cmd!(
                sh,
                "cargo clippy --workspace --all-targets --exclude wasm-browser-tests -- -D warnings"
            )
            .run()?;
        }
        Commands::Fmt { fix } => {
            if fix {
                println!("=== Fixing formatting ===");
                cmd!(sh, "cargo fmt --all").run()?;
            } else {
                println!("=== Checking formatting ===");
                cmd!(sh, "cargo fmt --all -- --check").run()?;
            }
        }
        Commands::Ci => {
            println!("=== Running all CI checks ===\n");

            println!(">>> cargo xtask test");
            cmd!(sh, "cargo xtask test").run()?;

            println!("\n>>> cargo xtask clippy");
            cmd!(sh, "cargo xtask clippy").run()?;

            println!("\n>>> cargo xtask fmt");
            cmd!(sh, "cargo xtask fmt").run()?;

            println!("\n>>> cargo xtask doc");
            cmd!(sh, "cargo xtask doc").run()?;

            println!("\n>>> cargo xtask coverage");
            cmd!(sh, "cargo xtask coverage").run()?;

            println!("\n>>> cargo xtask miri");
            cmd!(sh, "cargo xtask miri").run()?;

            println!("\n=== All CI checks passed ===");
        }
        Commands::Doc => {
            println!("=== Building documentation with warnings as errors ===");
            // Build docs for the default workspace members (rust/* crates).
            cmd!(sh, "cargo doc --no-deps")
                .env("RUSTDOCFLAGS", "-D warnings")
                .run()?;
            println!("\n=== Documentation built successfully ===");
        }
        Commands::Coverage => {
            println!("=== Generating code coverage report ===");

            // Check if cargo-llvm-cov is installed
            if cmd!(sh, "cargo llvm-cov --version").quiet().run().is_err() {
                eprintln!("cargo-llvm-cov not found. Install with:");
                eprintln!("  cargo install cargo-llvm-cov");
                return Err("cargo-llvm-cov not installed".into());
            }

            cmd!(sh, "cargo llvm-cov nextest --lcov --output-path lcov.info").run()?;

            println!("\n=== Code coverage report generated: lcov.info ===");
        }
        Commands::Miri => {
            println!("=== Running Miri (undefined behavior detection) ===");

            // Check if miri is available (requires nightly)
            if cmd!(sh, "cargo +nightly miri --version")
                .quiet()
                .run()
                .is_err()
            {
                eprintln!("cargo-miri not found. Install with:");
                eprintln!("  rustup +nightly component add miri");
                return Err("cargo-miri not installed".into());
            }

            println!("\n=== Setting up Miri ===");
            cmd!(sh, "cargo +nightly miri setup").run()?;

            println!("\n=== Running Miri tests ===");
            let result = cmd!(sh, "cargo +nightly miri test").run();

            // Miri may fail on some systems due to unsupported libc calls,
            // but we still want to report the result
            match result {
                Ok(()) => println!("\n=== Miri tests passed ==="),
                Err(e) => {
                    eprintln!("\nMiri tests had issues (this may be expected on some systems):");
                    eprintln!("  {}", e);
                    eprintln!("Note: Some tests may be skipped due to Miri limitations");
                }
            }
        }
        Commands::GenerateSpecMatrix => {
            generate_spec_matrix(&workspace_root)?;
        }
        Commands::Codegen {
            typescript,
            swift,
            swift_client,
            swift_server,
            swift_wire,
        } => {
            if typescript {
                codegen_typescript(&workspace_root)?;
            }
            if swift || swift_client || swift_server {
                codegen_swift(&workspace_root, swift, swift_client, swift_server)?;
            }
            if swift_wire {
                codegen_swift_wire(&workspace_root)?;
            }
        }
    }

    Ok(())
}

fn fmt_typescript(path: &std::path::Path, text: String) -> String {
    use dprint_plugin_typescript::configuration::ConfigurationBuilder;
    use dprint_plugin_typescript::{FormatTextOptions, format_text};
    let config = ConfigurationBuilder::new().build();
    match format_text(FormatTextOptions {
        path,
        extension: None,
        text: text.clone(),
        config: &config,
        external_formatter: None,
    }) {
        Ok(Some(formatted)) => formatted,
        Ok(None) => text,
        Err(e) => {
            eprintln!("warning: dprint failed to format {}: {e}", path.display());
            text
        }
    }
}

fn fmt_swift(path: &std::path::Path, text: String) -> String {
    fn try_swift_formatter(
        path: &std::path::Path,
        text: &str,
        program: &str,
        args: &[&str],
    ) -> Result<Option<String>, Box<dyn std::error::Error>> {
        use std::io::{ErrorKind, Write};
        use std::process::{Command, Stdio};

        let mut child = match Command::new(program)
            .args(args)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
        {
            Ok(child) => child,
            Err(e) if e.kind() == ErrorKind::NotFound => return Ok(None),
            Err(e) => {
                return Err(format!(
                    "failed to start {program} while formatting {}: {e}",
                    path.display()
                )
                .into());
            }
        };

        let mut stdin = child
            .stdin
            .take()
            .ok_or_else(|| format!("failed to open stdin for {program}"))?;
        stdin.write_all(text.as_bytes())?;
        drop(stdin);

        let output = child.wait_with_output()?;
        if output.status.success() {
            let stdout = String::from_utf8(output.stdout)?;
            return Ok(Some(stdout));
        }

        let stderr = String::from_utf8_lossy(&output.stderr);
        Err(format!(
            "{program} {} failed while formatting {}: {}",
            args.join(" "),
            path.display(),
            stderr.trim()
        )
        .into())
    }

    match try_swift_formatter(path, &text, "swift-format", &["format", "-"]) {
        Ok(Some(formatted)) => formatted,
        Ok(None) => match try_swift_formatter(path, &text, "swift", &["format", "-"]) {
            Ok(Some(formatted)) => formatted,
            Ok(None) => {
                eprintln!(
                    "warning: neither swift-format nor `swift format` found, leaving {} unformatted",
                    path.display()
                );
                text
            }
            Err(e) => {
                eprintln!("warning: swift format failed for {}: {e}", path.display());
                text
            }
        },
        Err(e) => {
            eprintln!("warning: swift-format failed for {}: {e}", path.display());
            text
        }
    }
}

fn codegen_typescript(workspace_root: &std::path::Path) -> Result<(), Box<dyn std::error::Error>> {
    let out_dir = workspace_root.join("typescript").join("generated");
    std::fs::create_dir_all(&out_dir)?;

    // Generate TypeScript for all services in spec-proto
    for service in spec_proto::all_services() {
        let ts = roam_codegen::targets::typescript::generate_service(service);
        let filename = format!("{}.ts", service.service_name.to_lowercase());
        let out_path = out_dir.join(&filename);
        write_if_changed(&out_path, fmt_typescript(&out_path, ts))?;
    }

    codegen_typescript_wire_schemas(workspace_root)?;

    Ok(())
}

fn codegen_typescript_wire_schemas(
    workspace_root: &std::path::Path,
) -> Result<(), Box<dyn std::error::Error>> {
    use roam_codegen::targets::typescript::schema::generate_schema;
    use roam_types as rt;

    let out_path = workspace_root
        .join("typescript")
        .join("packages")
        .join("roam-wire")
        .join("src")
        .join("schemas.generated.ts");

    let mut out = String::new();
    out.push_str("// @generated by cargo xtask codegen --typescript\n");
    out.push_str("// DO NOT EDIT - schemas are generated from rust/roam-types facet shapes.\n\n");
    out.push_str("import type { Schema, SchemaRegistry } from \"@bearcove/roam-postcard\";\n\n");

    macro_rules! emit_schema {
        ($name:literal, $shape:expr) => {{
            let schema = generate_schema($shape);
            out.push_str(&format!("export const {}: Schema = {};\n\n", $name, schema));
        }};
    }

    emit_schema!("ParitySchema", <rt::Parity as facet::Facet<'static>>::SHAPE);
    emit_schema!(
        "ConnectionSettingsSchema",
        <rt::ConnectionSettings as facet::Facet<'static>>::SHAPE
    );
    emit_schema!(
        "MetadataValueSchema",
        <rt::MetadataValue<'static> as facet::Facet<'static>>::SHAPE
    );
    emit_schema!(
        "MetadataEntrySchema",
        <rt::MetadataEntry<'static> as facet::Facet<'static>>::SHAPE
    );
    emit_schema!(
        "HelloSchema",
        <rt::Hello<'static> as facet::Facet<'static>>::SHAPE
    );
    emit_schema!(
        "HelloYourselfSchema",
        <rt::HelloYourself<'static> as facet::Facet<'static>>::SHAPE
    );
    emit_schema!(
        "ProtocolErrorSchema",
        <rt::ProtocolError<'static> as facet::Facet<'static>>::SHAPE
    );
    emit_schema!("PingSchema", <rt::Ping as facet::Facet<'static>>::SHAPE);
    emit_schema!("PongSchema", <rt::Pong as facet::Facet<'static>>::SHAPE);
    emit_schema!(
        "ConnectionOpenSchema",
        <rt::ConnectionOpen<'static> as facet::Facet<'static>>::SHAPE
    );
    emit_schema!(
        "ConnectionAcceptSchema",
        <rt::ConnectionAccept<'static> as facet::Facet<'static>>::SHAPE
    );
    emit_schema!(
        "ConnectionRejectSchema",
        <rt::ConnectionReject<'static> as facet::Facet<'static>>::SHAPE
    );
    emit_schema!(
        "ConnectionCloseSchema",
        <rt::ConnectionClose<'static> as facet::Facet<'static>>::SHAPE
    );
    emit_schema!(
        "RequestBodySchema",
        <rt::RequestBody<'static> as facet::Facet<'static>>::SHAPE
    );
    emit_schema!(
        "RequestMessageSchema",
        <rt::RequestMessage<'static> as facet::Facet<'static>>::SHAPE
    );
    emit_schema!(
        "ChannelBodySchema",
        <rt::ChannelBody<'static> as facet::Facet<'static>>::SHAPE
    );
    emit_schema!(
        "ChannelMessageSchema",
        <rt::ChannelMessage<'static> as facet::Facet<'static>>::SHAPE
    );
    emit_schema!(
        "MessagePayloadSchema",
        <rt::MessagePayload<'static> as facet::Facet<'static>>::SHAPE
    );
    emit_schema!(
        "MessageSchema",
        <rt::Message<'static> as facet::Facet<'static>>::SHAPE
    );

    out.push_str("export const wireSchemaRegistry: SchemaRegistry = new Map<string, Schema>([\n");
    out.push_str("  [\"Parity\", ParitySchema],\n");
    out.push_str("  [\"ConnectionSettings\", ConnectionSettingsSchema],\n");
    out.push_str("  [\"MetadataValue\", MetadataValueSchema],\n");
    out.push_str("  [\"MetadataEntry\", MetadataEntrySchema],\n");
    out.push_str("  [\"Hello\", HelloSchema],\n");
    out.push_str("  [\"HelloYourself\", HelloYourselfSchema],\n");
    out.push_str("  [\"ProtocolError\", ProtocolErrorSchema],\n");
    out.push_str("  [\"Ping\", PingSchema],\n");
    out.push_str("  [\"Pong\", PongSchema],\n");
    out.push_str("  [\"ConnectionOpen\", ConnectionOpenSchema],\n");
    out.push_str("  [\"ConnectionAccept\", ConnectionAcceptSchema],\n");
    out.push_str("  [\"ConnectionReject\", ConnectionRejectSchema],\n");
    out.push_str("  [\"ConnectionClose\", ConnectionCloseSchema],\n");
    out.push_str("  [\"RequestBody\", RequestBodySchema],\n");
    out.push_str("  [\"RequestMessage\", RequestMessageSchema],\n");
    out.push_str("  [\"ChannelBody\", ChannelBodySchema],\n");
    out.push_str("  [\"ChannelMessage\", ChannelMessageSchema],\n");
    out.push_str("  [\"MessagePayload\", MessagePayloadSchema],\n");
    out.push_str("  [\"Message\", MessageSchema],\n");
    out.push_str("]);\n");

    write_if_changed(&out_path, fmt_typescript(&out_path, out))?;
    Ok(())
}

fn codegen_swift(
    workspace_root: &std::path::Path,
    swift: bool,
    swift_client: bool,
    swift_server: bool,
) -> Result<(), Box<dyn std::error::Error>> {
    // Output directly to subject sources
    let out_dir = workspace_root
        .join("swift")
        .join("subject")
        .join("Sources")
        .join("subject-swift");
    std::fs::create_dir_all(&out_dir)?;

    let testbed = spec_proto::testbed_service_descriptor();
    if swift && !swift_client && !swift_server {
        let code = roam_codegen::targets::swift::generate_service(testbed);
        let out_path = out_dir.join("Testbed.swift");
        write_if_changed(&out_path, fmt_swift(&out_path, code))?;
        return Ok(());
    }

    if swift_client || (swift && !swift_server) {
        let code = roam_codegen::targets::swift::generate_service_with_bindings(
            testbed,
            roam_codegen::targets::swift::SwiftBindings::Client,
        );
        let out_path = out_dir.join("TestbedClient.swift");
        write_if_changed(&out_path, fmt_swift(&out_path, code))?;
    }

    if swift_server || (swift && !swift_client) {
        let code = roam_codegen::targets::swift::generate_service_with_bindings(
            testbed,
            roam_codegen::targets::swift::SwiftBindings::Server,
        );
        let out_path = out_dir.join("TestbedServer.swift");
        write_if_changed(&out_path, fmt_swift(&out_path, code))?;
    }

    Ok(())
}

fn codegen_swift_wire(workspace_root: &std::path::Path) -> Result<(), Box<dyn std::error::Error>> {
    use roam_codegen::targets::swift::wire::{WireType, generate_wire_types};
    use roam_types as rt;

    let out_path = workspace_root
        .join("swift")
        .join("roam-runtime")
        .join("Sources")
        .join("RoamRuntime")
        .join("WireV7.swift");

    macro_rules! wire_type {
        ($swift_name:literal, $ty:ty) => {
            WireType {
                swift_name: $swift_name.to_string(),
                shape: <$ty as facet::Facet<'static>>::SHAPE,
            }
        };
    }

    let types = vec![
        wire_type!("ParityV7", rt::Parity),
        wire_type!("ConnectionSettingsV7", rt::ConnectionSettings),
        wire_type!("MetadataValueV7", rt::MetadataValue<'static>),
        wire_type!("MetadataEntryV7", rt::MetadataEntry<'static>),
        wire_type!("HelloV7", rt::Hello<'static>),
        wire_type!("HelloYourselfV7", rt::HelloYourself<'static>),
        wire_type!("ProtocolErrorV7", rt::ProtocolError<'static>),
        wire_type!("PingV7", rt::Ping),
        wire_type!("PongV7", rt::Pong),
        wire_type!("ConnectionOpenV7", rt::ConnectionOpen<'static>),
        wire_type!("ConnectionAcceptV7", rt::ConnectionAccept<'static>),
        wire_type!("ConnectionRejectV7", rt::ConnectionReject<'static>),
        wire_type!("ConnectionCloseV7", rt::ConnectionClose<'static>),
        wire_type!("RequestCallV7", rt::RequestCall<'static>),
        wire_type!("RequestResponseV7", rt::RequestResponse<'static>),
        wire_type!("RequestCancelV7", rt::RequestCancel<'static>),
        wire_type!("RequestBodyV7", rt::RequestBody<'static>),
        wire_type!("RequestMessageV7", rt::RequestMessage<'static>),
        wire_type!("ChannelItemV7", rt::ChannelItem<'static>),
        wire_type!("ChannelCloseV7", rt::ChannelClose<'static>),
        wire_type!("ChannelResetV7", rt::ChannelReset<'static>),
        wire_type!("ChannelGrantCreditV7", rt::ChannelGrantCredit),
        wire_type!("ChannelBodyV7", rt::ChannelBody<'static>),
        wire_type!("ChannelMessageV7", rt::ChannelMessage<'static>),
        wire_type!("MessagePayloadV7", rt::MessagePayload<'static>),
        wire_type!("MessageV7", rt::Message<'static>),
    ];

    let code = generate_wire_types(&types);
    write_if_changed(&out_path, fmt_swift(&out_path, code))?;
    Ok(())
}

fn generate_spec_matrix(
    workspace_root: &std::path::Path,
) -> Result<(), Box<dyn std::error::Error>> {
    use proc_macro2::{Ident, Span, TokenStream};
    use quote::quote;

    struct Combo {
        mod_name: &'static str,
        spec_const: &'static str,
        shm: bool,
        ignore: bool,
    }

    struct TestCase {
        name: &'static str,
        call: &'static str,
    }

    let combos = [
        Combo {
            mod_name: "lang_rust_transport_tcp",
            spec_const: "SUBJECT_RUST_TCP",
            shm: false,
            ignore: false,
        },
        Combo {
            mod_name: "lang_rust_transport_shm_guest_mode",
            spec_const: "SUBJECT_RUST_SHM_GUEST",
            shm: true,
            ignore: false,
        },
        Combo {
            mod_name: "lang_typescript_transport_tcp",
            spec_const: "SUBJECT_TYPESCRIPT_TCP",
            shm: false,
            ignore: true,
        },
        Combo {
            mod_name: "lang_swift_transport_tcp",
            spec_const: "SUBJECT_SWIFT_TCP",
            shm: false,
            ignore: true,
        },
        Combo {
            mod_name: "lang_swift_transport_shm_guest_mode",
            spec_const: "SUBJECT_SWIFT_SHM_GUEST",
            shm: true,
            ignore: true,
        },
        Combo {
            mod_name: "lang_swift_transport_shm_host_mode",
            spec_const: "SUBJECT_SWIFT_SHM_HOST",
            shm: true,
            ignore: true,
        },
    ];

    let harness_to_subject = [
        TestCase {
            name: "rpc_echo_roundtrip",
            call: "testbed::run_rpc_echo_roundtrip",
        },
        TestCase {
            name: "rpc_user_error_roundtrip",
            call: "testbed::run_rpc_user_error_roundtrip",
        },
        TestCase {
            name: "rpc_pipelining_multiple_requests",
            call: "testbed::run_rpc_pipelining_multiple_requests",
        },
        TestCase {
            name: "channeling_generate_server_to_client",
            call: "channeling::run_channeling_generate_server_to_client",
        },
        TestCase {
            name: "binary_payload_sizes",
            call: "binary_payloads::run_subject_process_message_binary_payload_sizes",
        },
    ];
    let subject_to_harness = [TestCase {
        name: "channeling_sum_client_to_server",
        call: "channeling::run_channeling_sum_client_to_server",
    }];
    let bidirectional = [TestCase {
        name: "channeling_transform",
        call: "channeling::run_channeling_transform_bidirectional",
    }];
    let shm_harness_to_subject = [TestCase {
        name: "binary_payload_cutover_boundaries",
        call: "binary_payloads::run_subject_process_message_binary_payload_shm_cutover_boundaries",
    }];

    let gen_mod = |mod_name: &str, cases: &[TestCase], ignore: bool| -> TokenStream {
        let mod_ident = Ident::new(mod_name, Span::call_site());
        let fns: Vec<TokenStream> = cases
            .iter()
            .map(|t| {
                let fn_ident = Ident::new(t.name, Span::call_site());
                let call: TokenStream = t.call.parse().unwrap();
                let ignore_attr = if ignore {
                    quote! { #[ignore] }
                } else {
                    quote! {}
                };
                quote! {
                    #ignore_attr
                    #[test]
                    fn #fn_ident() { #call(SPEC); }
                }
            })
            .collect();
        quote! {
            mod #mod_ident {
                use super::*;
                #(#fns)*
            }
        }
    };

    let combo_mods: Vec<TokenStream> = combos
        .iter()
        .map(|c| {
            let mod_ident = Ident::new(c.mod_name, Span::call_site());
            let spec: TokenStream = c.spec_const.parse().unwrap();
            let h2s = gen_mod(
                "direction_harness_to_subject",
                &harness_to_subject,
                c.ignore,
            );
            let s2h = gen_mod(
                "direction_subject_to_harness",
                &subject_to_harness,
                c.ignore,
            );
            let bidi = gen_mod("direction_bidirectional", &bidirectional, c.ignore);
            let shm_mod = if c.shm {
                gen_mod(
                    "direction_harness_to_subject_shm_only",
                    &shm_harness_to_subject,
                    c.ignore,
                )
            } else {
                quote! {}
            };
            quote! {
                mod #mod_ident {
                    use super::*;
                    const SPEC: SubjectSpec = #spec;
                    #h2s
                    #s2h
                    #bidi
                    #shm_mod
                }
            }
        })
        .collect();

    let tokens = quote! {
        #[path = "cases/binary_payload_transport_matrix.rs"]
        mod binary_payload_transport_matrix;
        #[path = "cases/binary_payloads.rs"]
        mod binary_payloads;
        #[path = "cases/channeling.rs"]
        mod channeling;
        #[path = "cases/testbed.rs"]
        mod testbed;

        #[cfg(all(unix, target_os = "macos"))]
        #[path = "cases/cross_language_shm_guest_matrix.rs"]
        mod cross_language_shm_guest_matrix;

        use spec_tests::harness::{SubjectLanguage, SubjectSpec};

        const SUBJECT_RUST_TCP: SubjectSpec = SubjectSpec::tcp(SubjectLanguage::Rust);
        const SUBJECT_RUST_SHM_GUEST: SubjectSpec = SubjectSpec::shm_guest(SubjectLanguage::Rust);
        const SUBJECT_TYPESCRIPT_TCP: SubjectSpec = SubjectSpec::tcp(SubjectLanguage::TypeScript);
        const SUBJECT_SWIFT_TCP: SubjectSpec = SubjectSpec::tcp(SubjectLanguage::Swift);
        const SUBJECT_SWIFT_SHM_GUEST: SubjectSpec = SubjectSpec::shm_guest(SubjectLanguage::Swift);
        const SUBJECT_SWIFT_SHM_HOST: SubjectSpec = SubjectSpec::shm_host(SubjectLanguage::Swift);

        #(#combo_mods)*

        #[test]
        fn lang_rust_to_rust_transport_mem_direction_bidirectional_binary_payload_transport_matrix() {
            binary_payload_transport_matrix::run_rust_binary_payload_transport_matrix_mem();
        }
        #[test]
        fn lang_rust_to_rust_transport_tcp_direction_bidirectional_binary_payload_transport_matrix() {
            binary_payload_transport_matrix::run_rust_binary_payload_transport_matrix_subject_tcp(SUBJECT_RUST_TCP);
        }
        #[test]
        fn lang_rust_to_rust_transport_shm_direction_bidirectional_binary_payload_transport_matrix() {
            binary_payload_transport_matrix::run_rust_binary_payload_transport_matrix_subject_shm(SUBJECT_RUST_SHM_GUEST);
        }
        #[cfg(all(unix, target_os = "macos"))]
        #[test]
        fn lang_swift_to_rust_transport_shm_direction_guest_to_host_cross_language_data_path() {
            cross_language_shm_guest_matrix::run_data_path_case();
        }
        #[cfg(all(unix, target_os = "macos"))]
        #[test]
        fn lang_swift_to_rust_transport_shm_direction_guest_to_host_cross_language_message_v7() {
            cross_language_shm_guest_matrix::run_message_v7_case();
        }
        #[cfg(all(unix, target_os = "macos"))]
        #[test]
        fn lang_rust_to_swift_transport_shm_direction_host_to_guest_cross_language_mmap_ref_receive() {
            cross_language_shm_guest_matrix::run_mmap_ref_receive_case();
        }
        #[cfg(all(unix, target_os = "macos"))]
        #[test]
        fn lang_rust_to_swift_transport_shm_direction_host_to_guest_cross_language_cutover_boundaries() {
            cross_language_shm_guest_matrix::run_boundary_cutover_rust_to_swift_case();
        }
        #[cfg(all(unix, target_os = "macos"))]
        #[test]
        fn lang_swift_to_rust_transport_shm_direction_guest_to_host_cross_language_cutover_boundaries() {
            cross_language_shm_guest_matrix::run_boundary_cutover_swift_to_rust_case();
        }
        #[cfg(all(unix, target_os = "macos"))]
        #[test]
        fn lang_swift_to_rust_transport_shm_direction_guest_to_host_cross_language_fault_mmap_control_breakage() {
            cross_language_shm_guest_matrix::run_fault_mmap_control_breakage_case();
        }
        #[cfg(all(unix, target_os = "macos"))]
        #[test]
        fn lang_rust_to_swift_transport_shm_direction_host_to_guest_cross_language_fault_host_goodbye_wake() {
            cross_language_shm_guest_matrix::run_fault_host_goodbye_wake_case();
        }
    };

    let file: syn::File = syn::parse2(tokens)?;
    let mut out =
        String::from("// @generated by cargo xtask generate-spec-matrix\n// DO NOT EDIT\n\n");
    out.push_str(&prettyplease::unparse(&file));

    let out_path = workspace_root
        .join("spec")
        .join("spec-tests")
        .join("tests")
        .join("spec_matrix.rs");
    write_if_changed(&out_path, out)?;
    Ok(())
}

/// Write `contents` to `path` only if the file doesn't already have those exact bytes.
/// This preserves mtime when nothing changed, preventing unnecessary rebuilds in
/// timestamp-based build systems (Swift Package Manager, make, etc.).
fn write_if_changed(
    path: &std::path::Path,
    contents: impl AsRef<[u8]>,
) -> Result<(), Box<dyn std::error::Error>> {
    let contents = contents.as_ref();
    if std::fs::read(path).ok().as_deref() == Some(contents) {
        println!("Unchanged {}", path.display());
        return Ok(());
    }
    std::fs::write(path, contents)?;
    println!("Wrote {}", path.display());
    Ok(())
}

/// oha JSON output format (partial - just what we need)
#[derive(facet::Facet)]
#[facet(rename_all = "camelCase")]
struct OhaResult {
    summary: OhaSummary,
    latency_percentiles: OhaLatencyPercentiles,
}

#[derive(facet::Facet)]
#[facet(rename_all = "camelCase")]
struct OhaSummary {
    requests_per_sec: f64,
}

#[derive(facet::Facet)]
struct OhaLatencyPercentiles {
    p50: Option<f64>,
    p90: Option<f64>,
    p99: Option<f64>,
}

/// Benchmark result for a single run
#[allow(dead_code)]
struct BenchResult {
    name: String,
    endpoint: String,
    concurrency: u32,
    rps: f64,
    p50_ms: f64,
    p90_ms: f64,
    p99_ms: f64,
}
