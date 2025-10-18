use std::collections::HashMap;

use anyhow::Result;
use chrono::Duration;
use clap::Parser;
use reqwest::Url;
use tracing::{error, info, warn};
use tracing_subscriber::EnvFilter;

use crate::{
    config::Config,
    cpu::{CpuTempMonitor, CpuUsageMonitor},
    disk::{DiskScrubMonitor, DiskStatsMonitor, DiskUsageMonitor},
    memory::MemoryUsageMonitor,
    monitor::{Alert, Monitor},
};

mod config;
mod cpu;
mod disk;
mod memory;
mod monitor;

#[tokio::main]
async fn main() {
    tracing_subscriber::fmt()
        .with_env_filter(EnvFilter::from_default_env().add_directive("info".parse().unwrap()))
        .with_target(false) // Hide module paths if you want
        .with_thread_ids(false)
        .with_level(true)
        .with_ansi(true)
        .init();

    // This returns an error if the `.env` file doesn't exist, which we ignore
    dotenv::dotenv().ok();

    let config = Config::parse();

    let mut monitors: Vec<Box<dyn Monitor>> = vec![
        // CPU
        Box::new(CpuTempMonitor::new(80., Duration::minutes(1))),
        Box::new(CpuUsageMonitor::new(25., Duration::hours(12))),
        Box::new(CpuUsageMonitor::new(75., Duration::minutes(30))),
        // Disks
        Box::new(DiskStatsMonitor::new("/mnt/data")),
        Box::new(DiskUsageMonitor::new("/data/hdd", 0.75)),
        Box::new(DiskUsageMonitor::new("/", 0.75)),
        Box::new(DiskScrubMonitor::new("/data/hdd", Duration::days(60))),
        Box::new(DiskScrubMonitor::new("/", Duration::days(60))),
        // Memory
        Box::new(MemoryUsageMonitor::new(0.9, 0.75, Duration::minutes(30))),
    ];

    let mut last_alert_send: HashMap<String, chrono::DateTime<chrono::Utc>> = HashMap::new();

    info!("Starting sonar");
    loop {
        info!("Running monitor loop...");
        for monitor in monitors.iter_mut() {
            match monitor.run().await {
                Ok(Some(alert)) => {
                    // Ensure X hours have passed since the last alert with this ID
                    let sufficient_time_since_last_alert = last_alert_send
                        .get(&alert.id)
                        .map(|last_send| {
                            (*last_send + chrono::Duration::hours(4)) < chrono::Utc::now()
                        })
                        .unwrap_or(true);

                    if sufficient_time_since_last_alert {
                        last_alert_send.insert(alert.id.clone(), chrono::Utc::now());
                        send_alert(&config.discord_webhook_url, &alert)
                            .await
                            .unwrap();
                    } else {
                        info!(
                            "Not sending alert '{}' as it was sent less than 4 hours ago",
                            alert.id
                        );
                    }
                }
                Err(e) => error!("Error running monitor {:?}: {}", monitor, e),
                _ => {}
            }
        }
        info!("Monitor loop complete");
        tokio::time::sleep(std::time::Duration::from_millis(config.polling_interval_ms)).await;
    }
}

async fn send_alert(webhook_url: &Url, alert: &Alert) -> Result<()> {
    warn!(id = alert.id, msg = alert.message, fields = ?alert.fields, "Sending alert...");
    let client = reqwest::Client::new();
    client
        .post(webhook_url.clone())
        .json(&alert.to_json())
        .send()
        .await?;
    Ok(())
}
