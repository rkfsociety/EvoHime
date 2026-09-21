/// Immutable fields required to append a signed model-request receipt.
pub struct ModelRequestReceiptInput<'a> {
    pub request_id: &'a str,
    pub logical_request_id: &'a str,
    pub ledger_id: &'a str,
    pub attempt: u32,
    pub provider: &'a str,
    pub model: &'a str,
    pub envelope_hash: &'a str,
    pub context_projection_hash: &'a str,
    pub route_snapshot_hash: &'a str,
    pub policy_snapshot_hash: &'a str,
}
