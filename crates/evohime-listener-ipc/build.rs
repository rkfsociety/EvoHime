fn main() {
    let protoc = protoc_bin_vendored::protoc_bin_path().unwrap();
    std::env::set_var("PROTOC", protoc);
    prost_build::Config::new()
        .compile_protos(&["proto/evohime.listener.proto"], &["proto"])
        .unwrap();
    println!("cargo:rerun-if-changed=proto/evohime.listener.proto");
}
