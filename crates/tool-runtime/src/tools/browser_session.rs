//! Stable browser-session tool identifiers kept for compatibility diagnostics.
//!
//! The typed Core IPC browser surface is authoritative. The former raw CDP
//! implementation was unreachable from the production router and is removed;
//! these names now fail closed while callers migrate to the typed surface.

use crate::{ToolContext, ToolError, ToolResult};
use evohime_permissions::Permission;
use serde_json::Value;
use std::time::Duration;

pub const NAVIGATE_NAME: &str = "browser_session_navigate";
pub const NAVIGATE_DESCRIPTION: &str =
    "Navigate the task's persistent browser tab to a URL (CDP session reuse)";
pub const NAVIGATE_PERMISSIONS: &[Permission] = &[Permission::BrowserAccess];
pub const NAVIGATE_TIMEOUT: Duration = Duration::from_secs(30);

pub const READ_NAME: &str = "browser_session_read";
pub const READ_DESCRIPTION: &str =
    "Read the current page of the task's browser tab without re-navigating";
pub const READ_PERMISSIONS: &[Permission] = &[Permission::BrowserAccess];
pub const READ_TIMEOUT: Duration = Duration::from_secs(30);

pub const CLICK_NAME: &str = "browser_session_click";
pub const CLICK_DESCRIPTION: &str =
    "Click a CSS selector in the task's browser tab and report the resulting page";
pub const CLICK_PERMISSIONS: &[Permission] = &[Permission::BrowserAccess];
pub const CLICK_TIMEOUT: Duration = Duration::from_secs(30);

pub const SCREENSHOT_NAME: &str = "browser_session_screenshot";
pub const SCREENSHOT_DESCRIPTION: &str =
    "Save a PNG screenshot of the task's browser tab into the workspace";
pub const SCREENSHOT_PERMISSIONS: &[Permission] = &[Permission::BrowserAccess];
pub const SCREENSHOT_TIMEOUT: Duration = Duration::from_secs(30);

pub const TYPE_NAME: &str = "browser_session_type";
pub const TYPE_DESCRIPTION: &str =
    "Type text into a CSS selector in the task's browser tab (input/change dispatched)";
pub const TYPE_PERMISSIONS: &[Permission] = &[Permission::BrowserAccess];
pub const TYPE_TIMEOUT: Duration = Duration::from_secs(30);

pub const CLOSE_NAME: &str = "browser_session_close";
pub const CLOSE_DESCRIPTION: &str = "Close the task's persistent browser tab";
pub const CLOSE_PERMISSIONS: &[Permission] = &[Permission::BrowserAccess];

/// Identifiers of the retired raw-CDP browser tools.
///
/// They stay registered so stored conversations and legacy text calls receive
/// the typed `legacy_disabled` refusal, but Core must not advertise them to the
/// model: a tool that always fails is worse than an absent one.
pub const RETIRED_NAMES: &[&str] = &[
    NAVIGATE_NAME,
    READ_NAME,
    CLICK_NAME,
    SCREENSHOT_NAME,
    TYPE_NAME,
    CLOSE_NAME,
];
pub const CLOSE_TIMEOUT: Duration = Duration::from_secs(15);

/// Return a stable fail-closed error for the retired legacy tool names.
pub async fn legacy_disabled(_ctx: &ToolContext, _input: Value) -> Result<ToolResult, ToolError> {
    Err(ToolError::Execution("legacy_disabled".into()))
}
