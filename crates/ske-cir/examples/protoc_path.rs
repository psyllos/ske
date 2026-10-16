//! Locate the pinned compiler used by both Rust and Go schema generation.
fn main() {
    println!("{}", env!("SKE_VENDORED_PROTOC"));
}
