use std::path::PathBuf;

use anyhow::{Context, Result};
use async_trait::async_trait;
use indexmap::IndexMap;
use serde::Deserialize;
use sysinfo::Disks;

use crate::monitor::{Alert, Monitor, Severity};

#[derive(Debug, Deserialize)]
pub struct DiskUsageConfig {
    pub mount_point: PathBuf,
    pub threshold: f64,
}

#[derive(Debug)]
pub struct DiskUsageMonitor {
    mount_point: String,
    alert_threshold: f64,
}

impl DiskUsageMonitor {
    pub fn new(config: DiskUsageConfig) -> Self {
        Self {
            mount_point: config.mount_point.to_string_lossy().to_string(),
            alert_threshold: config.threshold,
        }
    }
}

#[async_trait]
impl Monitor for DiskUsageMonitor {
    fn name(&self) -> String {
        format!("Disk Usage: {}", self.mount_point)
    }

    async fn run(&mut self) -> Result<Option<Alert>> {
        let disks = Disks::new_with_refreshed_list();

        let disk = disks
            .iter()
            .find(|d| d.mount_point().to_str() == Some(&self.mount_point))
            .context(format!(
                "Disk not found at mount point '{}'",
                self.mount_point
            ))?;

        let usage_percent = 1. - (disk.available_space() as f64 / disk.total_space() as f64);
        if usage_percent < self.alert_threshold {
            return Ok(None);
        }

        Ok(Some(Alert::new(
            &format!("Disk Usage: {}", self.mount_point),
            &format!(
                "Disk usage above threshold of {}% for mount point '{}'",
                self.alert_threshold * 100.,
                self.mount_point
            ),
            Severity::Critical,
            IndexMap::from([
                (
                    "Current Usage".to_string(),
                    format!("{:.2}%", usage_percent * 100.),
                ),
                (
                    "Total Space (GBs)".to_string(),
                    format!("{:.2}", disk.total_space() as f64 / 1024. / 1024. / 1024.),
                ),
                (
                    "Available Space (GBs)".to_string(),
                    format!(
                        "{:.2}",
                        disk.available_space() as f64 / 1024. / 1024. / 1024.
                    ),
                ),
            ]),
        )))
    }
}
