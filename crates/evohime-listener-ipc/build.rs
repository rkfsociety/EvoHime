#![deny(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let protoc = protoc_bin_vendored::protoc_bin_path()?;
    std::env::set_var("PROTOC", protoc);
    prost_build::Config::new().compile_protos(&["proto/evohime.listener.proto"], &["proto"])?;
    println!("cargo:rerun-if-changed=proto/evohime.listener.proto");
    Ok(())
}
