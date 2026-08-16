fn main() {
    println!("cargo:rerun-if-changed=../../contracts/receipts/v1/limits.json");
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../contracts/receipts/v1/limits.json");
    let manifest: serde_json::Value = serde_json::from_slice(&std::fs::read(path).expect("receipt limits manifest"))
        .expect("valid receipt limits manifest");
    let out = format!(
        "pub const MAX_ENVELOPE_BYTES: usize = {};&#xA;pub const MAX_PAYLOAD_BYTES: usize = {};&#xA;pub const MAX_IDENTIFIER_BYTES: usize = {};&#xA;pub const MAX_DEPTH: usize = {};&#xA;",
        manifest["max_envelope_bytes"], manifest["max_payload_bytes"], manifest["max_identifier_bytes"], manifest["max_depth"]
    ).replace("&#xA;", "\n");
    std::fs::write(std::path::Path::new(&std::env::var("OUT_DIR").unwrap()).join("receipt_limits.rs"), out).expect("write generated receipt limits");
}
