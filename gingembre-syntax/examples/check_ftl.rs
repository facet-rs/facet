//! Parse every .jinja template under a directory; report parse errors and any
//! lossless violations. Usage: cargo run -p gingembre-syntax --example check_ftl -- <dir>
use std::path::Path;

fn walk(dir: &Path, out: &mut Vec<std::path::PathBuf>) {
    for e in std::fs::read_dir(dir).unwrap().flatten() {
        let p = e.path();
        if p.is_dir() {
            walk(&p, out);
        } else if p.extension().is_some_and(|x| x == "jinja") {
            out.push(p);
        }
    }
}

fn main() {
    let dir = std::env::args().nth(1).expect("need a dir");
    let mut files = Vec::new();
    walk(Path::new(&dir), &mut files);
    files.sort();
    let (mut ok, mut err_files) = (0usize, 0usize);
    for f in &files {
        let src = std::fs::read_to_string(f).unwrap();
        let parse = gingembre_syntax::parse(&src);
        let lossless = parse.syntax().to_string() == src;
        if parse.errors.is_empty() && lossless {
            ok += 1;
        } else {
            err_files += 1;
            let rel = f.strip_prefix(&dir).unwrap_or(f).display();
            if !lossless {
                println!("LOSSLESS VIOLATION: {rel}");
            }
            for e in parse.errors.iter().take(4) {
                println!("  {rel}:{} {}", e.offset, e.message);
            }
        }
    }
    println!("\n{} templates: {} clean, {} with issues", files.len(), ok, err_files);
}
