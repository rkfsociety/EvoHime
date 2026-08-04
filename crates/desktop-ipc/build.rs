fn main() {
    let protoc = protoc_bin_vendored::protoc_bin_path().expect("vendored protoc must exist");
    std::env::set_var("PROTOC", protoc);

    prost_build::Config::new()
        .compile_protos(&["proto/evohime.desktop.proto"], &["proto"])
        .expect("desktop IPC protobuf must compile");
    println!("cargo:rerun-if-changed=proto/evohime.desktop.proto");
}
