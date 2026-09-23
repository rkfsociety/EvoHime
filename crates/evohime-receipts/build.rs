#![deny(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

fn main() -> Result<(), Box<dyn std::error::Error>> {
    println!("cargo:rerun-if-changed=../../contracts/receipts/v1/limits.json");
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../contracts/receipts/v1/limits.json");
    let manifest: serde_json::Value = serde_json::from_slice(&std::fs::read(path)?)?;
    let out = format!(
        "/// Maximum serialized receipt envelope size in bytes.&#xA;pub const MAX_ENVELOPE_BYTES: usize = {};&#xA;/// Maximum canonical receipt payload size in bytes.&#xA;pub const MAX_PAYLOAD_BYTES: usize = {};&#xA;/// Maximum UTF-8 byte length of a receipt identifier.&#xA;pub const MAX_IDENTIFIER_BYTES: usize = {};&#xA;/// Maximum nesting depth accepted by canonical JSON validation.&#xA;pub const MAX_DEPTH: usize = {};&#xA;/// Version of the canonical fingerprint input contract.&#xA;pub const FINGERPRINT_INPUT_VERSION: u8 = {};&#xA;/// Version of the receipt audit-sampling policy contract.&#xA;pub const SAMPLING_POLICY_VERSION: u8 = {};&#xA;/// Default sampling rate, as a percentage, for read-only operations.&#xA;pub const DEFAULT_READ_ONLY_SAMPLING_RATE: u8 = {};&#xA;/// Maximum preview text size in bytes exposed by the receipt contract.&#xA;pub const CONTRACT_MAX_PREVIEW_BYTES: usize = {};&#xA;",
        manifest["max_envelope_bytes"], manifest["max_payload_bytes"], manifest["max_identifier_bytes"], manifest["max_depth"], manifest["fingerprint_input_version"], manifest["sampling_policy_version"], manifest["default_read_only_sampling_rate"], manifest["max_preview_bytes"]
    ).replace("&#xA;", "\n");
    let out_dir = std::env::var_os("OUT_DIR")
        .ok_or_else(|| std::io::Error::new(std::io::ErrorKind::NotFound, "OUT_DIR is not set"))?;
    std::fs::write(
        std::path::Path::new(&out_dir).join("receipt_limits.rs"),
        out,
    )?;
    Ok(())
}
