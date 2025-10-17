use clap::Parser;
use reqwest::Url;

#[derive(Parser, Clone)]
pub struct Config {
    #[arg(long, env, default_value = "15000")]
    pub polling_interval_ms: u64,

    #[arg(long, env)]
    pub discord_webhook_url: Url,
}
