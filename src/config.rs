use std::path::PathBuf;

use clap::Parser;
use reqwest::Url;
use serde::{Deserialize, Deserializer};

use crate::{
    cpu::{CpuTempConfig, CpuUsageConfig},
    disk::{DiskHealthConfig, DiskScrubConfig, DiskUsageConfig},
    memory::{MemoryOOMConfig, MemoryUsageConfig},
    nixpkgs::NixpkgsConfig,
    systemd::SystemdConfig,
};

#[derive(Parser, Clone)]
pub struct Config {
    #[arg(long, default_value = "monitors.toml")]
    pub monitors_config_path: PathBuf,

    #[arg(long, env, default_value = "15000")]
    pub polling_interval_ms: u64,

    #[arg(long, env)]
    pub discord_webhook_url: Url,

    #[arg(long, env, default_value = "false")]
    pub dry_run: bool,
}

#[derive(Debug, Deserialize)]
pub struct MonitorsConfig {
    #[serde(default)]
    pub cpu: CpuConfig,
    #[serde(default)]
    pub disk: DiskConfig,
    #[serde(default)]
    pub memory: MemoryConfig,
    #[serde(default)]
    pub nixpkgs: Option<NixpkgsConfig>,
    #[serde(default)]
    pub systemd: Option<SystemdConfig>,
}

#[derive(Debug, Default, Deserialize)]
pub struct CpuConfig {
    pub temp: Option<CpuTempConfig>,
    #[serde(default)]
    pub usage: Option<CpuUsageConfig>,
}

#[derive(Debug, Default, Deserialize)]
pub struct DiskConfig {
    #[serde(default)]
    pub health: OneOrMany<DiskHealthConfig>,
    #[serde(default)]
    pub usage: OneOrMany<DiskUsageConfig>,
    #[serde(default)]
    pub scrub: OneOrMany<DiskScrubConfig>,
}

#[derive(Debug, Default, Deserialize)]
pub struct MemoryConfig {
    #[serde(default)]
    pub oom: Option<MemoryOOMConfig>,
    #[serde(default)]
    pub usage: Option<MemoryUsageConfig>,
}

// Helper enum to handle both single values and arrays
#[derive(Debug)]
pub enum OneOrMany<T> {
    One(T),
    Many(Vec<T>),
}

impl<T> Default for OneOrMany<T> {
    fn default() -> Self {
        OneOrMany::Many(Vec::new())
    }
}

impl<T> OneOrMany<T> {
    pub fn into_vec(self) -> Vec<T> {
        match self {
            OneOrMany::One(item) => vec![item],
            OneOrMany::Many(items) => items,
        }
    }
}

impl<'de, T> Deserialize<'de> for OneOrMany<T>
where
    T: Deserialize<'de>,
{
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        #[derive(Deserialize)]
        #[serde(untagged)]
        enum Helper<T> {
            One(T),
            Many(Vec<T>),
        }

        match Helper::deserialize(deserializer)? {
            Helper::One(item) => Ok(OneOrMany::One(item)),
            Helper::Many(items) => Ok(OneOrMany::Many(items)),
        }
    }
}

impl MonitorsConfig {
    pub fn from_file(path: &std::path::Path) -> anyhow::Result<Self> {
        let contents = std::fs::read_to_string(path)?;
        let config: MonitorsConfig = toml::from_str(&contents)?;
        Ok(config)
    }
}
