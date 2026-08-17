# Receipt key lifecycle v1

Receipt signing keys belong to Rust Core. On Windows the PKCS#8 Ed25519
private key is protected with DPAPI `CurrentUser` scope and is never passed by
Electron, supervisor arguments, environment, IPC, logs, audit, or exports.

The public transition chain is an append-only diagnostic/export snapshot. Its
trust root is explicit: a valid signature without a pinned genesis is
`untrusted`. Missing or damaged active material is fail-closed and cannot
silently create a replacement genesis. Rotation uses a durable journal and
does not delete retired public keys.

Normative schemas are in `contracts/receipts/v1/`; the offline verifier is the
`evohime-verify` binary in `evohime-receipts`. It reads only explicitly supplied
public history and trust input and does not open SQLite, Core, network, or the
active private-key file.

The mutable source of truth is the migrated SQLite database: transition and
audit references commit together, while `public-history-v1.jsonl` and its
manifest are fsync/atomic post-commit exports. Rotation phases are recorded in
`rotation-state-v1.json`; startup and supervisor fail closed on an invalid or
ambiguous journal. Scheduled rotation is checked at 90 calendar days and no
more than once per 24 hours. Manual, compromise and approved recovery commands
are exposed through authenticated desktop IPC and one-time approval tokens.

The package includes the verifier, schemas, RFC 8032 transition vector and
checkpoint vector. Windows CI runs the lifecycle contract smoke test and
verifies trusted, untrusted, damaged-input and invalid-argument exit paths.
