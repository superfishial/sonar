use std::collections::HashMap;

use anyhow::Result;
use clap::Parser;
use indexmap::IndexMap;
use reqwest::Url;
use tracing::{error, info, warn};
use tracing_subscriber::EnvFilter;

use crate::{
    config::{Config, MonitorsConfig},
    cpu::{CpuTempMonitor, CpuUsageMonitor},
    disk::{
        DiskHddSmartMonitor, DiskHealthMonitor, DiskNvmeSmartMonitor, DiskScrubMonitor,
        DiskUsageMonitor,
    },
    memory::{MemoryOOMMonitor, MemoryUsageMonitor},
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

    // Returns an error if the `.env` file doesn't exist, which we ignore
    dotenv::dotenv().ok();

    // Configuration
    let config = Config::parse();
    if config.dry_run {
        warn!("!!! Dry run mode enabled !!!");
    }

    let monitors_config = MonitorsConfig::from_file(&config.monitors_config_path).unwrap();
    let mut monitors = build_monitors(monitors_config).unwrap();

    // Main loop
    info!("Starting sonar");
    let mut alert_sender = AlertSender::new(&config.discord_webhook_url, config.dry_run);
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
                            &format!("Error running monitor: {}", monitor.name()),
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
            .map(|last_send| (*last_send + chrono::Duration::hours(24)) < chrono::Utc::now())
            .unwrap_or(true);
        if !sufficient_time_since_last_alert {
            info!(
                id = alert.id,
                "Not sending alert as it was sent less than 4 hours ago",
            );
            return;
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

fn build_monitors(config: MonitorsConfig) -> Result<Vec<Box<dyn Monitor>>> {
    let mut monitors: Vec<Box<dyn Monitor>> = Vec::new();

    // CPU monitors
    if let Some(cpu_temp_config) = config.cpu.temp {
        monitors.push(Box::new(CpuTempMonitor::new(cpu_temp_config)));
    }
    for cpu_usage_config in config.cpu.usage.into_vec() {
        monitors.push(Box::new(CpuUsageMonitor::new(cpu_usage_config)));
    }

    // Disk monitors
    for disk_health_config in config.disk.health.into_vec() {
        monitors.push(Box::new(DiskHealthMonitor::new(disk_health_config)));
    }
    for disk_usage_config in config.disk.usage.into_vec() {
        monitors.push(Box::new(DiskUsageMonitor::new(disk_usage_config)));
    }
    for disk_scrub_config in config.disk.scrub.into_vec() {
        monitors.push(Box::new(DiskScrubMonitor::new(disk_scrub_config)));
    }

    for disk_hdd_smart_config in config.disk.smart.hdd.into_vec() {
        monitors.push(Box::new(DiskHddSmartMonitor::new(disk_hdd_smart_config)));
    }
    for disk_nvme_smart_config in config.disk.smart.nvme.into_vec() {
        monitors.push(Box::new(DiskNvmeSmartMonitor::new(disk_nvme_smart_config)));
    }

    // Memory monitor
    if let Some(memory_usage_config) = config.memory.usage {
        monitors.push(Box::new(MemoryUsageMonitor::new(memory_usage_config)));
    }
    if let Some(memory_oom_config) = config.memory.oom {
        monitors.push(Box::new(MemoryOOMMonitor::new(memory_oom_config)));
    }

    // Nixpkgs monitor
    if let Some(nixpkgs_config) = config.nixpkgs {
        monitors.push(Box::new(FlakeLockMonitor::new(nixpkgs_config)));
    }

    // Systemd monitor
    for systemd_config in config.systemd.into_vec() {
        monitors.push(Box::new(SystemdServiceMonitor::new(systemd_config)));
    }

    Ok(monitors)
}
