// `sqlx::migrate!` (db.rs) embeds the migrations at build time: rebuild
// when one is added or changed, not only when this crate's code changes.
fn main() {
    println!("cargo:rerun-if-changed=../../migrations");
}
