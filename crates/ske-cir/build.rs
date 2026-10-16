fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut config = prost_build::Config::new();
    let protoc = protoc_bin_vendored::protoc_bin_path()?;
    config.protoc_executable(&protoc);
    // Expose the same pinned compiler to the Go generation helper.
    println!("cargo:rustc-env=SKE_VENDORED_PROTOC={}", protoc.display());
    config.compile_protos(
        &[
            "../../schemas/cir/v1/cir.proto",
            "../../schemas/adapter/v1/adapter.proto",
        ],
        &["../../schemas"],
    )?;
    println!("cargo:rerun-if-changed=../../schemas");
    Ok(())
}
