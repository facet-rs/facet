use std::env;

fn main() {
    println!("cargo::rustc-check-cfg=cfg(snark_jit_active)");
    if env::var("DEP_WEAVY_JIT").as_deref() == Ok("1") {
        println!("cargo::rustc-cfg=snark_jit_active");
    }
}
