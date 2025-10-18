use anyhow::{Context, Result};
use async_trait::async_trait;
use chrono::{DateTime, Duration, Utc};
use indexmap::IndexMap;
use regex::Regex;
use serde::Deserialize;
use std::process::Command;

use crate::monitor::{Alert, Monitor, Severity};

#[derive(Debug)]
struct OOMKill {
    timestamp: DateTime<Utc>,
    process_name: String,
}

#[derive(Debug, Deserialize)]
pub struct MemoryOOMConfig {
    #[serde(with = "humantime_serde")]
    within_last: std::time::Duration,
}

#[derive(Debug)]
pub struct MemoryOOMMonitor {
    within_last: chrono::Duration,
}

impl MemoryOOMMonitor {
    pub fn new(config: MemoryOOMConfig) -> Self {
        Self {
            within_last: Duration::seconds(config.within_last.as_secs() as i64),
        }
    }

    fn get_oom_kills_from_journald() -> Result<Vec<OOMKill>> {
        let output = Command::new("journalctl")
            .args([
                "-k",                    // Kernel messages
                "--grep=Killed process", // Filter for OOM kills
                "--output=json",         // JSON output with metadata
                "--no-pager",
            ])
            .output()?;

        let stdout = String::from_utf8(output.stdout)?;
        let re = Regex::new(r"Killed process (\d+) \(([^)]+)\)")?;

        let mut kills = Vec::new();

        for line in stdout.lines() {
            let json: serde_json::Value = serde_json::from_str(line)?;

            if let Some(message) = json["MESSAGE"].as_str()
                && let Some(caps) = re.captures(message)
                // __REALTIME_TIMESTAMP is in microseconds
                && let Some(timestamp_us) = json["__REALTIME_TIMESTAMP"].as_str()
            {
                let timestamp_us: i64 = timestamp_us.parse()?;
                let timestamp = DateTime::from_timestamp(
                    timestamp_us / 1_000_000,
                    ((timestamp_us % 1_000_000) * 1000) as u32,
                )
                .context(format!("Failed to parse timestamp: {}", timestamp_us))?;

                kills.push(OOMKill {
                    timestamp,
                    process_name: caps[2].to_string(),
                });
            }
        }

        Ok(kills)
    }
}

#[async_trait]
impl Monitor for MemoryOOMMonitor {
    fn name(&self) -> String {
        "Memory OOM Kills".to_string()
    }

    async fn run(&mut self) -> Result<Option<Alert>> {
        let oom_events = Self::get_oom_kills_from_journald()?
            .into_iter()
            // Within the last 30 minutes
            .filter(|kill| kill.timestamp > chrono::Utc::now() - chrono::Duration::minutes(30))
            .collect::<Vec<_>>();
        if oom_events.is_empty() {
            return Ok(None);
        }

        Ok(Some(Alert::new(
            "OOM Kills",
            &format!(
                "One or more processes have been killed due to an Out of Memory (OOM) condition in the last {} minutes. Check the system logs for more information.",
                self.within_last.num_minutes()
            ),
            Severity::Critical,
            IndexMap::from([
                ("OOM Kills".to_string(), oom_events.len().to_string()),
                (
                    "Processes".to_string(),
                    oom_events
                        .iter()
                        .map(|k| k.process_name.clone())
                        .collect::<Vec<_>>()
                        .join(", "),
                ),
            ]),
        )))
    }
}
