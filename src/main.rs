use std::collections::HashMap;

use chrono::Duration;
use clap::Parser;
use indexmap::IndexMap;
use reqwest::Url;
use tracing::{error, info, warn};
use tracing_subscriber::EnvFilter;

use crate::{
    config::Config,
    cpu::{CpuTempMonitor, CpuUsageMonitor},
    disk::{DiskScrubMonitor, DiskStatsMonitor, DiskUsageMonitor},
    memory::MemoryUsageMonitor,
    monitor::{Alert, Monitor, Severity},
    nixpkgs::FlakeLockMonitor,
    systemd::SystemdServiceMonitor,
};

mod config;
mod cpu;
mod disk;
mod memory;
mod monitor;
mod nixpkgs;
mod systemd;

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

    if config.dry_run {
        warn!("!!! Dry run mode enabled !!!");
    }

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
        // Nixpkgs
        Box::new(FlakeLockMonitor::new(
            "/home/saghen/code/personal/nixfiles/flake.lock",
            Duration::days(30),
        )),
        // Systemd
        Box::new(SystemdServiceMonitor::new(
            "restic-backups-primary",
            Duration::days(1),
        )),
    ];

    let mut alert_sender = AlertSender::new(&config.discord_webhook_url, config.dry_run);

    info!("Starting sonar");
    loop {
        info!("Running monitor loop...");
        for monitor in monitors.iter_mut() {
            match monitor.run().await {
                Ok(Some(alert)) => {
                    alert_sender.send(&alert).await;
                }
                Err(e) => {
                    alert_sender
                        .send(&Alert::new(
                            &format!("Error running monitor {}", monitor.name()),
                            &format!("{:?}", e),
                            Severity::Warn,
                            IndexMap::new(),
                        ))
                        .await;
                }
                _ => {}
            }
        }
        info!("Monitor loop complete");
        tokio::time::sleep(std::time::Duration::from_millis(config.polling_interval_ms)).await;
    }
}

struct AlertSender {
    webhook_url: Url,
    dry_run: bool,
    last_alert_send: HashMap<String, chrono::DateTime<chrono::Utc>>,
}

impl AlertSender {
    pub fn new(webhook_url: &Url, dry_run: bool) -> Self {
        Self {
            webhook_url: webhook_url.clone(),
            dry_run,
            last_alert_send: HashMap::new(),
        }
    }

    async fn send(&mut self, alert: &Alert) {
        // Ensure X hours have passed since the last alert with this ID
        let sufficient_time_since_last_alert = self
            .last_alert_send
            .get(&alert.id)
            .map(|last_send| (*last_send + chrono::Duration::hours(4)) < chrono::Utc::now())
            .unwrap_or(true);
        if !sufficient_time_since_last_alert {
            info!(
                id = alert.id,
                "Not sending alert as it was sent less than 4 hours ago",
            );
        }

        self.last_alert_send
            .insert(alert.id.clone(), chrono::Utc::now());
        warn!(id = alert.id, msg = alert.message, fields = ?alert.fields, "Sending alert...");
        if self.dry_run {
            return;
        }

        let client = reqwest::Client::new();
        match client
            .post(self.webhook_url.clone())
            .json(&alert.to_json())
            .send()
            .await
        {
            Ok(_) => {}
            Err(e) => error!("Error sending alert: {:?}", e),
        };
    }
}
