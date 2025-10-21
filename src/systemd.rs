use anyhow::{Context, Result};
use async_trait::async_trait;
use chrono::{DateTime, Duration, NaiveDateTime, Utc};
use indexmap::IndexMap;
use serde::Deserialize;
use std::process::Command;

use crate::monitor::{Alert, Monitor, Severity};

#[derive(Debug, Deserialize)]
pub struct SystemdConfig {
    pub unit: String,
    #[serde(with = "humantime_serde")]
    pub max_time_since: std::time::Duration,
}

#[derive(Debug)]
struct ServiceStatus {
    last_run: Option<DateTime<Utc>>,
    last_status: Option<String>,
    is_failed: bool,
}

#[derive(Debug)]
pub struct SystemdServiceMonitor {
    unit: String,
    max_time_since: Duration,
}

impl SystemdServiceMonitor {
    pub fn new(config: SystemdConfig) -> Self {
        Self {
            unit: config.unit,
            max_time_since: Duration::seconds(config.max_time_since.as_secs() as i64),
        }
    }

    fn get_service_status(&self) -> Result<ServiceStatus> {
        // Get the active state
        let active_state = Command::new("systemctl")
            .args(["show", &self.unit, "--property=ActiveState", "--value"])
            .output()
            .context("Failed to execute systemctl show")?;

        let active_state_str = String::from_utf8_lossy(&active_state.stdout)
            .trim()
            .to_string();
        let is_failed = active_state_str == "failed";

        // Get the last execution timestamp
        let exec_time = Command::new("systemctl")
            .args([
                "show",
                &self.unit,
                "--property=ExecMainStartTimestamp",
                "--value",
                "--timestamp=utc",
            ])
            .output()
            .context("Failed to get service timestamp")?;

        let timestamp_str = String::from_utf8_lossy(&exec_time.stdout)
            .trim()
            .to_string();

        // Parse systemd timestamp (format: "Day YYYY-MM-DD HH:MM:SS TZ")
        let last_run = (!timestamp_str.is_empty() && !timestamp_str.eq("n/a"))
            .then(|| {
                NaiveDateTime::parse_from_str(&timestamp_str, "%a %Y-%m-%d %H:%M:%S %Z")
                    .context(format!(
                        "Failed to parse systemd timestamp: '{}'",
                        timestamp_str
                    ))
                    .map(|dt| dt.and_utc())
            })
            .transpose()?;

        let last_status = (!active_state_str.is_empty()).then_some(active_state_str);

        Ok(ServiceStatus {
            last_run,
            last_status,
            is_failed,
        })
    }
}

#[async_trait]
impl Monitor for SystemdServiceMonitor {
    fn name(&self) -> String {
        format!("Systemd Service: {}", self.unit)
    }

    async fn run(&mut self) -> Result<Option<Alert>> {
        let id = &format!("Systemd Service: {}", self.unit);
        let now = Utc::now();

        let status = self.get_service_status()?;

        // Check if last run failed
        if status.is_failed {
            return Ok(Some(Alert::new(
                id,
                &format!(
                    "Service '{}' last invocation failed. Check status with `systemctl status {}`",
                    self.unit, self.unit
                ),
                Severity::Warn,
                IndexMap::from([
                    ("Service".to_string(), self.unit.clone()),
                    (
                        "Status".to_string(),
                        status.last_status.unwrap_or_else(|| "failed".to_string()),
                    ),
                    (
                        "Last Run".to_string(),
                        status
                            .last_run
                            .map(|d| d.to_string())
                            .unwrap_or_else(|| "unknown".to_string()),
                    ),
                ]),
            )));
        }

        // Check if it hasn't run recently enough
        match status.last_run {
            Some(last_run) => {
                let duration_since = now.signed_duration_since(last_run);
                if duration_since > self.max_time_since {
                    let should_use_days = self.max_time_since.num_days() > 0;

                    Ok(Some(Alert::new(
                        id,
                        &format!(
                            "Service '{}' hasn't run in {} (expected every {}). Check with `systemctl status {}`",
                            self.unit,
                            if should_use_days {
                                format!("{} days", duration_since.num_days())
                            } else {
                                format!("{} minutes", duration_since.num_minutes())
                            },
                            if should_use_days {
                                format!("{} days", self.max_time_since.num_days())
                            } else {
                                format!("{} minutes", self.max_time_since.num_minutes())
                            },
                            self.unit
                        ),
                        Severity::Warn,
                        IndexMap::from([
                            ("Service".to_string(), self.unit.clone()),
                            ("Last Run".to_string(), last_run.to_string()),
                            (
                                "Days Since".to_string(),
                                duration_since.num_days().to_string(),
                            ),
                        ]),
                    )))
                } else {
                    Ok(None)
                }
            }
            None => Ok(Some(Alert::new(
                id,
                &format!(
                    "Service '{}' has never run. Check with `systemctl status {}`",
                    self.unit, self.unit
                ),
                Severity::Warn,
                IndexMap::from([("Service".to_string(), self.unit.clone())]),
            ))),
        }
    }
}
