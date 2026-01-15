//! Styx CLI tool

use std::io::{self, Read};

use facet::Facet;
use facet_args as args;
use styx_tree::{Entry, Tagged, Value};

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
    match value {
        Value::Scalar(s) => {
            println!("{pad}Scalar({:?}, {:?})", s.text, s.kind);
        }
        Value::Unit => {
            println!("{pad}Unit");
        }
        Value::Tagged(t) => {
            if let Some(payload) = &t.payload {
                println!("{pad}Tagged @{} {{", t.tag);
                print_tree(payload, indent + 1);
                println!("{pad}}}");
            } else {
                println!("{pad}Tagged @{}", t.tag);
            }
        }
        Value::Sequence(s) => {
            println!("{pad}Sequence [");
            for item in &s.items {
                print_tree(item, indent + 1);
            }
            println!("{pad}]");
        }
        Value::Object(o) => {
            println!("{pad}Object {{");
            for entry in &o.entries {
                print!("{pad}  key: ");
                print_tree_inline(&entry.key);
                println!();
                print!("{pad}  value: ");
                if matches!(
                    &entry.value,
                    Value::Object(_)
                        | Value::Sequence(_)
                        | Value::Tagged(Tagged {
                            payload: Some(_),
                            ..
                        })
                ) {
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

fn print_tree_inline(value: &Value) {
    match value {
        Value::Scalar(s) => print!("Scalar({:?})", s.text),
        Value::Unit => print!("Unit"),
        Value::Tagged(t) => {
            if t.payload.is_some() {
                print!("Tagged @{} {{...}}", t.tag);
            } else {
                print!("Tagged @{}", t.tag);
            }
        }
        Value::Sequence(_) => print!("Sequence [...]"),
        Value::Object(_) => print!("Object {{...}}"),
    }
}

/// Print canonical styx representation
fn print_canonical(value: &Value, indent: usize, is_root: bool) {
    let pad = "  ".repeat(indent);
    match value {
        Value::Scalar(s) => {
            // Quote if needed
            if needs_quoting(&s.text) {
                print!("{:?}", s.text);
            } else {
                print!("{}", s.text);
            }
        }
        Value::Unit => {
            print!("@");
        }
        Value::Tagged(t) => {
            print!("@{}", t.tag);
            if let Some(payload) = &t.payload {
                match payload.as_ref() {
                    Value::Object(_) => {
                        print!(" ");
                        print_canonical(payload, indent, false);
                    }
                    Value::Sequence(_) => {
                        print_canonical(payload, indent, false);
                    }
                    _ => {
                        print!(" ");
                        print_canonical(payload, indent, false);
                    }
                }
            }
        }
        Value::Sequence(s) => {
            print!("(");
            for (i, item) in s.items.iter().enumerate() {
                if i > 0 {
                    print!(" ");
                }
                print_canonical(item, indent, false);
            }
            print!(")");
        }
        Value::Object(o) => {
            if is_root {
                // Root object: no braces, newline separated
                for (i, entry) in o.entries.iter().enumerate() {
                    if i > 0 {
                        println!();
                    }
                    print_canonical(&entry.key, indent, false);
                    print!(" ");
                    if matches!(&entry.value, Value::Object(_)) {
                        print_canonical(&entry.value, indent, false);
                    } else {
                        print_canonical(&entry.value, indent, false);
                    }
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

fn is_simple_entry(entry: &Entry) -> bool {
    is_simple_value(&entry.key) && is_simple_value(&entry.value)
}

fn is_simple_value(value: &Value) -> bool {
    matches!(
        value,
        Value::Scalar(_) | Value::Unit | Value::Tagged(Tagged { payload: None, .. })
    )
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
    match value {
        Value::Scalar(s) => serde_json::Value::String(s.text.clone()),
        Value::Unit => serde_json::Value::Null,
        Value::Tagged(t) => {
            let mut obj = serde_json::Map::new();
            obj.insert("$tag".to_string(), serde_json::Value::String(t.tag.clone()));
            if let Some(payload) = &t.payload {
                obj.insert("$payload".to_string(), value_to_json(payload));
            }
            serde_json::Value::Object(obj)
        }
        Value::Sequence(s) => serde_json::Value::Array(s.items.iter().map(value_to_json).collect()),
        Value::Object(o) => {
            let mut obj = serde_json::Map::new();
            for entry in &o.entries {
                let key = match &entry.key {
                    Value::Scalar(s) => s.text.clone(),
                    Value::Unit => "@".to_string(),
                    _ => format!("{:?}", entry.key),
                };
                obj.insert(key, value_to_json(&entry.value));
            }
            serde_json::Value::Object(obj)
        }
    }
}
