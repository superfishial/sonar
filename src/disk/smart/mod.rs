pub mod hdd;
pub mod nvme;

use anyhow::{Context, Result};
use serde_json::Value;
use std::{path::PathBuf, process::Command};

pub fn get_smartctl_data(device: &PathBuf) -> Result<Value> {
    let output = Command::new("smartctl")
        .arg("-a")
        .arg(device)
        .arg("-j")
        .output()
        .context("Failed to execute smartctl command")?;

    if !output.status.success() && output.status.code() != Some(0) {
        // smartctl returns non-zero exit codes for various conditions
        // We still want to parse the JSON if available
        if output.stdout.is_empty() {
            anyhow::bail!(
                "smartctl failed with exit code: {:?}, stderr: {}",
                output.status.code(),
                String::from_utf8_lossy(&output.stderr)
            );
        }
    }

    let json: Value =
        serde_json::from_slice(&output.stdout).context("Failed to parse smartctl JSON output")?;

    Ok(json)
}
