//! Core-owned lifecycle and redacted provenance for structured model output.

pub use evohime_model_gateway::structured_response::{
    ResponseContract, ResponseError, ResponseResult, ResponseStrategy, MAX_REPAIR_ATTEMPTS,
    MAX_SCHEMA_BYTES, MAX_TOTAL_ATTEMPTS, STRUCTURED_RESPONSE_SCHEMA_VERSION,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ResponseState {
    Validating,
    Dispatched,
    Repairing,
    Completed,
    Failed,
    Unknown,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ResponseRunSnapshot {
    pub run_id: String,
    pub contract_hash: String,
    pub policy_hash: String,
    pub state: ResponseState,
    pub attempts: u32,
}

impl ResponseRunSnapshot {
    pub fn new(
        run_id: impl Into<String>,
        contract: &ResponseContract,
        policy_hash: impl Into<String>,
    ) -> Result<Self, ResponseError> {
        contract.validate_schema()?;
        Ok(Self {
            run_id: run_id.into(),
            contract_hash: contract.contract_hash.clone(),
            policy_hash: policy_hash.into(),
            state: ResponseState::Validating,
            attempts: 0,
        })
    }
    pub fn dispatch(&mut self) {
        self.state = ResponseState::Dispatched;
        self.attempts = self.attempts.saturating_add(1);
    }
    pub fn repair(&mut self) -> Result<(), ResponseError> {
        if self.attempts >= MAX_TOTAL_ATTEMPTS {
            return Err(ResponseError::RepairLimit);
        }
        self.state = ResponseState::Repairing;
        self.attempts += 1;
        Ok(())
    }
    pub fn complete(&mut self) {
        self.state = ResponseState::Completed;
    }
    pub fn unknown_after_restart(&mut self) {
        self.state = ResponseState::Unknown;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    #[test]
    fn restart_never_retries_unknown_model_call() {
        let contract =
            ResponseContract::new("c", 1, json!({"type":"object"}), ResponseStrategy::Auto)
                .unwrap();
        let mut run = ResponseRunSnapshot::new("run", &contract, "policy").unwrap();
        run.dispatch();
        run.unknown_after_restart();
        assert_eq!(run.state, ResponseState::Unknown);
        assert_eq!(run.attempts, 1);
    }
}
