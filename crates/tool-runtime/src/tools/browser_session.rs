//! Stable browser-session tool identifiers kept for compatibility diagnostics.
//!
//! The typed Core IPC browser surface is authoritative. The former raw CDP
//! implementation was unreachable from the production router and is removed;
//! these names now fail closed while callers migrate to the typed surface.

use crate::{ToolContext, ToolError, ToolResult};
use evohime_permissions::Permission;
use serde_json::Value;
use std::time::Duration;

pub const NAVIGATE_NAME: &str = "browser.session.navigate";
pub const NAVIGATE_DESCRIPTION: &str =
    "Navigate the task's persistent browser tab to a URL (CDP session reuse)";
pub const NAVIGATE_PERMISSIONS: &[Permission] = &[Permission::BrowserAccess];
pub const NAVIGATE_TIMEOUT: Duration = Duration::from_secs(30);

pub const READ_NAME: &str = "browser.session.read";
pub const READ_DESCRIPTION: &str =
    "Read the current page of the task's browser tab without re-navigating";
pub const READ_PERMISSIONS: &[Permission] = &[Permission::BrowserAccess];
pub const READ_TIMEOUT: Duration = Duration::from_secs(30);

pub const CLICK_NAME: &str = "browser.session.click";
pub const CLICK_DESCRIPTION: &str =
    "Click a CSS selector in the task's browser tab and report the resulting page";
pub const CLICK_PERMISSIONS: &[Permission] = &[Permission::BrowserAccess];
pub const CLICK_TIMEOUT: Duration = Duration::from_secs(30);

pub const SCREENSHOT_NAME: &str = "browser.session.screenshot";
pub const SCREENSHOT_DESCRIPTION: &str =
    "Save a PNG screenshot of the task's browser tab into the workspace";
pub const SCREENSHOT_PERMISSIONS: &[Permission] = &[Permission::BrowserAccess];
pub const SCREENSHOT_TIMEOUT: Duration = Duration::from_secs(30);

pub const TYPE_NAME: &str = "browser.session.type";
pub const TYPE_DESCRIPTION: &str =
    "Type text into a CSS selector in the task's browser tab (input/change dispatched)";
pub const TYPE_PERMISSIONS: &[Permission] = &[Permission::BrowserAccess];
pub const TYPE_TIMEOUT: Duration = Duration::from_secs(30);

pub const CLOSE_NAME: &str = "browser.session.close";
pub const CLOSE_DESCRIPTION: &str = "Close the task's persistent browser tab";
pub const CLOSE_PERMISSIONS: &[Permission] = &[Permission::BrowserAccess];
pub const CLOSE_TIMEOUT: Duration = Duration::from_secs(15);

/// Return a stable fail-closed error for the retired legacy tool names.
pub async fn legacy_disabled(_ctx: &ToolContext, _input: Value) -> Result<ToolResult, ToolError> {
    Err(ToolError::Execution("legacy_disabled".into()))
}
