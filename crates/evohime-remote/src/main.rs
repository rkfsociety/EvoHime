#![cfg_attr(
    not(test),
    deny(clippy::unwrap_used, clippy::expect_used, clippy::panic)
)]

use std::process::ExitCode;

use evohime_remote::service::RemoteA2aService;
use evohime_service_contract::serve_stdio;

fn main() -> ExitCode {
    let service = RemoteA2aService::default();
    match serve_stdio(&service) {
        Ok(()) => ExitCode::SUCCESS,
        Err(err) => {
            eprintln!("evohime-remote exited with error: {err}");
            ExitCode::FAILURE
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use evohime_service_contract::{
        dispatch_request, validate_descriptor, ServiceHandler, ServiceRequest, ServiceResponse,
    };

    #[test]
    fn remote_a2a_descriptor_and_dispatch_work() {
        let svc = RemoteA2aService::default();
        validate_descriptor(&svc.descriptor()).expect("valid descriptor");
        let resp = dispatch_request(
            &svc,
            ServiceRequest::Invoke {
                request_id: "r1".to_owned(),
                capability: "remote.dispatch_task".to_owned(),
                payload: serde_json::json!({ "agent_id": "a2a-coder" }),
                caller_session_id: None,
            },
        );
        assert!(matches!(resp, ServiceResponse::InvokeResult { .. }));
    }
}
