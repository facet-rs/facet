#[picante::tracked]
pub async fn foo(db: &DB) -> u64 {
    let _ = db;
    0
}

fn main() {}
