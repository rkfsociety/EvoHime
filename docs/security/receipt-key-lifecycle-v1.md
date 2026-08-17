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
