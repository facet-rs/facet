//! Generate the TypeScript client for the Daemon service. The daemon's RPC
//! surface is open protocol — any IDE/tool can generate a client against it.
//!
//! ```sh
//! cargo run -p vix-daemon --features codegen --bin gen_ts -- path/to/daemon.ts
//! ```
//!
//! With no argument, the module is written to stdout.

use std::path::Path;

fn main() {
    let detail = vix_daemon::daemon_service_descriptor();
    let ts = vox_codegen::targets::typescript::generate_service(detail);

    match std::env::args().nth(1) {
        Some(out) => {
            let out = Path::new(&out);
            if let Some(parent) = out.parent() {
                std::fs::create_dir_all(parent).expect("create output dir");
            }
            std::fs::write(out, ts).expect("write generated client");
            eprintln!("wrote {}", out.display());
        }
        None => print!("{ts}"),
    }
}
