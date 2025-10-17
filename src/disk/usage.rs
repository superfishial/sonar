use anyhow::{Context, Result};
use async_trait::async_trait;
use indexmap::IndexMap;
use sysinfo::Disks;

use crate::monitor::{Alert, Monitor, Severity};

#[derive(Debug)]
pub struct DiskUsageMonitor {
    alert_threshold: f64,
    mount_point: String,
}

impl DiskUsageMonitor {
    pub fn new(alert_threshold: f64, mount_point: &str) -> Self {
        Self {
            alert_threshold,
            mount_point: mount_point.to_string(),
        }
    }
}

#[async_trait]
impl Monitor for DiskUsageMonitor {
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
