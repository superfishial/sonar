use anyhow::{Result, ensure};
use async_trait::async_trait;
use chrono::Duration;
use indexmap::IndexMap;
use serde::Deserialize;
use serde_json::Value;
use std::process::Command;
use tracing::warn;

use crate::monitor::{Alert, Monitor, Severity};

#[derive(Debug, Deserialize)]
pub struct AutoRestartConfig {
    #[serde(with = "humantime_serde")]
    tailscale_restart_duration_threshold: std::time::Duration,
    #[serde(with = "humantime_serde")]
    system_restart_duration_threshold: std::time::Duration,
}

#[derive(Debug)]
pub struct AutoRestart {
    tailscale_restart_duration_threshold: Duration,
    system_restart_duration_threshold: Duration,

    triggered_at: Option<chrono::DateTime<chrono::Utc>>,
    tailscale_restarted: bool,
}

impl AutoRestart {
    pub fn new(config: AutoRestartConfig) -> Self {
        println!("AutoRestartConfig: {:?}", config);
        Self {
            tailscale_restart_duration_threshold: Duration::seconds(
                config.tailscale_restart_duration_threshold.as_secs() as i64,
            ),
            system_restart_duration_threshold: Duration::seconds(
                config.system_restart_duration_threshold.as_secs() as i64,
            ),
            triggered_at: None,
            tailscale_restarted: false,
        }
    }

    fn is_tailscale_connected() -> Result<bool> {
        let output = Command::new("tailscale")
            .args(["status", "--json"])
            .output()?;
        ensure!(output.status.success(), "Failed to check tailscale status");

        let stdout = String::from_utf8(output.stdout)?;
        let json: Value = serde_json::from_str(&stdout).unwrap_or_else(|err| {
            warn!("Failed to parse JSON for tailscale status: {err}");
            Value::Null
        });

        Ok(json
            .get("Self")
            .and_then(|self_json| self_json.get("Online"))
            .and_then(|online| online.as_bool())
            .unwrap_or_else(|| {
                warn!("Coudn't find Self.Online field in tailscale status JSON");
                false
            }))
    }

    fn restart_tailscale() -> Result<()> {
        let output = Command::new("tailscale").args(["down"]).output()?;
        if !output.status.success() {
            return Err(anyhow::anyhow!("Failed to restart tailscale: {:?}", output));
        }
        let output = Command::new("tailscale").args(["up"]).output()?;
        if !output.status.success() {
            return Err(anyhow::anyhow!("Failed to restart tailscale: {:?}", output));
        }
        Ok(())
    }
}

#[async_trait]
impl Monitor for AutoRestart {
    fn name(&self) -> String {
        "Auto Restart".to_string()
    }

    async fn run(&mut self) -> Result<Option<Alert>> {
        let is_tailscale_connected = Self::is_tailscale_connected()?;
        if is_tailscale_connected {
            self.triggered_at = None;
            self.tailscale_restarted = false;
            return Ok(None);
        }

        if self.triggered_at.is_none() {
            self.triggered_at = Some(chrono::Utc::now());

            return Ok(Some(Alert::new(
                "Auto Restart - Tailscale Disconnected",
                "Tailscale status reports that the server has been disconnected from the network. Attempting to restart Tailscale...",
                Severity::Critical,
                IndexMap::new(),
            )));
        }

        if let Some(triggered_at) = self.triggered_at
            && !self.tailscale_restarted
            && triggered_at + self.tailscale_restart_duration_threshold < chrono::Utc::now()
        {
            Self::restart_tailscale()?;
            self.tailscale_restarted = true;
        }

        if let Some(triggered_at) = self.triggered_at
            && self.tailscale_restarted
            && triggered_at + self.system_restart_duration_threshold < chrono::Utc::now()
        {
            let output = Command::new("shutdown").args(["-r", "now"]).output()?;
            if !output.status.success() {
                return Err(anyhow::anyhow!("Failed to restart system: {:?}", output));
            }
        }

        Ok(None)
    }
}
