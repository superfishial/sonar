// TODO: OOM kills

use anyhow::Result;
use async_trait::async_trait;
use chrono::Duration;
use indexmap::IndexMap;
use serde::Deserialize;
use sysinfo::{MemoryRefreshKind, RefreshKind, System};

use crate::monitor::{Alert, Monitor, Severity};

#[derive(Debug, Deserialize)]
pub struct MemoryConfig {
    pub critical_threshold: f64,
    pub warn_threshold: f64,
    #[serde(with = "humantime_serde")]
    pub duration: std::time::Duration,
}

#[derive(Debug)]
pub struct MemoryUsageMonitor {
    critical_alert_threshold: f64,
    warn_alert_threshold: f64,
    duration_threshold: Duration,

    over_limit_at: Option<chrono::DateTime<chrono::Utc>>,
    system: System,
}

impl MemoryUsageMonitor {
    pub fn new(config: MemoryConfig) -> Self {
        let system = System::new_with_specifics(
            RefreshKind::nothing().with_memory(MemoryRefreshKind::nothing().with_ram()),
        );
        Self {
            critical_alert_threshold: config.critical_threshold,
            warn_alert_threshold: config.warn_threshold,
            duration_threshold: Duration::seconds(config.duration.as_secs() as i64),

            over_limit_at: None,
            system,
        }
    }
}

#[async_trait]
impl Monitor for MemoryUsageMonitor {
    fn name(&self) -> String {
        "Memory Usage".to_string()
    }

    async fn run(&mut self) -> Result<Option<Alert>> {
        self.system.refresh_memory();
        let usage_percent =
            (self.system.used_memory() as f64) / (self.system.total_memory() as f64);

        if usage_percent < self.warn_alert_threshold {
            self.over_limit_at = None;
            return Ok(None);
        }
        self.over_limit_at = self.over_limit_at.or(Some(chrono::Utc::now()));

        // Immediately alert if hitting critical threshold
        if usage_percent > self.critical_alert_threshold {
            return Ok(Some(Alert::new(
                "Memory Usage (Fatal)",
                &format!(
                    "Memory usage above critical threshold of {}%",
                    self.critical_alert_threshold * 100.
                ),
                Severity::Critical,
                IndexMap::from([
                    (
                        "Current Usage".to_string(),
                        format!("{}%", usage_percent * 100.),
                    ),
                    (
                        "Duration".to_string(),
                        format!("{} minutes", self.duration_threshold.num_minutes()),
                    ),
                ]),
            )));
        }

        // Alert after some time if usage hits warning threshold
        if let Some(over_limit_at) = self.over_limit_at {
            let duration_mins = chrono::Utc::now() - over_limit_at;
            if duration_mins > self.duration_threshold {
                return Ok(Some(Alert::new(
                    "Memory Usage",
                    &format!(
                        "Memory usage has been above {}% for {} minutes",
                        self.warn_alert_threshold * 100.,
                        duration_mins.num_minutes()
                    ),
                    Severity::Warn,
                    IndexMap::from([
                        (
                            "Current Usage".to_string(),
                            format!("{}%", usage_percent * 100.),
                        ),
                        (
                            "Duration".to_string(),
                            format!("{} minutes", duration_mins.num_minutes()),
                        ),
                    ]),
                )));
            }
        }

        Ok(None)
    }
}
