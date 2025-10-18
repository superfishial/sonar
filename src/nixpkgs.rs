use anyhow::{Context, Result};
use async_trait::async_trait;
use chrono::{DateTime, Duration, Utc};
use indexmap::IndexMap;
use serde::Deserialize;
use std::collections::HashMap;
use std::fs;
use std::path::Path;

use crate::monitor::{Alert, Monitor, Severity};

#[derive(Debug, Deserialize)]
struct FlakeLock {
    nodes: HashMap<String, Node>,
}

#[derive(Debug, Deserialize)]
struct Node {
    inputs: Option<HashMap<String, Input>>,
    locked: Option<Locked>,
}

#[derive(Debug, Deserialize)]
#[serde(untagged)]
enum Input {
    String(String),
    Array(Vec<String>),
}

#[derive(Debug, Deserialize)]
struct Locked {
    #[serde(rename = "lastModified")]
    last_modified: Option<i64>,
}

#[derive(Debug)]
pub struct FlakeLockMonitor {
    flake_lock_path: String,
    max_time_since_update: Duration,
}

impl FlakeLockMonitor {
    pub fn new(flake_lock_path: &str, max_time_since_update: Duration) -> Self {
        Self {
            flake_lock_path: flake_lock_path.to_string(),
            max_time_since_update,
        }
    }

    fn get_nixpkgs_last_modified(&self) -> Result<Option<DateTime<Utc>>> {
        let path = Path::new(&self.flake_lock_path);
        let contents = fs::read_to_string(path)
            .with_context(|| format!("Failed to read flake.lock at {}", self.flake_lock_path))?;

        let flake_lock: FlakeLock =
            serde_json::from_str(&contents).with_context(|| "Failed to parse flake.lock")?;

        // Find the root node's nixpkgs input
        let root_node = flake_lock
            .nodes
            .get("root")
            .with_context(|| "No root node found in flake.lock")?;

        let nixpkgs_node_name = root_node
            .inputs
            .as_ref()
            .and_then(|inputs| inputs.get("nixpkgs"))
            .with_context(|| "No nixpkgs input found in root node")?;
        let nixpkgs_node_name = match nixpkgs_node_name {
            Input::String(s) => s,
            Input::Array(a) => a.first().unwrap(),
        };

        // Get the actual nixpkgs node
        flake_lock
            .nodes
            .get(nixpkgs_node_name)
            .and_then(|nixpkgs_node| nixpkgs_node.locked.as_ref())
            .and_then(|locked| locked.last_modified)
            .map(|last_modified| {
                DateTime::from_timestamp(last_modified, 0)
                    .with_context(|| "Invalid timestamp in flake.lock")
            })
            .transpose()
    }
}

#[async_trait]
impl Monitor for FlakeLockMonitor {
    async fn run(&mut self) -> Result<Option<Alert>> {
        let id = "Flake Lock Outdated: nixpkgs";
        let now = Utc::now();

        match self.get_nixpkgs_last_modified()? {
            Some(last_modified) => {
                let duration_since = now.signed_duration_since(last_modified);
                if duration_since < self.max_time_since_update {
                    return Ok(None);
                }

                Ok(Some(Alert::new(
                    id,
                    &format!(
                        "It's been {} days since nixpkgs was updated in flake.lock. Run `nix flake update nixpkgs` to update",
                        duration_since.num_days()
                    ),
                    Severity::Warn,
                    IndexMap::from([("Last Updated".to_string(), last_modified.to_string())]),
                )))
            }
            None => Ok(Some(Alert::new(
                id,
                &format!(
                    "No nixpkgs input found in flake.lock at {}",
                    self.flake_lock_path
                ),
                Severity::Warn,
                IndexMap::new(),
            ))),
        }
    }
}
