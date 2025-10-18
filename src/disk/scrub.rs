use anyhow::Result;
use async_trait::async_trait;
use chrono::Duration;
use indexmap::IndexMap;

use crate::{
    disk::btrfs::get_btrfs_scrub_date,
    monitor::{Alert, Monitor, Severity},
};

#[derive(Debug)]
pub struct DiskScrubMonitor {
    mount_point: String,
    time_between_scrubs: Duration,
}

impl DiskScrubMonitor {
    pub fn new(mount_point: &str, time_between_scrubs: Duration) -> Self {
        Self {
            mount_point: mount_point.to_string(),
            time_between_scrubs,
        }
    }
}

#[async_trait]
impl Monitor for DiskScrubMonitor {
    fn name(&self) -> String {
        format!("BTRFS Scrub Needed: {}", self.mount_point)
    }

    async fn run(&mut self) -> Result<Option<Alert>> {
        let id = &format!("BTRFS Scrub Needed: {}", self.mount_point);
        let now = chrono::Utc::now();

        match get_btrfs_scrub_date(&self.mount_point)? {
            Some(scrub_date) => {
                let duration_since = now.signed_duration_since(scrub_date);
                if duration_since < self.time_between_scrubs {
                    return Ok(None);
                }

                Ok(Some(Alert::new(
                    id,
                    &format!(
                        "It's been {} days since the last BTRFS scrub. Run `sudo btrfs scrub start '{}'` on the system",
                        now.signed_duration_since(scrub_date).num_days(),
                        self.mount_point
                    ),
                    Severity::Warn,
                    IndexMap::from([("Last Scrub".to_string(), scrub_date.to_string())]),
                )))
            }
            // Never scrubbed
            None => Ok(Some(Alert::new(
                id,
                &format!(
                    "No BTRFS scrub found for {}. Run `sudo btrfs scrub start '{}'` on the system",
                    self.mount_point, self.mount_point
                ),
                Severity::Warn,
                IndexMap::new(),
            ))),
        }
    }
}
