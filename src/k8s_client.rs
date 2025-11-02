use anyhow::{Context, Result};
use k8s_openapi::api::core::v1::Service;
use kube::{
    Client,
    api::{Api, DynamicObject, ListParams},
    discovery::ApiResource,
};
use serde_json::Value;
use std::collections::HashMap;
use tracing::{debug, info};

use crate::config::ResourceMapping;

/// Kubernetes client wrapper
#[derive(Clone)]
pub struct K8sClient {
    client: Client,
}

impl K8sClient {
    /// Create a new Kubernetes client using in-cluster configuration
    pub async fn new() -> Result<Self> {
        let client = Client::try_default().await.context(
            "Failed to create Kubernetes client. Ensure running in-cluster or KUBECONFIG is set",
        )?;

        info!("Kubernetes client initialized successfully");
        Ok(Self { client })
    }

    /// Query for resources matching the given criteria
    pub async fn query_resources(
        &self,
        namespace: &str,
        mapping: &ResourceMapping,
        status_query: Option<&StatusQuery>,
        label_selector: Option<&HashMap<String, String>>,
    ) -> Result<Vec<DynamicObject>> {
        // Create API resource definition
        let api_resource = ApiResource {
            group: mapping.group.clone(),
            version: mapping.version.clone(),
            api_version: if mapping.group.is_empty() {
                mapping.version.clone()
            } else {
                format!("{}/{}", mapping.group, mapping.version)
            },
            kind: String::new(), // Not needed for dynamic queries
            plural: mapping.resource.clone(),
        };

        let api: Api<DynamicObject> =
            Api::namespaced_with(self.client.clone(), namespace, &api_resource);

        // Build label selector string
        let label_selector_str = label_selector.map(|labels| {
            labels
                .iter()
                .map(|(k, v)| format!("{}={}", k, v))
                .collect::<Vec<_>>()
                .join(",")
        });

        let mut list_params = ListParams::default();
        if let Some(selector) = label_selector_str {
            list_params = list_params.labels(&selector);
        }

        // List resources
        let resource_list = api
            .list(&list_params)
            .await
            .with_context(|| format!("Failed to list resources: {}", mapping.resource))?;

        debug!(
            "Found {} resources of type {}",
            resource_list.items.len(),
            mapping.resource
        );

        // Filter by status query if provided
        let filtered = if let Some(query) = status_query {
            resource_list
                .items
                .into_iter()
                .filter(|resource| self.matches_status_query(resource, query))
                .collect()
        } else {
            resource_list.items
        };

        debug!("After filtering: {} resources match", filtered.len());
        Ok(filtered)
    }

    /// Check if a resource matches the status query
    fn matches_status_query(&self, resource: &DynamicObject, query: &StatusQuery) -> bool {
        // Parse the JSONPath and extract the value
        let resource_json = serde_json::to_value(resource).ok();
        if resource_json.is_none() {
            return false;
        }

        let value = self.extract_json_path(&resource_json.unwrap(), &query.json_path);

        match value {
            Some(Value::String(s)) => query.expected_values.iter().any(|expected| expected == &s),
            Some(Value::Number(n)) => {
                let n_str = n.to_string();
                query
                    .expected_values
                    .iter()
                    .any(|expected| expected == &n_str)
            }
            Some(Value::Bool(b)) => {
                let b_str = b.to_string();
                query
                    .expected_values
                    .iter()
                    .any(|expected| expected == &b_str)
            }
            _ => false,
        }
    }

    /// Extract a value from JSON using a simple JSONPath-like syntax
    /// Supports paths like "status.state" or "metadata.name"
    fn extract_json_path(&self, json: &Value, path: &str) -> Option<Value> {
        let parts: Vec<&str> = path.split('.').collect();
        let mut current = json;

        for part in parts {
            current = current.get(part)?;
        }

        Some(current.clone())
    }

    /// Extract address from a resource using JSONPath
    pub fn extract_address(&self, resource: &DynamicObject, address_path: &str) -> Result<String> {
        let resource_json =
            serde_json::to_value(resource).context("Failed to serialize resource to JSON")?;

        let value = self
            .extract_json_path(&resource_json, address_path)
            .context(format!(
                "Failed to extract address from path: {}",
                address_path
            ))?;

        match value {
            Value::String(s) => Ok(s),
            _ => anyhow::bail!("Address path did not resolve to a string: {}", address_path),
        }
    }

    /// Extract port from a resource using JSONPath or port name
    pub fn extract_port(
        &self,
        resource: &DynamicObject,
        port_path: Option<&str>,
        port_name: Option<&str>,
    ) -> Result<u16> {
        let resource_json =
            serde_json::to_value(resource).context("Failed to serialize resource to JSON")?;

        // If port_name is provided, look it up in status.ports array
        if let Some(name) = port_name {
            if let Some(Value::Object(status)) = resource_json.get("status") {
                if let Some(Value::Array(ports)) = status.get("ports") {
                    for port in ports {
                        if let Some(Value::String(port_name_val)) = port.get("name") {
                            if port_name_val == name {
                                if let Some(Value::Number(port_num)) = port.get("port") {
                                    return port_num
                                        .as_u64()
                                        .and_then(|n| u16::try_from(n).ok())
                                        .context("Port number out of range");
                                }
                            }
                        }
                    }
                }
            }
            anyhow::bail!("Port with name '{}' not found in resource", name);
        }

        // Otherwise use port_path
        if let Some(path) = port_path {
            let value = self
                .extract_json_path(&resource_json, path)
                .context(format!("Failed to extract port from path: {}", path))?;

            match value {
                Value::Number(n) => n
                    .as_u64()
                    .and_then(|n| u16::try_from(n).ok())
                    .context("Port number out of range"),
                _ => anyhow::bail!("Port path did not resolve to a number: {}", path),
            }
        } else {
            anyhow::bail!("Either port_path or port_name must be provided");
        }
    }

    /// Find a service for a given resource
    pub async fn find_service_for_resource(
        &self,
        namespace: &str,
        resource_name: &str,
        selector_label: &str,
        port_name: &str,
    ) -> Result<Option<(String, u16)>> {
        let services: Api<Service> = Api::namespaced(self.client.clone(), namespace);

        // List all services in the namespace
        let service_list = services
            .list(&ListParams::default())
            .await
            .context("Failed to list services")?;

        // Find a service with the matching label
        for service in service_list.items {
            if let Some(labels) = &service.metadata.labels {
                if let Some(label_value) = labels.get(selector_label) {
                    if label_value == resource_name {
                        // Found matching service, extract cluster IP and port
                        if let Some(spec) = &service.spec {
                            let cluster_ip = spec.cluster_ip.clone().unwrap_or_default();

                            // Find the port by name
                            if let Some(ports) = &spec.ports {
                                for port in ports {
                                    if let Some(name) = &port.name {
                                        if name == port_name {
                                            let port_number = port.port;
                                            debug!(
                                                "Found service {} with IP {} and port {}",
                                                service
                                                    .metadata
                                                    .name
                                                    .as_ref()
                                                    .unwrap_or(&"unknown".to_string()),
                                                cluster_ip,
                                                port_number
                                            );
                                            return Ok(Some((cluster_ip, port_number as u16)));
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }

        debug!(
            "No service found for resource {} with label {}",
            resource_name, selector_label
        );
        Ok(None)
    }

    /// Resolve a service name to cluster IP and port
    #[allow(dead_code)]
    pub async fn resolve_service(
        &self,
        service_name: &str,
        namespace: &str,
        port: u16,
    ) -> Result<(String, u16)> {
        let services: Api<Service> = Api::namespaced(self.client.clone(), namespace);

        let service = services
            .get(service_name)
            .await
            .with_context(|| format!("Failed to get service {}.{}", service_name, namespace))?;

        let cluster_ip = service
            .spec
            .and_then(|spec| spec.cluster_ip)
            .context("Service has no cluster IP")?;

        Ok((cluster_ip, port))
    }
}

/// Status query for filtering resources
#[derive(Debug, Clone)]
pub struct StatusQuery {
    pub json_path: String,
    pub expected_values: Vec<String>,
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[tokio::test]
    async fn test_extract_json_path() {
        // Create a mock client for testing
        let client = match K8sClient::new().await {
            Ok(c) => c,
            Err(_) => {
                // Skip test if not in a k8s environment
                return;
            }
        };

        let json = json!({
            "status": {
                "state": "Allocated"
            },
            "metadata": {
                "name": "test-server"
            }
        });

        let value = client.extract_json_path(&json, "status.state");
        assert_eq!(value, Some(Value::String("Allocated".to_string())));

        let value = client.extract_json_path(&json, "metadata.name");
        assert_eq!(value, Some(Value::String("test-server".to_string())));

        let value = client.extract_json_path(&json, "nonexistent.path");
        assert_eq!(value, None);
    }
}
