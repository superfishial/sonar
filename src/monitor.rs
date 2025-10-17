use std::fmt::Debug;

use anyhow::Result;
use async_trait::async_trait;
use indexmap::IndexMap;
use serde_json::{Value, json};

#[async_trait]
pub trait Monitor: Debug {
    async fn run(&mut self) -> Result<Option<Alert>>;
}

#[derive(Debug)]
pub struct Alert {
    pub id: String,
    pub message: String,
    pub severity: Severity,
    pub fields: IndexMap<String, String>,
}

impl Alert {
    pub fn new(
        id: &str,
        title: &str,
        severity: Severity,
        fields: IndexMap<String, String>,
    ) -> Self {
        Self {
            id: id.to_string(),
            message: title.to_string(),
            severity,
            fields,
        }
    }

    pub fn to_json(&self) -> Value {
        let title = match self.severity {
            Severity::Warn => "⚠️ ".to_string(),
            Severity::Critical => "🔥 ".to_string(),
        } + &self.id;

        json!({
            "embeds": [{
                "title": title,
                "description": self.message,
                "color": match self.severity {
                    Severity::Warn => 0xFFD06B,
                    Severity::Critical => 0xFF6B6B,
                },
                "fields": self.fields.iter().map(|(k, v)| json!({ "name": k, "value": v, "inline": true })).collect::<Vec<_>>(),
                "timestamp": chrono::Utc::now().to_rfc3339(),
            }]
        })
    }
}

#[derive(Debug)]
pub enum Severity {
    Warn,
    Critical,
}
