use chrono::{DateTime, TimeZone, Utc};
use indexmap::IndexMap;
use std::ops::Add;
use std::process::Command;

use anyhow::{Context, Result, bail, ensure};

#[derive(Debug, Default, Clone)]
pub struct BtrfsStats {
    pub write_io_errs: u64,
    pub read_io_errs: u64,
    pub flush_io_errs: u64,
    pub corruption_errs: u64,
    pub generation_errs: u64,
}

impl Add for BtrfsStats {
    type Output = Self;

    fn add(self, rhs: Self) -> Self::Output {
        Self {
            write_io_errs: self.write_io_errs + rhs.write_io_errs,
            read_io_errs: self.read_io_errs + rhs.read_io_errs,
            flush_io_errs: self.flush_io_errs + rhs.flush_io_errs,
            corruption_errs: self.corruption_errs + rhs.corruption_errs,
            generation_errs: self.generation_errs + rhs.generation_errs,
        }
    }
}

impl BtrfsStats {
    pub fn is_ok(&self) -> bool {
        self.write_io_errs == 0
            && self.read_io_errs == 0
            && self.flush_io_errs == 0
            && self.corruption_errs == 0
            && self.generation_errs == 0
    }
}

pub fn get_btrfs_device_stats(mount_point: &str) -> Result<IndexMap<String, BtrfsStats>> {
    let output = Command::new("btrfs")
        .arg("device")
        .arg("stats")
        .arg(mount_point)
        .output()
        .context(format!(
            "Failed to execute btrfs command: btrfs device stats '{}'",
            mount_point
        ))?;

    ensure!(
        output.status.success(),
        "btrfs command failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    let stdout = String::from_utf8_lossy(&output.stdout);
    parse_btrfs_stats(&stdout)
}

fn parse_btrfs_stats(output: &str) -> Result<IndexMap<String, BtrfsStats>> {
    let mut stats_map: IndexMap<String, BtrfsStats> = IndexMap::new();

    for line in output.lines() {
        let line = line.trim();
        if line.is_empty() {
            continue;
        }

        // Parse line format: [/dev/device].stat_name    value
        let parts: Vec<&str> = line.split_whitespace().collect();
        if parts.len() != 2 {
            continue;
        }

        let key_part = parts[0];
        let value = parts[1]
            .parse::<u64>()
            .context(format!("Failed to parse value as u64: '{}'", parts[1]))?;

        // Extract device and stat name
        if let Some(dot_pos) = key_part.rfind('.') {
            let device_part = &key_part[..dot_pos];
            let stat_name = &key_part[dot_pos + 1..];

            // Remove brackets from device name
            let device = device_part
                .trim_start_matches('[')
                .trim_end_matches(']')
                .to_string();

            // Get or create the stats entry for this device
            let stats = stats_map.entry(device).or_default();

            // Update the appropriate field
            match stat_name {
                "write_io_errs" => stats.write_io_errs = value,
                "read_io_errs" => stats.read_io_errs = value,
                "flush_io_errs" => stats.flush_io_errs = value,
                "corruption_errs" => stats.corruption_errs = value,
                "generation_errs" => stats.generation_errs = value,
                _ => bail!("Unknown stat name: {}", stat_name),
            }
        }
    }

    Ok(stats_map)
}

pub fn get_btrfs_scrub_date(mount_point: &str) -> Result<Option<DateTime<Utc>>> {
    let output = Command::new("btrfs")
        .arg("scrub")
        .arg("status")
        .arg(mount_point)
        .output()
        .context(format!(
            "Failed to execute btrfs command: btrfs scrub status '{}'",
            mount_point
        ))?;

    ensure!(
        output.status.success(),
        "btrfs command failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    let stdout = String::from_utf8_lossy(&output.stdout);
    parse_btrfs_scrub_date(&stdout)
}

fn parse_btrfs_scrub_date(output: &str) -> Result<Option<DateTime<Utc>>> {
    for line in output.lines() {
        if let Some(date_str) = line.strip_prefix("Scrub started:").map(|s| s.trim()) {
            // Parse the date string (format: "Sat Oct 18 09:42:43 2025")
            let parsed = chrono::NaiveDateTime::parse_from_str(date_str, "%a %b %d %H:%M:%S %Y")
                .context(format!("Failed to parse scrub date: '{}'", date_str))?;

            // Assume local timezone and convert to UTC
            let local = chrono::Local::now().timezone();
            let datetime = local
                .from_local_datetime(&parsed)
                .single()
                .context("Ambiguous or invalid local datetime")?
                .with_timezone(&Utc);

            return Ok(Some(datetime));
        }
    }

    Ok(None)
}
