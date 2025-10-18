use std::path::PathBuf;

use anyhow::Result;
use async_trait::async_trait;
use indexmap::IndexMap;
use serde::Deserialize;

use crate::{
    disk::btrfs::{BtrfsStats, get_btrfs_device_stats},
    monitor::{Alert, Monitor, Severity},
};

#[derive(Debug, Deserialize)]
pub struct DiskHealthConfig {
    pub mount_point: PathBuf,
}

#[derive(Debug)]
pub struct DiskHealthMonitor {
    mount_point: String,
}

impl DiskHealthMonitor {
    pub fn new(config: DiskHealthConfig) -> Self {
        Self {
            mount_point: config.mount_point.to_string_lossy().to_string(),
        }
    }
}

#[async_trait]
impl Monitor for DiskHealthMonitor {
    fn name(&self) -> String {
        format!("BTRFS Disk Health: {}", self.mount_point)
    }

    async fn run(&mut self) -> Result<Option<Alert>> {
        let per_device_stats = get_btrfs_device_stats(&self.mount_point)?;

        if !per_device_stats.values().any(|stats| !stats.is_ok()) {
            return Ok(None);
        }

        let devices_with_alerts = per_device_stats.keys().cloned().collect::<Vec<_>>();
        let stats = per_device_stats
            .values()
            .fold(BtrfsStats::default(), |acc, stats| acc + (*stats).clone());

        Ok(Some(Alert::new(
            &format!("BTRFS Disk Unhealthy: {}", self.mount_point),
            &format!(
                "One or more disks have errors thrown by btrfs at mount point '{}'",
                self.mount_point
            ),
            Severity::Critical,
            IndexMap::from([
                ("Devices".to_string(), devices_with_alerts.join(", ")),
                ("Write Errors".to_string(), stats.write_io_errs.to_string()),
                ("Read Errors".to_string(), stats.read_io_errs.to_string()),
                ("Flush Errors".to_string(), stats.flush_io_errs.to_string()),
                (
                    "Corruption Errors".to_string(),
                    stats.corruption_errs.to_string(),
                ),
                (
                    "Generation Errors".to_string(),
                    stats.generation_errs.to_string(),
                ),
            ]),
        )))
    }
}
