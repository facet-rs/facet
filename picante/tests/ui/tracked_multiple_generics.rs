#[picante::tracked]
pub async fn foo<DB, T>(db: &DB) -> u64 {
    let _ = (db, std::marker::PhantomData::<T>);
    0
}

fn main() {}
