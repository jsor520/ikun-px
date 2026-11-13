//! Stage reporting for the connect and disconnect lifecycles.
//!
//! Connecting runs a different number of stages depending on which
//! integrations are enabled, so the total is computed by the caller and each
//! stage is announced before its work begins. That ordering lets the UI treat
//! the most recent stage as the failing one when a command rejects, without
//! every error path having to report itself.

use std::sync::atomic::{AtomicU8, Ordering};

use serde::Serialize;
use tauri::{AppHandle, Emitter};

/// Stages executed inside `start_proxy_runtime`.
pub const RUNTIME_STAGES: u8 = 7;

/// Stages executed inside `AppState::shutdown`, including the terminal one.
pub const SHUTDOWN_STAGES: u8 = 5;

const EVENT: &str = "vpn-progress";

#[derive(Debug, Clone, Copy, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Direction {
    Connect,
    Disconnect,
}

#[derive(Debug, Clone, Serialize)]
struct Payload {
    direction: Direction,
    phase: &'static str,
    label: &'static str,
    step: u8,
    total: u8,
}

pub struct Reporter {
    app: Option<AppHandle>,
    direction: Direction,
    total: u8,
    step: AtomicU8,
}

impl Reporter {
    /// Restarts the user did not ask for — rule edits, domain routing changes
    /// and rollbacks — must not drive the hero button animation.
    pub fn silent() -> Self {
        Self {
            app: None,
            direction: Direction::Connect,
            total: 0,
            step: AtomicU8::new(0),
        }
    }

    pub fn new(app: AppHandle, direction: Direction, total: u8) -> Self {
        Self {
            app: Some(app),
            direction,
            total,
            step: AtomicU8::new(0),
        }
    }

    /// Announces the stage that is about to run.
    pub fn stage(&self, phase: &'static str, label: &'static str) {
        let Some(app) = &self.app else { return };
        let step = self.step.fetch_add(1, Ordering::Relaxed) + 1;
        let _ = app.emit(
            EVENT,
            Payload {
                direction: self.direction,
                phase,
                label,
                step: step.min(self.total),
                total: self.total,
            },
        );
    }

    /// Announces the terminal stage and pins progress at 100%.
    pub fn done(&self, phase: &'static str, label: &'static str) {
        let Some(app) = &self.app else { return };
        self.step.store(self.total, Ordering::Relaxed);
        let _ = app.emit(
            EVENT,
            Payload {
                direction: self.direction,
                phase,
                label,
                step: self.total,
                total: self.total,
            },
        );
    }
}
