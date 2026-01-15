//! Styx CLI tool

use std::io::{self, Read};

use facet::Facet;
use facet_args as args;
use styx_tree::{Entry, Payload, Value};

/// Styx command-line tool
#[derive(Facet, Debug)]
struct Cli {
    /// Subcommand to run
    #[facet(args::subcommand)]
    command: Command,
}

/// Available commands
#[derive(Facet, Debug)]
#[repr(u8)]
enum Command {
    /// Parse styx and print the tree structure
    Tree {
        /// Input file (reads from stdin if not provided)
        #[facet(default, args::positional)]
        file: Option<String>,
    },
    /// Parse styx and print canonical form
    Canonicalize {
        /// Input file (reads from stdin if not provided)
        #[facet(default, args::positional)]
        file: Option<String>,
    },
    /// Parse styx and print as JSON
    Json {
        /// Input file (reads from stdin if not provided)
        #[facet(default, args::positional)]
        file: Option<String>,
        /// Pretty-print the JSON output
        #[facet(args::named, args::short = 'p')]
        pretty: bool,
    },
}

fn main() {
    let cli: Cli = match args::from_std_args() {
        Ok(cli) => cli,
        Err(e) => {
            if e.is_help_request() {
                println!("{}", e.help_text().unwrap_or_default());
                return;
            }
            eprintln!("{e}");
            std::process::exit(1);
        }
    };

    if let Err(e) = run(cli) {
        eprintln!("error: {e}");
        std::process::exit(1);
    }
}

fn run(cli: Cli) -> Result<(), Box<dyn std::error::Error>> {
    match cli.command {
        Command::Tree { file } => {
            let source = read_input(file.as_deref())?;
            let value = styx_tree::parse(&source)?;
            print_tree(&value, 0);
        }
        Command::Canonicalize { file } => {
            let source = read_input(file.as_deref())?;
            let value = styx_tree::parse(&source)?;
            print_canonical(&value, 0, true);
            println!();
        }
        Command::Json { file, pretty } => {
            let source = read_input(file.as_deref())?;
            let value = styx_tree::parse(&source)?;
            let json = value_to_json(&value);
            if pretty {
                println!("{}", serde_json::to_string_pretty(&json)?);
            } else {
                println!("{}", serde_json::to_string(&json)?);
            }
        }
    }
    Ok(())
}

fn read_input(file: Option<&str>) -> Result<String, io::Error> {
    match file {
        Some(path) => std::fs::read_to_string(path),
        None => {
            let mut buf = String::new();
            io::stdin().read_to_string(&mut buf)?;
            Ok(buf)
        }
    }
}

/// Print a debug tree representation
fn print_tree(value: &Value, indent: usize) {
    let pad = "  ".repeat(indent);

    // Print tag if present
    if let Some(tag) = &value.tag {
        print!("{pad}Tagged @{}", tag.name);
        match &value.payload {
            None => {
                println!();
            }
            Some(payload) => {
                println!(" {{");
                print_payload(payload, indent + 1);
                println!("{pad}}}");
            }
        }
    } else {
        // No tag
        match &value.payload {
            None => {
                println!("{pad}Unit");
            }
            Some(Payload::Scalar(s)) => {
                println!("{pad}Scalar({:?}, {:?})", s.text, s.kind);
            }
            Some(Payload::Sequence(s)) => {
                println!("{pad}Sequence [");
                for item in &s.items {
                    print_tree(item, indent + 1);
                }
                println!("{pad}]");
            }
            Some(Payload::Object(o)) => {
                println!("{pad}Object {{");
                for entry in &o.entries {
                    print!("{pad}  key: ");
                    print_tree_inline(&entry.key);
                    println!();
                    print!("{pad}  value: ");
                    if is_complex_value(&entry.value) {
                        println!();
                        print_tree(&entry.value, indent + 2);
                    } else {
                        print_tree_inline(&entry.value);
                        println!();
                    }
                }
                println!("{pad}}}");
            }
        }
    }
}

fn print_payload(payload: &Payload, indent: usize) {
    let pad = "  ".repeat(indent);
    match payload {
        Payload::Scalar(s) => {
            println!("{pad}Scalar({:?}, {:?})", s.text, s.kind);
        }
        Payload::Sequence(s) => {
            println!("{pad}Sequence [");
            for item in &s.items {
                print_tree(item, indent + 1);
            }
            println!("{pad}]");
        }
        Payload::Object(o) => {
            println!("{pad}Object {{");
            for entry in &o.entries {
                print!("{pad}  key: ");
                print_tree_inline(&entry.key);
                println!();
                print!("{pad}  value: ");
                if is_complex_value(&entry.value) {
                    println!();
                    print_tree(&entry.value, indent + 2);
                } else {
                    print_tree_inline(&entry.value);
                    println!();
                }
            }
            println!("{pad}}}");
        }
    }
}

fn is_complex_value(value: &Value) -> bool {
    if value.tag.is_some() && value.payload.is_some() {
        return true;
    }
    matches!(
        &value.payload,
        Some(Payload::Object(_)) | Some(Payload::Sequence(_))
    )
}

fn print_tree_inline(value: &Value) {
    if let Some(tag) = &value.tag {
        if value.payload.is_some() {
            print!("Tagged @{} {{...}}", tag.name);
        } else {
            print!("Tagged @{}", tag.name);
        }
    } else {
        match &value.payload {
            None => print!("Unit"),
            Some(Payload::Scalar(s)) => print!("Scalar({:?})", s.text),
            Some(Payload::Sequence(_)) => print!("Sequence [...]"),
            Some(Payload::Object(_)) => print!("Object {{...}}"),
        }
    }
}

/// Print canonical styx representation
fn print_canonical(value: &Value, indent: usize, is_root: bool) {
    let _pad = "  ".repeat(indent);

    // Print tag if present
    if let Some(tag) = &value.tag {
        print!("@{}", tag.name);
        if let Some(payload) = &value.payload {
            match payload {
                Payload::Object(_) => {
                    print!(" ");
                    print_payload_canonical(payload, indent, false);
                }
                Payload::Sequence(_) => {
                    print_payload_canonical(payload, indent, false);
                }
                Payload::Scalar(_) => {
                    print!(" ");
                    print_payload_canonical(payload, indent, false);
                }
            }
        }
    } else {
        // No tag
        match &value.payload {
            None => {
                print!("@");
            }
            Some(payload) => {
                print_payload_canonical(payload, indent, is_root);
            }
        }
    }

    fn print_payload_canonical(payload: &Payload, indent: usize, is_root: bool) {
        let pad = "  ".repeat(indent);
        match payload {
            Payload::Scalar(s) => {
                if needs_quoting(&s.text) {
                    print!("{:?}", s.text);
                } else {
                    print!("{}", s.text);
                }
            }
            Payload::Sequence(s) => {
                print!("(");
                for (i, item) in s.items.iter().enumerate() {
                    if i > 0 {
                        print!(" ");
                    }
                    print_canonical(item, indent, false);
                }
                print!(")");
            }
            Payload::Object(o) => {
                if is_root {
                    // Root object: no braces, newline separated
                    for (i, entry) in o.entries.iter().enumerate() {
                        if i > 0 {
                            println!();
                        }
                        print_canonical(&entry.key, indent, false);
                        print!(" ");
                        print_canonical(&entry.value, indent, false);
                    }
                } else {
                    // Nested object: with braces
                    if o.entries.is_empty() {
                        print!("{{}}");
                    } else if o.entries.len() == 1 && is_simple_entry(&o.entries[0]) {
                        // Single simple entry: inline
                        print!("{{ ");
                        print_canonical(&o.entries[0].key, indent, false);
                        print!(" ");
                        print_canonical(&o.entries[0].value, indent, false);
                        print!(" }}");
                    } else {
                        // Multi-line
                        println!("{{");
                        for entry in &o.entries {
                            print!("{pad}  ");
                            print_canonical(&entry.key, indent + 1, false);
                            print!(" ");
                            print_canonical(&entry.value, indent + 1, false);
                            println!();
                        }
                        print!("{pad}}}");
                    }
                }
            }
        }
    }
}

fn is_simple_entry(entry: &Entry) -> bool {
    is_simple_value(&entry.key) && is_simple_value(&entry.value)
}

fn is_simple_value(value: &Value) -> bool {
    // Simple: no payload, or scalar with no tag, or just a tag
    if value.is_unit() {
        return true;
    }
    if value.tag.is_some() && value.payload.is_none() {
        return true;
    }
    if value.tag.is_none() && matches!(&value.payload, Some(Payload::Scalar(_))) {
        return true;
    }
    false
}

fn needs_quoting(s: &str) -> bool {
    if s.is_empty() {
        return true;
    }
    // Check for characters that need quoting
    s.contains(|c: char| c.is_whitespace() || "{}()@#\"\\'".contains(c))
        || s.starts_with('@')
        || s.starts_with('#')
}

/// Convert to JSON-like structure (for debugging)
fn value_to_json(value: &Value) -> serde_json::Value {
    if let Some(tag) = &value.tag {
        let mut obj = serde_json::Map::new();
        obj.insert(
            "$tag".to_string(),
            serde_json::Value::String(tag.name.clone()),
        );
        if let Some(payload) = &value.payload {
            obj.insert("$payload".to_string(), payload_to_json(payload));
        }
        serde_json::Value::Object(obj)
    } else {
        match &value.payload {
            None => serde_json::Value::Null,
            Some(payload) => payload_to_json(payload),
        }
    }
}

fn payload_to_json(payload: &Payload) -> serde_json::Value {
    match payload {
        Payload::Scalar(s) => serde_json::Value::String(s.text.clone()),
        Payload::Sequence(s) => {
            serde_json::Value::Array(s.items.iter().map(value_to_json).collect())
        }
        Payload::Object(o) => {
            let mut obj = serde_json::Map::new();
            for entry in &o.entries {
                let key = if entry.key.is_unit() {
                    "@".to_string()
                } else if let Some(s) = entry.key.as_str() {
                    s.to_string()
                } else {
                    format!("{:?}", entry.key)
                };
                obj.insert(key, value_to_json(&entry.value));
            }
            serde_json::Value::Object(obj)
        }
    }
}
