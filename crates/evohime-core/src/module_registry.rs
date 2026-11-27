//! Registry and invocation proxy for isolated EvoHime service modules.
//!
//! Each external module advertises its capabilities through
//! [`evohime_service_contract::ServiceDescriptor`]. `ModuleRegistry` validates
//! descriptors, tracks availability, enforces Core permission checks before
//! dispatch, and routes capability invocations without coupling `evohime-core`
//! to module-internal implementations.

use std::collections::BTreeMap;
use std::future::Future;
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::pin::Pin;
use std::process::{Child, Command, Stdio};
use std::sync::{Arc, Mutex, RwLock};

use evohime_service_contract::{
    decode_frame, encode_frame, validate_descriptor, MAX_SERVICE_FRAME_BYTES,
    ServiceCapabilityDescriptor, ServiceContractError, ServiceDescriptor, ServiceRequest,
    ServiceResponse,
};

/// Boxed `Send` future returned by [`ServiceTransport`].
pub type ServiceFuture<'a, T> = Pin<Box<dyn Future<Output = T> + Send + 'a>>;

/// Transport abstraction between `evohime-core` and an isolated service module.
pub trait ServiceTransport: Send + Sync {
    /// Sends one [`ServiceRequest`] to the target module and awaits its
    /// [`ServiceResponse`].
    fn send(&self, request: ServiceRequest) -> ServiceFuture<'_, Result<ServiceResponse, ServiceContractError>>;
}

/// Runtime availability state of a registered service module.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ModuleAvailability {
    /// Module is healthy and ready to accept invocations.
    Ready,
    /// Module is temporarily unreachable or disabled; invocations fail cleanly
    /// with [`evohime_service_contract::ServiceErrorCode::Unavailable`].
    Unavailable {
        /// Diagnostic reason why the module is unavailable.
        reason: String,
    },
}

/// Summary snapshot of a registered service module for diagnostics and UI.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RegisteredModuleInfo {
    /// Canonical module identifier.
    pub service_id: String,
    /// Running module version.
    pub version: String,
    /// Current availability state.
    pub availability: ModuleAvailability,
    /// Number of capabilities exposed by this module.
    pub capability_count: usize,
}

struct RegisteredModule {
    descriptor: ServiceDescriptor,
    availability: ModuleAvailability,
    transport: Arc<dyn ServiceTransport>,
}

#[derive(Default)]
struct RegistryState {
    modules: BTreeMap<String, RegisteredModule>,
    capability_owner: BTreeMap<String, String>,
}

/// Thread-safe registry and invocation proxy for external service modules.
#[derive(Clone, Default)]
pub struct ModuleRegistry {
    state: Arc<RwLock<RegistryState>>,
}

/// [`ServiceTransport`] backed by a child service-module process speaking
/// `evohime-service-contract` frames over `stdin`/`stdout`.
pub struct StdioProcessTransport {
    executable: PathBuf,
    process: Mutex<Option<Child>>,
}

impl StdioProcessTransport {
    /// Creates a lazy process transport for `executable`.
    pub fn new(executable: impl AsRef<Path>) -> Self {
        Self {
            executable: executable.as_ref().to_path_buf(),
            process: Mutex::new(None),
        }
    }

    fn send_blocking(&self, request: ServiceRequest) -> Result<ServiceResponse, ServiceContractError> {
        let frame = encode_frame(&request)?;
        let mut guard = self.process.lock().map_err(|_| {
            ServiceContractError::unavailable("service process lock poisoned")
        })?;
        if guard.is_none() {
            let child = Command::new(&self.executable)
                .stdin(Stdio::piped())
                .stdout(Stdio::piped())
                .stderr(Stdio::null())
                .spawn()
                .map_err(|err| {
                    ServiceContractError::unavailable(format!(
                        "failed to spawn {}: {err}",
                        self.executable.display()
                    ))
                })?;
            *guard = Some(child);
        }
        let child = guard.as_mut().ok_or_else(|| {
            ServiceContractError::unavailable("service process handle missing")
        })?;
        let stdin = child.stdin.as_mut().ok_or_else(|| {
            ServiceContractError::unavailable("service stdin is unavailable")
        })?;
        if let Err(err) = stdin.write_all(&frame).and_then(|()| stdin.flush()) {
            *guard = None;
            return Err(ServiceContractError::unavailable(format!(
                "failed to write to service stdin: {err}"
            )));
        }
        let stdout = child.stdout.as_mut().ok_or_else(|| {
            ServiceContractError::unavailable("service stdout is unavailable")
        })?;
        let mut len_buf = [0u8; 4];
        if let Err(err) = stdout.read_exact(&mut len_buf) {
            *guard = None;
            return Err(ServiceContractError::unavailable(format!(
                "failed to read response header: {err}"
            )));
        }
        let payload_len = u32::from_be_bytes(len_buf) as usize;
        if payload_len > MAX_SERVICE_FRAME_BYTES {
            *guard = None;
            return Err(ServiceContractError::invalid_input(format!(
                "service response {payload_len} bytes exceeds limit {MAX_SERVICE_FRAME_BYTES}"
            )));
        }
        let mut full_frame = Vec::with_capacity(4 + payload_len);
        full_frame.extend_from_slice(&len_buf);
        full_frame.resize(4 + payload_len, 0);
        if let Err(err) = stdout.read_exact(&mut full_frame[4..]) {
            *guard = None;
            return Err(ServiceContractError::unavailable(format!(
                "failed to read response body: {err}"
            )));
        }
        if matches!(request, ServiceRequest::Shutdown) {
            *guard = None;
        }
        decode_frame(&full_frame)
    }
}

impl Drop for StdioProcessTransport {
    fn drop(&mut self) {
        if let Ok(mut guard) = self.process.lock() {
            if let Some(mut child) = guard.take() {
                let _ = child.kill();
                let _ = child.wait();
            }
        }
    }
}

impl ServiceTransport for StdioProcessTransport {
    fn send(
        &self,
        request: ServiceRequest,
    ) -> ServiceFuture<'_, Result<ServiceResponse, ServiceContractError>> {
        Box::pin(async move { self.send_blocking(request) })
    }
}

impl ModuleRegistry {
    /// Creates an empty [`ModuleRegistry`].
    pub fn new() -> Self {
        Self::default()
    }

    /// Validates and registers a service module with its transport.
    ///
    /// Rejects invalid descriptors or capability name collisions with already
    /// registered modules.
    pub fn register(
        &self,
        descriptor: ServiceDescriptor,
        transport: Arc<dyn ServiceTransport>,
    ) -> Result<(), ServiceContractError> {
        validate_descriptor(&descriptor)?;

        let mut guard = self
            .state
            .write()
            .map_err(|_| ServiceContractError::unavailable("module registry lock poisoned"))?;

        if let Some(existing) = guard.modules.get(&descriptor.service_id) {
            let old_caps: Vec<String> = existing
                .descriptor
                .capabilities
                .iter()
                .map(|c| c.name.clone())
                .collect();
            for cap in old_caps {
                guard.capability_owner.remove(&cap);
            }
        }

        for cap in &descriptor.capabilities {
            if let Some(owner) = guard.capability_owner.get(&cap.name) {
                if owner != &descriptor.service_id {
                    return Err(ServiceContractError::invalid_input(format!(
                        "capability '{}' is already owned by module '{}'",
                        cap.name, owner
                    )));
                }
            }
        }

        for cap in &descriptor.capabilities {
            guard
                .capability_owner
                .insert(cap.name.clone(), descriptor.service_id.clone());
        }

        guard.modules.insert(
            descriptor.service_id.clone(),
            RegisteredModule {
                descriptor,
                availability: ModuleAvailability::Ready,
                transport,
            },
        );
        Ok(())
    }

    /// Updates the availability state of a registered module.
    pub fn set_availability(
        &self,
        service_id: &str,
        availability: ModuleAvailability,
    ) -> Result<(), ServiceContractError> {
        let mut guard = self
            .state
            .write()
            .map_err(|_| ServiceContractError::unavailable("module registry lock poisoned"))?;
        let module = guard.modules.get_mut(service_id).ok_or_else(|| {
            ServiceContractError::not_found(format!("unknown service module: {service_id}"))
        })?;
        module.availability = availability;
        Ok(())
    }

    /// Lists all registered modules and their current availability.
    pub fn list_modules(&self) -> Vec<RegisteredModuleInfo> {
        let Ok(guard) = self.state.read() else {
            return Vec::new();
        };
        guard
            .modules
            .values()
            .map(|m| RegisteredModuleInfo {
                service_id: m.descriptor.service_id.clone(),
                version: m.descriptor.version.clone(),
                availability: m.availability.clone(),
                capability_count: m.descriptor.capabilities.len(),
            })
            .collect()
    }

    /// Returns the capability descriptor for `capability_name`, if registered.
    pub fn capability(&self, capability_name: &str) -> Option<ServiceCapabilityDescriptor> {
        let guard = self.state.read().ok()?;
        let service_id = guard.capability_owner.get(capability_name)?;
        let module = guard.modules.get(service_id)?;
        module
            .descriptor
            .capabilities
            .iter()
            .find(|c| c.name == capability_name)
            .cloned()
    }

    /// Authorizes via `authorize_permissions` and invokes `capability_name` on
    /// its owning module.
    pub async fn invoke<F>(
        &self,
        request_id: impl Into<String>,
        capability_name: &str,
        payload: serde_json::Value,
        caller_session_id: Option<String>,
        authorize_permissions: F,
    ) -> Result<serde_json::Value, ServiceContractError>
    where
        F: FnOnce(&[String]) -> Result<(), String>,
    {
        let request_id = request_id.into();
        let (transport, required_permissions) = {
            let guard = self.state.read().map_err(|_| {
                ServiceContractError::unavailable("module registry lock poisoned")
            })?;
            let service_id = guard.capability_owner.get(capability_name).ok_or_else(|| {
                ServiceContractError::not_found(format!(
                    "capability '{capability_name}' is not registered"
                ))
            })?;
            let module = guard.modules.get(service_id).ok_or_else(|| {
                ServiceContractError::unavailable(format!(
                    "service module '{service_id}' is missing"
                ))
            })?;
            if let ModuleAvailability::Unavailable { reason } = &module.availability {
                return Err(ServiceContractError::unavailable(format!(
                    "service module '{service_id}' is unavailable: {reason}"
                )));
            }
            let cap = module
                .descriptor
                .capabilities
                .iter()
                .find(|c| c.name == capability_name)
                .ok_or_else(|| {
                    ServiceContractError::not_found(format!(
                        "capability '{capability_name}' is not advertised by '{service_id}'"
                    ))
                })?;
            (Arc::clone(&module.transport), cap.required_permissions.clone())
        };

        if let Err(reason) = authorize_permissions(&required_permissions) {
            return Err(ServiceContractError::permission_denied(reason));
        }

        let response = transport
            .send(ServiceRequest::Invoke {
                request_id: request_id.clone(),
                capability: capability_name.to_owned(),
                payload,
                caller_session_id,
            })
            .await?;

        match response {
            ServiceResponse::InvokeResult {
                request_id: resp_id,
                payload,
            } if resp_id == request_id => Ok(payload),
            ServiceResponse::InvokeResult { request_id: resp_id, .. } => {
                Err(ServiceContractError::invalid_input(format!(
                    "mismatched response request_id: expected '{request_id}', got '{resp_id}'"
                )))
            }
            ServiceResponse::Error { error, .. } => Err(error),
            other => Err(ServiceContractError::invalid_input(format!(
                "unexpected response kind for Invoke: {other:?}"
            ))),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use evohime_service_contract::{ServiceErrorCode, SERVICE_CONTRACT_VERSION};

    struct MockTransport;

    impl ServiceTransport for MockTransport {
        fn send(
            &self,
            request: ServiceRequest,
        ) -> ServiceFuture<'_, Result<ServiceResponse, ServiceContractError>> {
            Box::pin(async move {
                match request {
                    ServiceRequest::Invoke {
                        request_id,
                        capability,
                        payload,
                        ..
                    } => Ok(ServiceResponse::InvokeResult {
                        request_id,
                        payload: serde_json::json!({
                            "handled_by": capability,
                            "echo": payload
                        }),
                    }),
                    _ => Ok(ServiceResponse::Pong {
                        contract_version: SERVICE_CONTRACT_VERSION,
                    }),
                }
            })
        }
    }

    fn sample_descriptor(service_id: &str, cap_name: &str) -> ServiceDescriptor {
        ServiceDescriptor {
            service_id: service_id.to_owned(),
            version: "0.1.0".to_owned(),
            contract_version: SERVICE_CONTRACT_VERSION,
            capabilities: vec![ServiceCapabilityDescriptor {
                name: cap_name.to_owned(),
                description: "Sample capability".to_owned(),
                input_schema: serde_json::json!({"type": "object"}),
                required_permissions: vec!["media.generate".to_owned()],
                supports_streaming: false,
            }],
        }
    }

    #[tokio::test]
    async fn registers_and_invokes_capability() {
        let registry = ModuleRegistry::new();
        registry
            .register(
                sample_descriptor("media-voice", "media.generate_image"),
                Arc::new(MockTransport),
            )
            .expect("register module");

        let result = registry
            .invoke(
                "req-1",
                "media.generate_image",
                serde_json::json!({"prompt": "cat"}),
                None,
                |perms| {
                    assert_eq!(perms, &["media.generate"]);
                    Ok(())
                },
            )
            .await
            .expect("invoke succeeds");

        assert_eq!(result["handled_by"], "media.generate_image");
        assert_eq!(result["echo"]["prompt"], "cat");
    }

    #[tokio::test]
    async fn unavailable_module_returns_typed_unavailable() {
        let registry = ModuleRegistry::new();
        registry
            .register(
                sample_descriptor("evals", "evals.run"),
                Arc::new(MockTransport),
            )
            .expect("register");
        registry
            .set_availability(
                "evals",
                ModuleAvailability::Unavailable {
                    reason: "restarting".to_owned(),
                },
            )
            .expect("set unavailable");

        let err = registry
            .invoke("req-2", "evals.run", serde_json::json!({}), None, |_| Ok(()))
            .await
            .expect_err("must fail with unavailable");
        assert_eq!(err.code, ServiceErrorCode::Unavailable);
    }

    #[tokio::test]
    async fn permission_denial_blocks_dispatch() {
        let registry = ModuleRegistry::new();
        registry
            .register(
                sample_descriptor("research", "research.fetch"),
                Arc::new(MockTransport),
            )
            .expect("register");

        let err = registry
            .invoke(
                "req-3",
                "research.fetch",
                serde_json::json!({}),
                None,
                |_| Err("denied by policy".to_owned()),
            )
            .await
            .expect_err("must fail with permission denied");
        assert_eq!(err.code, ServiceErrorCode::PermissionDenied);
    }
}
