/// Immutable fields required to append a signed model-request receipt.
pub struct ModelRequestReceiptInput<'a> {
    /// Identifier of this model request attempt.
    pub request_id: &'a str,
    /// Identifier shared by retries of the same logical request.
    pub logical_request_id: &'a str,
    /// Identifier of the ledger entry that owns the request.
    pub ledger_id: &'a str,
    /// One-based attempt number for the logical request.
    pub attempt: u32,
    /// Provider identity used for dispatch.
    pub provider: &'a str,
    /// Model identity used for dispatch.
    pub model: &'a str,
    /// Digest of the canonical request envelope.
    pub envelope_hash: &'a str,
    /// Digest of the context projection supplied to the model.
    pub context_projection_hash: &'a str,
    /// Digest of the selected route snapshot.
    pub route_snapshot_hash: &'a str,
    /// Digest of the policy snapshot authorizing the request.
    pub policy_snapshot_hash: &'a str,
}
