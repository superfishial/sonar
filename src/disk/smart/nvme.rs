use std::path::PathBuf;

use anyhow::Result;
use async_trait::async_trait;
use indexmap::IndexMap;
use serde::Deserialize;
use serde_json::Value;

use crate::{
    disk::smart::get_smartctl_data,
    monitor::{Alert, Monitor, Severity},
};

#[derive(Debug, Deserialize)]
pub struct DiskNvmeSmartConfig {
    pub device: PathBuf,
    #[serde(default = "default_percentage_used_threshold")]
    pub percentage_used_threshold: u8,
    #[serde(default = "default_temperature_warning")]
    pub temperature_warning: u8,
    #[serde(default = "default_temperature_critical")]
    pub temperature_critical: u8,
    #[serde(default = "default_available_spare_threshold")]
    pub available_spare_min: u8,
    #[serde(default = "default_critical_warning_check")]
    pub check_critical_warning: bool,
}

fn default_percentage_used_threshold() -> u8 {
    50
}
fn default_temperature_warning() -> u8 {
    70
}
fn default_temperature_critical() -> u8 {
    80
}
fn default_available_spare_threshold() -> u8 {
    20
}
fn default_critical_warning_check() -> bool {
    true
}

#[derive(Debug)]
pub struct DiskNvmeSmartMonitor {
    device: PathBuf,
    percentage_used_threshold: u8,
    temperature_warning: u8,
    temperature_critical: u8,
    available_spare_min: u8,
    check_critical_warning: bool,
}

impl DiskNvmeSmartMonitor {
    pub fn new(config: DiskNvmeSmartConfig) -> Self {
        Self {
            device: config.device,
            percentage_used_threshold: config.percentage_used_threshold,
            temperature_warning: config.temperature_warning,
            temperature_critical: config.temperature_critical,
            available_spare_min: config.available_spare_min,
            check_critical_warning: config.check_critical_warning,
        }
    }

    fn check_smart_health(&self, data: &Value) -> Result<Vec<String>> {
        let mut issues = Vec::new();

        let smart_log = &data["nvme_smart_health_information_log"];

        // Write endurance
        if let Some(percentage_used) = smart_log["percentage_used"].as_u64() {
            if percentage_used > self.percentage_used_threshold as u64 {
                issues.push(format!(
                    "Endurance used ({percentage_used}%) exceeds threshold ({}%)",
                    self.percentage_used_threshold
                ));
            }
        }

        // Temperature
        if let Some(temp) = smart_log["temperature"].as_u64() {
            if temp >= self.temperature_critical as u64 {
                issues.push(format!(
                    "Temperature ({temp}°C) is at or above critical threshold ({}°C)",
                    self.temperature_critical
                ));
            } else if temp >= self.temperature_warning as u64 {
                issues.push(format!(
                    "Temperature ({temp}°C) is at or above warning threshold ({}°C)",
                    self.temperature_warning
                ));
            }
        }

        // Available spare
        if let Some(spare) = smart_log["available_spare"].as_u64() {
            if spare < self.available_spare_min as u64 {
                issues.push(format!(
                    "Available spare ({spare}%) is below minimum threshold ({}%)",
                    self.available_spare_min
                ));
            }
        }

        // Critical warning flags
        if self.check_critical_warning {
            if let Some(critical_warning) = smart_log["critical_warning"].as_u64() {
                if critical_warning != 0 {
                    let warnings = self.decode_critical_warning(critical_warning);
                    issues.push(format!(
                        "Critical warning flags set: {}",
                        warnings.join(", ")
                    ));
                }
            }
        }

        // SMART status
        if let Some(passed) = data["smart_status"]["passed"].as_bool() {
            if !passed {
                issues.push("SMART status: FAILED".to_string());
            }
        }

        // Media errors
        if let Some(media_errors) = smart_log["media_errors"].as_u64() {
            if media_errors > 0 {
                issues.push(format!("Media errors detected: {media_errors}"));
            }
        }

        // Error log entries
        if let Some(err_log_entries) = smart_log["num_err_log_entries"].as_u64() {
            if err_log_entries > 0 {
                issues.push(format!("Error log entries: {err_log_entries}"));
            }
        }

        Ok(issues)
    }

    fn decode_critical_warning(&self, flags: u64) -> Vec<String> {
        let mut warnings = Vec::new();

        if flags & 0x01 != 0 {
            warnings.push("Available spare below threshold".to_string());
        }
        if flags & 0x02 != 0 {
            warnings.push("Temperature threshold exceeded".to_string());
        }
        if flags & 0x04 != 0 {
            warnings.push("NVM subsystem reliability degraded".to_string());
        }
        if flags & 0x08 != 0 {
            warnings.push("Media in read-only mode".to_string());
        }
        if flags & 0x10 != 0 {
            warnings.push("Volatile memory backup device failed".to_string());
        }
        if flags & 0x20 != 0 {
            warnings.push("Persistent memory region read-only or unreliable".to_string());
        }

        warnings
    }

    fn get_metadata(&self, data: &Value) -> IndexMap<String, String> {
        let mut metadata = IndexMap::new();
        let smart_log = &data["nvme_smart_health_information_log"];

        // Basic device info
        if let Some(model) = data["model_name"].as_str() {
            metadata.insert("Model".to_string(), model.to_string());
        }
        if let Some(serial) = data["serial_number"].as_str() {
            metadata.insert("Serial Number".to_string(), serial.to_string());
        }
        if let Some(firmware) = data["firmware_version"].as_str() {
            metadata.insert("Firmware".to_string(), firmware.to_string());
        }

        // Health metrics
        if let Some(temp) = smart_log["temperature"].as_u64() {
            metadata.insert("Temperature".to_string(), format!("{temp}°C"));
        }
        if let Some(percentage_used) = smart_log["percentage_used"].as_u64() {
            metadata.insert("Endurance Used".to_string(), format!("{percentage_used}%"));
        }
        if let Some(spare) = smart_log["available_spare"].as_u64() {
            metadata.insert("Available Spare".to_string(), format!("{spare}%"));
        }
        if let Some(power_on) = smart_log["power_on_hours"].as_u64() {
            metadata.insert("Power On Hours".to_string(), power_on.to_string());
        }
        if let Some(power_cycles) = smart_log["power_cycles"].as_u64() {
            metadata.insert("Power Cycles".to_string(), power_cycles.to_string());
        }
        if let Some(unsafe_shutdowns) = smart_log["unsafe_shutdowns"].as_u64() {
            metadata.insert("Unsafe Shutdowns".to_string(), unsafe_shutdowns.to_string());
        }
        if let Some(media_errors) = smart_log["media_errors"].as_u64() {
            metadata.insert("Media Errors".to_string(), media_errors.to_string());
        }

        // Data transfer stats
        if let Some(data_read) = smart_log["data_units_read"].as_u64() {
            let gb_read = (data_read * 512 * 1000) as f64 / 1_000_000_000.0;
            metadata.insert("Data Read".to_string(), format!("{:.2} GB", gb_read));
        }
        if let Some(data_written) = smart_log["data_units_written"].as_u64() {
            let gb_written = (data_written * 512 * 1000) as f64 / 1_000_000_000.0;
            metadata.insert("Data Written".to_string(), format!("{:.2} GB", gb_written));
        }

        metadata
    }

    fn determine_severity(&self, issues: &[String]) -> Severity {
        for issue in issues {
            if issue.contains("FAILED")
                || issue.contains("critical")
                || issue.contains("Media errors")
                || issue.contains("read-only mode")
                || issue.contains("reliability degraded")
            {
                return Severity::Critical;
            }
        }
        Severity::Warn
    }
}

#[async_trait]
impl Monitor for DiskNvmeSmartMonitor {
    fn name(&self) -> String {
        format!("NVMe SMART: {}", self.device.display())
    }

    async fn run(&mut self) -> Result<Option<Alert>> {
        let data = get_smartctl_data(&self.device)?;
        let issues = self.check_smart_health(&data)?;

        if issues.is_empty() {
            return Ok(None);
        }

        let severity = self.determine_severity(&issues);
        let metadata = self.get_metadata(&data);

        Ok(Some(Alert::new(
            &format!("NVMe SMART: {}", self.device.display()),
            &format!(
                "NVMe SMART health issues detected:\n- {}",
                issues.join("\n- ")
            ),
            severity,
            metadata,
        )))
    }
}
