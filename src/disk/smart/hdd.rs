use std::path::PathBuf;
use std::process::Command;

use anyhow::{Context, Result};
use async_trait::async_trait;
use indexmap::IndexMap;
use serde::Deserialize;
use serde_json::Value;

use crate::monitor::{Alert, Monitor, Severity};

#[derive(Debug, Deserialize)]
pub struct DiskHddSmartConfig {
    pub device: PathBuf,
    #[serde(default = "default_temperature_warning")]
    pub temperature_warning: i64,
    #[serde(default = "default_temperature_critical")]
    pub temperature_critical: i64,
    #[serde(default = "default_reallocated_sectors_threshold")]
    pub reallocated_sectors_threshold: u64,
    #[serde(default = "default_pending_sectors_threshold")]
    pub pending_sectors_threshold: u64,
    #[serde(default = "default_udma_crc_error_threshold")]
    pub udma_crc_error_threshold: u64,
    #[serde(default = "default_spin_retry_threshold")]
    pub spin_retry_threshold: u64,
    #[serde(default = "default_helium_level_min")]
    pub helium_level_min: u8,
}

fn default_temperature_warning() -> i64 {
    50
}
fn default_temperature_critical() -> i64 {
    60
}
fn default_reallocated_sectors_threshold() -> u64 {
    0
}
fn default_pending_sectors_threshold() -> u64 {
    0
}
fn default_udma_crc_error_threshold() -> u64 {
    0
}
fn default_spin_retry_threshold() -> u64 {
    0
}
fn default_helium_level_min() -> u8 {
    50
}

#[derive(Debug)]
pub struct DiskHddSmartMonitor {
    device: PathBuf,
    temperature_warning: i64,
    temperature_critical: i64,
    reallocated_sectors_threshold: u64,
    pending_sectors_threshold: u64,
    udma_crc_error_threshold: u64,
    spin_retry_threshold: u64,
    helium_level_min: u8,
}

impl DiskHddSmartMonitor {
    pub fn new(config: DiskHddSmartConfig) -> Self {
        Self {
            device: config.device,
            temperature_warning: config.temperature_warning,
            temperature_critical: config.temperature_critical,
            reallocated_sectors_threshold: config.reallocated_sectors_threshold,
            pending_sectors_threshold: config.pending_sectors_threshold,
            udma_crc_error_threshold: config.udma_crc_error_threshold,
            spin_retry_threshold: config.spin_retry_threshold,
            helium_level_min: config.helium_level_min,
        }
    }

    fn get_smartctl_data(&self) -> Result<Value> {
        let output = Command::new("smartctl")
            .arg("-a")
            .arg(&self.device)
            .arg("-j")
            .output()
            .context("Failed to execute smartctl command")?;

        if !output.status.success() && output.status.code() != Some(0) {
            if output.stdout.is_empty() {
                anyhow::bail!(
                    "smartctl failed with exit code: {:?}, stderr: {}",
                    output.status.code(),
                    String::from_utf8_lossy(&output.stderr)
                );
            }
        }

        let json: Value = serde_json::from_slice(&output.stdout)
            .context("Failed to parse smartctl JSON output")?;

        Ok(json)
    }

    fn get_attribute_by_id<'a>(&self, data: &'a Value, id: u64) -> Option<&'a Value> {
        data["ata_smart_attributes"]["table"]
            .as_array()?
            .iter()
            .find(|attr| attr["id"].as_u64() == Some(id))
    }

    fn get_attribute_raw_value(&self, attr: &Value) -> u64 {
        attr["raw"]["value"].as_u64().unwrap_or(0)
    }

    fn get_attribute_value(&self, attr: &Value) -> u64 {
        attr["value"].as_u64().unwrap_or(0)
    }

    fn get_attribute_threshold(&self, attr: &Value) -> u64 {
        attr["thresh"].as_u64().unwrap_or(0)
    }

    fn check_smart_health(&self, data: &Value) -> Result<Vec<String>> {
        let mut issues = Vec::new();

        // SMART status
        if let Some(passed) = data["smart_status"]["passed"].as_bool()
            && !passed
        {
            issues.push("SMART status: FAILED".to_string());
        }

        // Temperature
        if let Some(_) = self.get_attribute_by_id(data, 194)
            && let Some(current_temp) = data["temperature"]["current"].as_i64()
        {
            if current_temp >= self.temperature_critical {
                issues.push(format!(
                    "Temperature ({current_temp}°C) is at or above critical threshold ({}°C)",
                    self.temperature_critical
                ));
            } else if current_temp >= self.temperature_warning {
                issues.push(format!(
                    "Temperature ({current_temp}°C) is at or above warning threshold ({}°C)",
                    self.temperature_warning
                ));
            }
        }

        // Reallocated sectors count (typically means physical damage)
        if let Some(attr) = self.get_attribute_by_id(data, 5) {
            let raw_value = self.get_attribute_raw_value(attr);
            if raw_value > self.reallocated_sectors_threshold {
                issues.push(format!(
                    "Reallocated sectors ({raw_value}) exceeds threshold ({})",
                    self.reallocated_sectors_threshold
                ));
            }
        }

        // Current pending sector count
        if let Some(attr) = self.get_attribute_by_id(data, 197) {
            let raw_value = self.get_attribute_raw_value(attr);
            if raw_value > self.pending_sectors_threshold {
                issues.push(format!(
                    "Pending sectors ({raw_value}) exceeds threshold ({})",
                    self.pending_sectors_threshold
                ));
            }
        }

        // Offline uncorrectable sectors
        if let Some(attr) = self.get_attribute_by_id(data, 198) {
            let raw_value = self.get_attribute_raw_value(attr);
            if raw_value > 0 {
                issues.push(format!(
                    "Offline uncorrectable sectors detected: {raw_value}"
                ));
            }
        }

        // UDMA CRC error count
        if let Some(attr) = self.get_attribute_by_id(data, 199) {
            let raw_value = self.get_attribute_raw_value(attr);
            if raw_value > self.udma_crc_error_threshold {
                issues.push(format!(
                    "UDMA CRC errors ({raw_value}) exceeds threshold ({})",
                    self.udma_crc_error_threshold
                ));
            }
        }

        // Spin retry count
        if let Some(attr) = self.get_attribute_by_id(data, 10) {
            let raw_value = self.get_attribute_raw_value(attr);
            if raw_value > self.spin_retry_threshold {
                issues.push(format!(
                    "Spin retry count ({raw_value}) exceeds threshold ({})",
                    self.spin_retry_threshold
                ));
            }
        }

        // Reallocated event count
        if let Some(attr) = self.get_attribute_by_id(data, 196) {
            let raw_value = self.get_attribute_raw_value(attr);
            if raw_value > 0 {
                issues.push(format!("Reallocated event count: {raw_value}"));
            }
        }

        // Raw read error rate
        if let Some(attr) = self.get_attribute_by_id(data, 1) {
            let value = self.get_attribute_value(attr);
            let threshold = self.get_attribute_threshold(attr);
            if value < threshold && threshold > 0 {
                issues.push(format!(
                    "Raw read error rate ({value}) is below threshold ({threshold})"
                ));
            }
        }

        // Seek error rate
        if let Some(attr) = self.get_attribute_by_id(data, 7) {
            let value = self.get_attribute_value(attr);
            let threshold = self.get_attribute_threshold(attr);
            if value < threshold && threshold > 0 {
                issues.push(format!(
                    "Seek error rate ({value}) is below threshold ({threshold})"
                ));
            }
        }

        // Helium level
        if let Some(attr) = self.get_attribute_by_id(data, 22) {
            let value = self.get_attribute_value(attr);
            if value < self.helium_level_min as u64 {
                issues.push(format!(
                    "Helium level ({value}%) is below minimum threshold ({}%)",
                    self.helium_level_min
                ));
            }
        }

        // Error log entries
        if let Some(error_count) = data["ata_smart_error_log"]["summary"]["count"].as_u64()
            && error_count > 0
        {
            issues.push(format!("SMART error log contains {error_count} entries"));
        }

        // Prefailure attributes below threshold
        if let Some(attrs) = data["ata_smart_attributes"]["table"].as_array() {
            for attr in attrs {
                if let Some(flags) = attr["flags"].as_object()
                    && flags["prefailure"].as_bool() == Some(true)
                {
                    let value = self.get_attribute_value(attr);
                    let threshold = self.get_attribute_threshold(attr);
                    if value <= threshold && threshold > 0 {
                        let name = attr["name"].as_str().unwrap_or("Unknown");
                        let id = attr["id"].as_u64().unwrap_or(0);
                        issues.push(format!(
                                "Prefailure attribute {name} (ID {id}) value ({value}) is at or below threshold ({threshold})"
                            ));
                    }
                }
            }
        }

        Ok(issues)
    }

    fn get_metadata(&self, data: &Value) -> IndexMap<String, String> {
        let mut metadata = IndexMap::new();

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
        if let Some(capacity) = data["user_capacity"]["bytes"].as_u64() {
            let tb = capacity as f64 / 1_000_000_000_000.0;
            metadata.insert("Capacity".to_string(), format!("{:.2} TB", tb));
        }
        if let Some(rpm) = data["rotation_rate"].as_u64() {
            metadata.insert("RPM".to_string(), rpm.to_string());
        }

        // Key SMART attributes
        if let Some(temp) = data["temperature"]["current"].as_i64() {
            metadata.insert("Temperature".to_string(), format!("{temp}°C"));
        }
        if let Some(power_on) = data["power_on_time"]["hours"].as_u64() {
            metadata.insert("Power On Hours".to_string(), power_on.to_string());
        }
        if let Some(power_cycles) = data["power_cycle_count"].as_u64() {
            metadata.insert("Power Cycles".to_string(), power_cycles.to_string());
        }

        // Critical attributes
        if let Some(attr) = self.get_attribute_by_id(data, 5) {
            let value = self.get_attribute_raw_value(attr);
            metadata.insert("Reallocated Sectors".to_string(), value.to_string());
        }
        if let Some(attr) = self.get_attribute_by_id(data, 197) {
            let value = self.get_attribute_raw_value(attr);
            metadata.insert("Pending Sectors".to_string(), value.to_string());
        }
        if let Some(attr) = self.get_attribute_by_id(data, 198) {
            let value = self.get_attribute_raw_value(attr);
            metadata.insert("Offline Uncorrectable".to_string(), value.to_string());
        }
        if let Some(attr) = self.get_attribute_by_id(data, 199) {
            let value = self.get_attribute_raw_value(attr);
            metadata.insert("UDMA CRC Errors".to_string(), value.to_string());
        }
        if let Some(attr) = self.get_attribute_by_id(data, 10) {
            let value = self.get_attribute_raw_value(attr);
            metadata.insert("Spin Retry Count".to_string(), value.to_string());
        }
        if let Some(attr) = self.get_attribute_by_id(data, 22) {
            let value = self.get_attribute_value(attr);
            metadata.insert("Helium Level".to_string(), format!("{value}%"));
        }

        // Error counts
        if let Some(error_count) = data["ata_smart_error_log"]["summary"]["count"].as_u64() {
            metadata.insert("Error Log Entries".to_string(), error_count.to_string());
        }

        metadata
    }

    fn determine_severity(&self, issues: &[String]) -> Severity {
        for issue in issues {
            if issue.contains("FAILED")
                || issue.contains("Prefailure attribute")
                || issue.contains("Offline uncorrectable")
                || issue.contains("Pending sectors")
                || issue.contains("Reallocated sectors")
                || issue.contains("critical")
            {
                return Severity::Critical;
            }
        }
        Severity::Warn
    }
}

#[async_trait]
impl Monitor for DiskHddSmartMonitor {
    fn name(&self) -> String {
        format!("HDD SMART: {}", self.device.display())
    }

    async fn run(&mut self) -> Result<Option<Alert>> {
        let data = self.get_smartctl_data()?;
        let issues = self.check_smart_health(&data)?;

        if issues.is_empty() {
            return Ok(None);
        }

        let severity = self.determine_severity(&issues);
        let metadata = self.get_metadata(&data);

        Ok(Some(Alert::new(
            &format!("HDD SMART: {}", self.device.display()),
            &format!("HDD SMART health issues detected:\n{}", issues.join("\n")),
            severity,
            metadata,
        )))
    }
}
