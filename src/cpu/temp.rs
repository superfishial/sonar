use anyhow::{Context, Result};
use async_trait::async_trait;
use chrono::Duration;
use indexmap::IndexMap;
use sysinfo::Components;

use crate::monitor::{Alert, Monitor, Severity};

#[derive(Debug)]
pub struct CpuTempMonitor {
    alert_threshold: f32,
    duration_threshold: Duration,

    over_limit_at: Option<chrono::DateTime<chrono::Utc>>,
    components: Components,
}

impl CpuTempMonitor {
    pub fn new(alert_threshold: f32, duration_threshold: Duration) -> Self {
        Self {
            alert_threshold,
            duration_threshold,

            over_limit_at: None,
            components: Components::new(),
        }
    }
}

#[async_trait]
impl Monitor for CpuTempMonitor {
    fn name(&self) -> String {
        "CPU Temperature".to_string()
    }

    async fn run(&mut self) -> Result<Option<Alert>> {
        self.components.refresh(true);
        let temperatures = self.components.list();
        let cpu_temp = temperatures
            .iter()
            // NOTE: Only works for AMD Zen CPUs which is fine for my use case
            // Tctl = Control Temperature
            // Tccd1 = Core Complex Die 1
            // Tccd2 = Core Complex Die 2
            .find(|t| t.label() == "k10temp Tctl")
            .context("CPU temperature not found. Are you using an AMD Zen CPU?")?
            .temperature()
            .context("Could not read CPU temperature")?;

        if cpu_temp < self.alert_threshold {
            self.over_limit_at = None;
            return Ok(None);
        }
        self.over_limit_at = self.over_limit_at.or(Some(chrono::Utc::now()));

        if let Some(over_limit_at) = self.over_limit_at {
            let duration_mins = chrono::Utc::now() - over_limit_at;
            if duration_mins > self.duration_threshold {
                return Ok(Some(Alert::new(
                    "CPU Temperature",
                    &format!(
                        "CPU temperature has been above {}°C for {} minutes",
                        self.alert_threshold,
                        duration_mins.num_minutes()
                    ),
                    Severity::Warn,
                    IndexMap::from([
                        ("Current Usage".to_string(), format!("{}°C", cpu_temp)),
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
