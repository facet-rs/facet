use std::env;

fn main() {
    println!("cargo::rustc-check-cfg=cfg(snark_jit_active)");
    if env::var_os("DEP_WEAVY_JIT").is_some() {
        println!("cargo::rustc-cfg=snark_jit_active");
    }
}
