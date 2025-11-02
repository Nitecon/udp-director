use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::RwLock;
use tracing::info;

/// Main configuration structure for the UDP Director
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Config {
    /// Port for the Phase 1 TCP Query Server
    pub query_port: u16,

    /// Port for the Phase 2 TCP/UDP Data Proxy
    pub data_port: u16,

    /// Default endpoint query to use if no token is provided
    pub default_endpoint: DefaultEndpoint,

    /// How long a token is valid for lookup (in seconds)
    pub token_ttl_seconds: u64,

    /// How long a data proxy session can be inactive before being torn down (in seconds)
    pub session_timeout_seconds: u64,

    /// Magic byte sequence (as a hex string) that prefixes a "Control Packet"
    pub control_packet_magic_bytes: String,

    /// Defines how client queries map to k8s resources
    pub resource_query_mapping: HashMap<String, ResourceMapping>,
}

/// Default endpoint query configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DefaultEndpoint {
    /// Resource type to query (e.g., "gameserver")
    pub resource_type: String,

    /// Namespace to search in
    pub namespace: String,

    /// Label selector for filtering resources
    #[serde(skip_serializing_if = "Option::is_none")]
    pub label_selector: Option<HashMap<String, String>>,

    /// Status query for filtering resources
    #[serde(skip_serializing_if = "Option::is_none")]
    pub status_query: Option<StatusQueryConfig>,
}

/// Status query configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct StatusQueryConfig {
    /// JSONPath to the status field (e.g., "status.state")
    pub json_path: String,

    /// Expected values for the status field (matches if any value matches)
    pub expected_values: Vec<String>,
}

/// Configuration for mapping a resource type to Kubernetes resources
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ResourceMapping {
    /// Group of the Kubernetes resource (e.g., "agones.dev")
    pub group: String,

    /// Version of the Kubernetes resource (e.g., "v1")
    pub version: String,

    /// Resource type (e.g., "gameservers")
    pub resource: String,

    /// SERVICE-BASED APPROACH (Legacy/Optional)
    /// The label on a SERVICE that links it to this resource
    #[serde(skip_serializing_if = "Option::is_none")]
    pub service_selector_label: Option<String>,

    /// The *name* of the port on the found Service to route to
    #[serde(skip_serializing_if = "Option::is_none")]
    pub service_target_port_name: Option<String>,

    /// DIRECT RESOURCE APPROACH (New)
    /// JSONPath to extract the address from the resource (e.g., "status.address")
    #[serde(skip_serializing_if = "Option::is_none")]
    pub address_path: Option<String>,

    /// JSONPath to extract the port from the resource (e.g., "status.ports[0].port")
    /// OR simple port name to look up (e.g., "default")
    #[serde(skip_serializing_if = "Option::is_none")]
    pub port_path: Option<String>,

    /// Simple port name lookup (alternative to portPath)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub port_name: Option<String>,
}

impl Config {
    /// Load configuration from environment or ConfigMap
    pub async fn load() -> Result<Self> {
        // In Kubernetes, we'll read from a mounted ConfigMap
        // Default path: /etc/udp-director/config.yaml
        let config_path =
            std::env::var("CONFIG_PATH").unwrap_or_else(|_| "/etc/udp-director/config.yaml".into());

        let config_content = tokio::fs::read_to_string(&config_path)
            .await
            .with_context(|| format!("Failed to read config file: {}", config_path))?;

        let config: Config =
            serde_yaml::from_str(&config_content).with_context(|| "Failed to parse config YAML")?;

        // Validate configuration
        config.validate()?;

        Ok(config)
    }

    /// Validate the configuration
    fn validate(&self) -> Result<()> {
        if self.query_port == 0 {
            anyhow::bail!("query_port must be non-zero");
        }
        if self.data_port == 0 {
            anyhow::bail!("data_port must be non-zero");
        }
        if self.default_endpoint.resource_type.is_empty() {
            anyhow::bail!("default_endpoint.resource_type must not be empty");
        }
        if self.default_endpoint.namespace.is_empty() {
            anyhow::bail!("default_endpoint.namespace must not be empty");
        }
        if self.resource_query_mapping.is_empty() {
            anyhow::bail!("resource_query_mapping must not be empty");
        }

        // Validate hex string for magic bytes
        hex::decode(&self.control_packet_magic_bytes)
            .with_context(|| "control_packet_magic_bytes must be a valid hex string")?;

        Ok(())
    }

    /// Get the default endpoint configuration
    pub fn get_default_endpoint(&self) -> &DefaultEndpoint {
        &self.default_endpoint
    }

    /// Get the decoded magic bytes
    pub fn get_magic_bytes(&self) -> Result<Vec<u8>> {
        hex::decode(&self.control_packet_magic_bytes)
            .with_context(|| "Failed to decode control_packet_magic_bytes")
    }

    /// Watch for configuration changes (simplified version)
    /// In a full implementation, this would use the Kubernetes API to watch the ConfigMap
    pub async fn watch_for_changes(&self) -> Result<()> {
        // This is a placeholder for ConfigMap watching
        // A full implementation would use kube-rs to watch the ConfigMap resource
        // and reload the configuration when it changes
        info!("Config watcher started (placeholder - not yet implemented)");

        // Keep the task alive
        loop {
            tokio::time::sleep(tokio::time::Duration::from_secs(60)).await;
        }
    }
}

/// Global configuration holder with hot-reload support
#[allow(dead_code)]
pub struct ConfigHolder {
    config: Arc<RwLock<Config>>,
}

impl ConfigHolder {
    /// Create a new config holder
    #[allow(dead_code)]
    pub fn new(config: Config) -> Self {
        Self {
            config: Arc::new(RwLock::new(config)),
        }
    }

    /// Get a read lock on the config
    #[allow(dead_code)]
    pub async fn read(&self) -> tokio::sync::RwLockReadGuard<'_, Config> {
        self.config.read().await
    }

    /// Reload the configuration
    #[allow(dead_code)]
    pub async fn reload(&self) -> Result<()> {
        let new_config = Config::load().await?;
        let mut config = self.config.write().await;
        *config = new_config;
        info!("Configuration reloaded successfully");
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_default_endpoint_config() {
        let mut label_selector = HashMap::new();
        label_selector.insert("agones.dev/fleet".to_string(), "m-tutorial".to_string());

        let config = Config {
            query_port: 9000,
            data_port: 7777,
            default_endpoint: DefaultEndpoint {
                resource_type: "gameserver".to_string(),
                namespace: "default".to_string(),
                label_selector: Some(label_selector),
                status_query: Some(StatusQueryConfig {
                    json_path: "status.state".to_string(),
                    expected_values: vec!["Ready".to_string()],
                }),
            },
            token_ttl_seconds: 30,
            session_timeout_seconds: 300,
            control_packet_magic_bytes: "FFFFFFFF5245534554".to_string(),
            resource_query_mapping: HashMap::new(),
        };

        let endpoint = config.get_default_endpoint();
        assert_eq!(endpoint.resource_type, "gameserver");
        assert_eq!(endpoint.namespace, "default");
    }

    #[test]
    fn test_default_endpoint_without_status_query() {
        let mut label_selector = HashMap::new();
        label_selector.insert("agones.dev/fleet".to_string(), "m-tutorial".to_string());

        let config = Config {
            query_port: 9000,
            data_port: 7777,
            default_endpoint: DefaultEndpoint {
                resource_type: "gameserver".to_string(),
                namespace: "starx".to_string(),
                label_selector: Some(label_selector),
                status_query: None, // No status filtering
            },
            token_ttl_seconds: 30,
            session_timeout_seconds: 300,
            control_packet_magic_bytes: "FFFFFFFF5245534554".to_string(),
            resource_query_mapping: HashMap::new(),
        };

        let endpoint = config.get_default_endpoint();
        assert_eq!(endpoint.resource_type, "gameserver");
        assert_eq!(endpoint.namespace, "starx");
        assert!(endpoint.status_query.is_none());
    }

    #[test]
    fn test_magic_bytes_decode() {
        let config = Config {
            query_port: 9000,
            data_port: 7777,
            default_endpoint: DefaultEndpoint {
                resource_type: "gameserver".to_string(),
                namespace: "default".to_string(),
                label_selector: None,
                status_query: None,
            },
            token_ttl_seconds: 30,
            session_timeout_seconds: 300,
            control_packet_magic_bytes: "FFFFFFFF5245534554".to_string(),
            resource_query_mapping: HashMap::new(),
        };

        let magic_bytes = config.get_magic_bytes().unwrap();
        assert_eq!(
            magic_bytes,
            vec![0xFF, 0xFF, 0xFF, 0xFF, 0x52, 0x45, 0x53, 0x45, 0x54]
        );
    }
}
