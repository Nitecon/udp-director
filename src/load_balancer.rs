use anyhow::Result;
use dashmap::DashMap;
use kube::api::DynamicObject;
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use tracing::{debug, info, warn};

use crate::k8s_client::K8sClient;

/// Load balancing strategy configuration
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(tag = "type", rename_all = "camelCase")]
pub enum LoadBalancingStrategy {
    /// Least sessions - route to the backend with the fewest active sessions
    LeastSessions,
    /// Label-based arithmetic - evaluate expressions on resource labels
    LabelArithmetic {
        /// Label containing current user count (e.g., "currentUsers")
        current_label: String,
        /// Label containing maximum user count (e.g., "maxUsers")
        max_label: String,
        /// Overlap allowance for concurrent proxy instances (default: 0)
        #[serde(default)]
        overlap: i64,
    },
}

impl Default for LoadBalancingStrategy {
    fn default() -> Self {
        LoadBalancingStrategy::LeastSessions
    }
}

/// Load balancing configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LoadBalancingConfig {
    /// Load balancing strategy to use
    #[serde(default)]
    pub strategy: LoadBalancingStrategy,
}

impl Default for LoadBalancingConfig {
    fn default() -> Self {
        Self {
            strategy: LoadBalancingStrategy::LeastSessions,
        }
    }
}

/// Backend resource information
#[derive(Debug, Clone)]
pub struct Backend {
    /// Resource name
    pub name: String,
    /// Resource address (IP)
    pub address: String,
    /// Number of active sessions to this backend
    pub session_count: usize,
    /// Resource labels for label-based load balancing
    pub labels: std::collections::HashMap<String, String>,
}

/// Load balancer for selecting backend resources
pub struct LoadBalancer {
    /// Strategy to use for load balancing
    strategy: LoadBalancingStrategy,
    /// Track session counts per backend address
    /// Key: backend IP address -> session count
    session_counts: Arc<DashMap<String, usize>>,
    /// K8s client for extracting labels
    k8s_client: K8sClient,
}

impl LoadBalancer {
    /// Create a new load balancer
    pub fn new(strategy: LoadBalancingStrategy, k8s_client: K8sClient) -> Self {
        info!("Load balancer initialized with strategy: {:?}", strategy);
        Self {
            strategy,
            session_counts: Arc::new(DashMap::new()),
            k8s_client,
        }
    }

    /// Select the best backend from a list of resources
    pub fn select_backend(
        &self,
        resources: &[DynamicObject],
        address_path: &str,
        address_type: Option<&str>,
    ) -> Result<DynamicObject> {
        if resources.is_empty() {
            anyhow::bail!("No resources available for load balancing");
        }

        match &self.strategy {
            LoadBalancingStrategy::LeastSessions => {
                self.select_least_sessions(resources, address_path, address_type)
            }
            LoadBalancingStrategy::LabelArithmetic {
                current_label,
                max_label,
                overlap,
            } => self.select_label_arithmetic(
                resources,
                address_path,
                address_type,
                current_label,
                max_label,
                *overlap,
            ),
        }
    }

    /// Select backend using least sessions strategy
    fn select_least_sessions(
        &self,
        resources: &[DynamicObject],
        address_path: &str,
        address_type: Option<&str>,
    ) -> Result<DynamicObject> {
        let mut backends = Vec::new();

        // Build backend list with session counts
        for resource in resources {
            let name = resource
                .metadata
                .name
                .as_ref()
                .unwrap_or(&"unknown".to_string())
                .clone();

            // Extract address
            let address = match self
                .k8s_client
                .extract_address(resource, address_path, address_type)
            {
                Ok(addr) => addr,
                Err(e) => {
                    warn!("Failed to extract address from resource {}: {}", name, e);
                    continue;
                }
            };

            // Get current session count for this backend
            let session_count = self
                .session_counts
                .get(&address)
                .map(|v| *v)
                .unwrap_or(0);

            backends.push((resource.clone(), address, session_count));
        }

        if backends.is_empty() {
            anyhow::bail!("No valid backends found after address extraction");
        }

        // Sort by session count (ascending) and select the first
        backends.sort_by_key(|(_, _, count)| *count);
        let (selected, address, count) = &backends[0];

        let name = selected
            .metadata
            .name
            .as_deref()
            .unwrap_or("unknown");

        debug!(
            "Selected backend '{}' ({}) with {} sessions (least of {} backends)",
            name,
            address,
            count,
            backends.len()
        );

        Ok(selected.clone())
    }

    /// Select backend using label-based arithmetic strategy
    fn select_label_arithmetic(
        &self,
        resources: &[DynamicObject],
        address_path: &str,
        address_type: Option<&str>,
        current_label: &str,
        max_label: &str,
        overlap: i64,
    ) -> Result<DynamicObject> {
        let mut candidates = Vec::new();

        for resource in resources {
            let name = resource
                .metadata
                .name
                .as_deref()
                .unwrap_or("unknown")
                .to_string();

            // Extract address
            let address = match self
                .k8s_client
                .extract_address(resource, address_path, address_type)
            {
                Ok(addr) => addr,
                Err(e) => {
                    warn!("Failed to extract address from resource {}: {}", name, e);
                    continue;
                }
            };

            // Get labels
            let labels = resource
                .metadata
                .labels
                .as_ref()
                .cloned()
                .unwrap_or_default();

            // Extract current and max values from labels
            let current_value = match labels.get(current_label) {
                Some(val) => match val.parse::<i64>() {
                    Ok(v) => v,
                    Err(_) => {
                        warn!(
                            "Backend '{}': label '{}' is not a valid integer: {}",
                            name, current_label, val
                        );
                        continue;
                    }
                },
                None => {
                    debug!(
                        "Backend '{}': missing label '{}', assuming 0",
                        name, current_label
                    );
                    0
                }
            };

            let max_value = match labels.get(max_label) {
                Some(val) => match val.parse::<i64>() {
                    Ok(v) => v,
                    Err(_) => {
                        warn!(
                            "Backend '{}': label '{}' is not a valid integer: {}",
                            name, max_label, val
                        );
                        continue;
                    }
                },
                None => {
                    warn!(
                        "Backend '{}': missing required label '{}', skipping",
                        name, max_label
                    );
                    continue;
                }
            };

            // Get session count for this backend
            let session_count = self
                .session_counts
                .get(&address)
                .map(|v| *v)
                .unwrap_or(0) as i64;

            // Calculate available capacity: max - current - sessions - overlap
            // This ensures: current + sessions + overlap <= max
            let available = max_value - current_value - session_count - overlap;

            debug!(
                "Backend '{}' ({}): current={}, max={}, sessions={}, overlap={}, available={}",
                name, address, current_value, max_value, session_count, overlap, available
            );

            // Only consider backends with available capacity
            if available > 0 {
                candidates.push((resource.clone(), address, available, current_value));
            } else {
                debug!(
                    "Backend '{}' ({}) is at capacity (available={})",
                    name, address, available
                );
            }
        }

        if candidates.is_empty() {
            anyhow::bail!(
                "No backends available with capacity (checked {} resources). \
                All backends may be at max capacity or missing required labels '{}' and '{}'",
                resources.len(),
                current_label,
                max_label
            );
        }

        // Sort by available capacity (descending), then by current load (ascending)
        candidates.sort_by(|a, b| {
            b.2.cmp(&a.2) // More available capacity first
                .then_with(|| a.3.cmp(&b.3)) // Lower current load as tiebreaker
        });

        let (selected, address, available, current) = &candidates[0];
        let name = selected
            .metadata
            .name
            .as_deref()
            .unwrap_or("unknown");

        info!(
            "Selected backend '{}' ({}) with {} available capacity (current={}, {} candidates)",
            name,
            address,
            available,
            current,
            candidates.len()
        );

        Ok(selected.clone())
    }

    /// Increment session count for a backend
    pub fn increment_session(&self, backend_address: &str) {
        let mut entry = self.session_counts.entry(backend_address.to_string()).or_insert(0);
        *entry += 1;
        debug!(
            "Incremented session count for backend {}: {}",
            backend_address, *entry
        );
    }

    /// Decrement session count for a backend
    pub fn decrement_session(&self, backend_address: &str) {
        if let Some(mut entry) = self.session_counts.get_mut(backend_address) {
            if *entry > 0 {
                *entry -= 1;
                debug!(
                    "Decremented session count for backend {}: {}",
                    backend_address, *entry
                );
            }
        }
    }

    /// Get session count for a backend
    pub fn get_session_count(&self, backend_address: &str) -> usize {
        self.session_counts
            .get(backend_address)
            .map(|v| *v)
            .unwrap_or(0)
    }

    /// Get total session count across all backends
    pub fn get_total_sessions(&self) -> usize {
        self.session_counts.iter().map(|entry| *entry.value()).sum()
    }

    /// Clear session count for a backend (used when backend is removed)
    pub fn clear_backend(&self, backend_address: &str) {
        self.session_counts.remove(backend_address);
        debug!("Cleared session count for backend {}", backend_address);
    }

    /// Get all backend addresses and their session counts
    pub fn get_all_session_counts(&self) -> Vec<(String, usize)> {
        self.session_counts
            .iter()
            .map(|entry| (entry.key().clone(), *entry.value()))
            .collect()
    }
}

impl Clone for LoadBalancer {
    fn clone(&self) -> Self {
        Self {
            strategy: self.strategy.clone(),
            session_counts: self.session_counts.clone(),
            k8s_client: self.k8s_client.clone(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    use std::collections::HashMap;

    fn create_mock_resource(name: &str, address: &str, labels: HashMap<String, String>) -> DynamicObject {
        let mut metadata = kube::api::ObjectMeta::default();
        metadata.name = Some(name.to_string());
        metadata.labels = Some(labels.into_iter().collect());

        let data = json!({
            "apiVersion": "v1",
            "kind": "Pod",
            "metadata": metadata,
            "status": {
                "podIP": address
            }
        });

        serde_json::from_value(data).unwrap()
    }

    #[tokio::test]
    async fn test_least_sessions_selection() {
        let k8s_client = match K8sClient::new().await {
            Ok(c) => c,
            Err(_) => return, // Skip if not in k8s environment
        };

        let lb = LoadBalancer::new(LoadBalancingStrategy::LeastSessions, k8s_client);

        // Create mock resources
        let resources = vec![
            create_mock_resource("pod-1", "10.0.0.1", HashMap::new()),
            create_mock_resource("pod-2", "10.0.0.2", HashMap::new()),
            create_mock_resource("pod-3", "10.0.0.3", HashMap::new()),
        ];

        // Simulate sessions on backends
        lb.increment_session("10.0.0.1");
        lb.increment_session("10.0.0.1");
        lb.increment_session("10.0.0.2");

        // Select backend - should pick pod-3 (0 sessions)
        let selected = lb
            .select_backend(&resources, "status.podIP", None)
            .unwrap();
        assert_eq!(selected.metadata.name.as_ref().unwrap(), "pod-3");

        // Add session to pod-3, now pod-2 has least
        lb.increment_session("10.0.0.3");
        lb.increment_session("10.0.0.3");

        let selected = lb
            .select_backend(&resources, "status.podIP", None)
            .unwrap();
        assert_eq!(selected.metadata.name.as_ref().unwrap(), "pod-2");
    }

    #[tokio::test]
    async fn test_label_arithmetic_selection() {
        let k8s_client = match K8sClient::new().await {
            Ok(c) => c,
            Err(_) => return, // Skip if not in k8s environment
        };

        let strategy = LoadBalancingStrategy::LabelArithmetic {
            current_label: "currentUsers".to_string(),
            max_label: "maxUsers".to_string(),
            overlap: 2,
        };
        let lb = LoadBalancer::new(strategy, k8s_client);

        // Create mock resources with labels
        let mut labels1 = HashMap::new();
        labels1.insert("currentUsers".to_string(), "5".to_string());
        labels1.insert("maxUsers".to_string(), "10".to_string());

        let mut labels2 = HashMap::new();
        labels2.insert("currentUsers".to_string(), "8".to_string());
        labels2.insert("maxUsers".to_string(), "10".to_string());

        let mut labels3 = HashMap::new();
        labels3.insert("currentUsers".to_string(), "2".to_string());
        labels3.insert("maxUsers".to_string(), "10".to_string());

        let resources = vec![
            create_mock_resource("pod-1", "10.0.0.1", labels1),
            create_mock_resource("pod-2", "10.0.0.2", labels2),
            create_mock_resource("pod-3", "10.0.0.3", labels3),
        ];

        // Select backend - should pick pod-3 (most available: 10-2-0-2=6)
        let selected = lb
            .select_backend(&resources, "status.podIP", None)
            .unwrap();
        assert_eq!(selected.metadata.name.as_ref().unwrap(), "pod-3");

        // Add sessions to pod-3
        lb.increment_session("10.0.0.3");
        lb.increment_session("10.0.0.3");
        lb.increment_session("10.0.0.3");
        lb.increment_session("10.0.0.3");

        // Now pod-1 should be selected (10-5-0-2=3 vs 10-2-4-2=2)
        let selected = lb
            .select_backend(&resources, "status.podIP", None)
            .unwrap();
        assert_eq!(selected.metadata.name.as_ref().unwrap(), "pod-1");
    }

    #[tokio::test]
    async fn test_label_arithmetic_at_capacity() {
        let k8s_client = match K8sClient::new().await {
            Ok(c) => c,
            Err(_) => return,
        };

        let strategy = LoadBalancingStrategy::LabelArithmetic {
            current_label: "currentUsers".to_string(),
            max_label: "maxUsers".to_string(),
            overlap: 1,
        };
        let lb = LoadBalancer::new(strategy, k8s_client);

        // Create resource at capacity
        let mut labels = HashMap::new();
        labels.insert("currentUsers".to_string(), "9".to_string());
        labels.insert("maxUsers".to_string(), "10".to_string());

        let resources = vec![create_mock_resource("pod-1", "10.0.0.1", labels)];

        // Should fail - no capacity (10-9-0-1=0)
        let result = lb.select_backend(&resources, "status.podIP", None);
        assert!(result.is_err());
    }
}
