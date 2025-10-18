use anyhow::Result;
use async_trait::async_trait;
use chrono::Duration;
use indexmap::IndexMap;
use sysinfo::{CpuRefreshKind, RefreshKind, System};

use crate::monitor::{Alert, Monitor, Severity};

#[derive(Debug)]
pub struct CpuUsageMonitor {
    alert_threshold: f32,
    duration_threshold: Duration,

    over_limit_at: Option<chrono::DateTime<chrono::Utc>>,
    system: System,
}

impl CpuUsageMonitor {
    pub fn new(alert_threshold: f32, duration_threshold: Duration) -> Self {
        let system = System::new_with_specifics(
            RefreshKind::nothing().with_cpu(CpuRefreshKind::nothing().with_cpu_usage()),
        );
        Self {
            alert_threshold,
            duration_threshold,

            over_limit_at: None,
            system,
        }
    }
}

#[async_trait]
impl Monitor for CpuUsageMonitor {
    fn name(&self) -> String {
        "CPU Usage".to_string()
    }

    async fn run(&mut self) -> Result<Option<Alert>> {
        self.system.refresh_cpu_usage();
        let usage_percent = self.system.global_cpu_usage();

        if usage_percent < self.alert_threshold {
            self.over_limit_at = None;
            return Ok(None);
        }
        self.over_limit_at = self.over_limit_at.or(Some(chrono::Utc::now()));

        // TODO: get processes using the most CPU
        if let Some(over_limit_at) = self.over_limit_at {
            let duration_mins = chrono::Utc::now() - over_limit_at;
            if duration_mins > self.duration_threshold {
                return Ok(Some(Alert::new(
                    "CPU Usage",
                    &format!(
                        "CPU usage has been above {}% for {} minutes",
                        self.alert_threshold,
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
