//! Shared trust contract for the one pinned local llama.cpp adapter.
//!
//! Core verifies the package during installation and status checks; Supervisor
//! repeats the same file checks immediately before creating a child process.

use sha2::{Digest, Sha256};
use std::path::Path;

/// Pinned llama.cpp release used for local CPU quantization and inference.
pub const LLAMA_CPP_VERSION: &str = "b10981";
/// SHA-256 of the pinned Windows x64 CPU release archive.
pub const LLAMA_CPP_ARCHIVE_SHA256: &str =
    "ca53c86dba93aaa23a2b6bcc5bf5e19409de11c28dfebdd87c0148d687dead71";
/// Exact size of the pinned Windows x64 CPU release archive.
pub const LLAMA_CPP_ARCHIVE_SIZE_BYTES: u64 = 18_428_680;
/// Fixed upstream release asset URL.
pub const LLAMA_CPP_ASSET_URL: &str =
    "https://github.com/ggml-org/llama.cpp/releases/download/b10981/llama-b10981-bin-win-cpu-x64.zip";
/// SHA-256 allowlist for executable and DLL files needed by quantizer/server.
pub const LLAMA_CPP_RUNTIME_FILES: &[(&str, &str)] = &[
    (
        "llama-quantize.exe",
        "909951611a2caf48d4c580c773d1ce1048bc3fe81007b262266402d7c28e065c",
    ),
    (
        "llama-quantize-impl.dll",
        "4563b86a4e4f77f2fefd2c3d05154498a91d4243c10159ad9e7155a4bef22a6f",
    ),
    (
        "llama-server.exe",
        "ac50ff262975f8f87a2b0855ae58562e3b36ef197f570c4aab5d33005a9ab3e3",
    ),
    (
        "llama-server-impl.dll",
        "d27c6b5b581a440aea51bf63b03cd823bed40de0b48dee4835520aa1d4309ed0",
    ),
    (
        "llama-common.dll",
        "9a45f5de067b9c71161ca58f92627fb835c6d25718c6f96598d6ceacbfea72f8",
    ),
    (
        "llama.dll",
        "5eab64be02c12c494e063fb4ef45f8b1dbf720eb707266ef2dd57334dc984687",
    ),
    (
        "ggml.dll",
        "994f391985477ea466f585018806be551f983e5d968af84971e032e96b65b8c2",
    ),
    (
        "ggml-base.dll",
        "0b95213a24e405ceb040aff94680238a0c0d6da8c6d3873de3c78737fce128e4",
    ),
    (
        "ggml-cpu-x64.dll",
        "743ce42593633c84867ad03b2d1244090c1857581dd1113e39f1b925999576ef",
    ),
    (
        "libomp.dll",
        "a12116ba72d1d6820407cf30be23da04ce79d6bb8a71a5ee71759c5a1faa6f1c",
    ),
];
/// SHA-256 of the upstream OpenMP license file retained with the package.
pub const LLAMA_CPP_OPENMP_LICENSE_SHA256: &str =
    "fdad1758a9e1f9d5a81e18879b3406772115edc92c24bfa36b70c654f325e8e4";

/// Checks the managed directory ancestry and hashes of all runtime binaries.
///
/// The directory must be `tools/llama.cpp/b10981`; its three directory
/// components must be real directories, not symlinks. The caller separately
/// validates the manifest and the copied llama.cpp MIT license.
pub fn verify_runtime_files(directory: &Path) -> bool {
    let Some(package_parent) = directory.parent() else {
        return false;
    };
    let Some(tools_root) = package_parent.parent() else {
        return false;
    };
    if !is_plain_directory(directory)
        || !is_plain_directory(package_parent)
        || !is_plain_directory(tools_root)
    {
        return false;
    }
    LLAMA_CPP_RUNTIME_FILES
        .iter()
        .all(|(name, expected)| file_sha256_matches(&directory.join(name), expected))
        && file_sha256_matches(
            &directory.join("LICENSE-LLVM-OpenMP"),
            LLAMA_CPP_OPENMP_LICENSE_SHA256,
        )
}

fn is_plain_directory(path: &Path) -> bool {
    let Ok(metadata) = std::fs::symlink_metadata(path) else {
        return false;
    };
    if !metadata.file_type().is_dir() || metadata.file_type().is_symlink() {
        return false;
    }
    #[cfg(windows)]
    {
        use std::os::windows::fs::MetadataExt;
        const FILE_ATTRIBUTE_REPARSE_POINT: u32 = 0x0400;
        return metadata.file_attributes() & FILE_ATTRIBUTE_REPARSE_POINT == 0;
    }
    #[cfg(not(windows))]
    {
        true
    }
}

fn file_sha256_matches(path: &Path, expected: &str) -> bool {
    let Ok(metadata) = std::fs::symlink_metadata(path) else {
        return false;
    };
    if !metadata.file_type().is_file() || metadata.len() > 16 * 1024 * 1024 {
        return false;
    }
    let Ok(bytes) = std::fs::read(path) else {
        return false;
    };
    let digest = Sha256::digest(bytes);
    expected.len() == digest.len() * 2
        && expected
            .as_bytes()
            .chunks_exact(2)
            .zip(digest)
            .all(|(pair, byte)| {
                let high = (pair[0] as char).to_digit(16);
                let low = (pair[1] as char).to_digit(16);
                high.zip(low)
                    .is_some_and(|(high, low)| ((high << 4) | low) as u8 == byte)
            })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn runtime_hash_contract_is_bounded_and_unique() {
        assert_eq!(LLAMA_CPP_RUNTIME_FILES.len(), 10);
        assert!(
            LLAMA_CPP_RUNTIME_FILES
                .iter()
                .all(|(_, hash)| hash.len() == 64
                    && hash.bytes().all(|byte| byte.is_ascii_hexdigit()))
        );
        let names: std::collections::BTreeSet<_> = LLAMA_CPP_RUNTIME_FILES
            .iter()
            .map(|(name, _)| *name)
            .collect();
        assert_eq!(names.len(), LLAMA_CPP_RUNTIME_FILES.len());
    }

    #[test]
    fn modified_or_incomplete_package_is_not_trusted() {
        struct TempRoot(std::path::PathBuf);
        impl Drop for TempRoot {
            fn drop(&mut self) {
                let _ = std::fs::remove_dir_all(&self.0);
            }
        }
        let root = std::env::temp_dir().join(format!(
            "evohime-adapter-contract-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        std::fs::create_dir(&root).unwrap();
        let _cleanup = TempRoot(root.clone());
        let package = root.join("tools/llama.cpp").join(LLAMA_CPP_VERSION);
        std::fs::create_dir_all(&package).unwrap();
        std::fs::write(package.join("llama-server.exe"), b"modified executable").unwrap();
        assert!(!verify_runtime_files(&package));
    }
}
