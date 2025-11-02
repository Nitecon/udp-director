use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{TcpListener, TcpStream};
use tracing::{debug, error, info};

use crate::config::Config;
use crate::k8s_client::{K8sClient, StatusQuery};
use crate::token_cache::{TokenCache, TokenTarget};

/// Query request from client
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct QueryRequest {
    pub resource_type: String,
    pub namespace: String,
    pub status_query: Option<StatusQueryDto>,
    pub label_selector: Option<HashMap<String, String>>,
}

/// Status query DTO
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct StatusQueryDto {
    pub json_path: String,
    pub expected_values: Vec<String>,
}

/// Query response to client
#[derive(Debug, Serialize)]
#[serde(untagged)]
pub enum QueryResponse {
    Success { token: String },
    Error { error: String },
}

/// TCP Query Server (Phase 1)
pub struct QueryServer {
    port: u16,
    k8s_client: K8sClient,
    token_cache: TokenCache,
    config: Config,
}

impl QueryServer {
    /// Create a new query server
    pub fn new(port: u16, k8s_client: K8sClient, token_cache: TokenCache, config: Config) -> Self {
        Self {
            port,
            k8s_client,
            token_cache,
            config,
        }
    }

    /// Run the query server
    pub async fn run(&self) -> Result<()> {
        let listener = TcpListener::bind(format!("0.0.0.0:{}", self.port))
            .await
            .with_context(|| format!("Failed to bind query server to port {}", self.port))?;

        info!("Query server listening on port {}", self.port);

        loop {
            match listener.accept().await {
                Ok((stream, addr)) => {
                    debug!("New query connection from {}", addr);
                    let server = self.clone();
                    tokio::spawn(async move {
                        if let Err(e) = server.handle_connection(stream).await {
                            error!("Error handling query connection: {}", e);
                        }
                    });
                }
                Err(e) => {
                    error!("Failed to accept connection: {}", e);
                }
            }
        }
    }

    /// Handle a single query connection
    async fn handle_connection(&self, mut stream: TcpStream) -> Result<()> {
        // Read the JSON payload
        let mut buffer = vec![0u8; 4096];
        let n = stream
            .read(&mut buffer)
            .await
            .context("Failed to read from stream")?;

        if n == 0 {
            return Ok(());
        }

        let request_data = &buffer[..n];
        let request: QueryRequest = match serde_json::from_slice(request_data) {
            Ok(req) => req,
            Err(e) => {
                let response = QueryResponse::Error {
                    error: format!("Invalid JSON: {}", e),
                };
                let response_json = serde_json::to_string(&response)?;
                stream.write_all(response_json.as_bytes()).await?;
                return Ok(());
            }
        };

        debug!("Received query: {:?}", request);

        // Process the query
        let response = self.process_query(request).await;
        let response_json = serde_json::to_string(&response)?;

        // Send response
        stream.write_all(response_json.as_bytes()).await?;
        stream.flush().await?;

        Ok(())
    }

    /// Process a query request
    async fn process_query(&self, request: QueryRequest) -> QueryResponse {
        // Look up the resource mapping
        let mapping = match self
            .config
            .resource_query_mapping
            .get(&request.resource_type)
        {
            Some(m) => m,
            None => {
                return QueryResponse::Error {
                    error: format!("Unknown resource type: {}", request.resource_type),
                };
            }
        };

        // Convert status query
        let status_query = request.status_query.as_ref().map(|sq| StatusQuery {
            json_path: sq.json_path.clone(),
            expected_values: sq.expected_values.clone(),
        });

        // Query Kubernetes for matching resources
        let resources = match self
            .k8s_client
            .query_resources(
                &request.namespace,
                mapping,
                status_query.as_ref(),
                request.label_selector.as_ref(),
            )
            .await
        {
            Ok(res) => res,
            Err(e) => {
                return QueryResponse::Error {
                    error: format!("Failed to query resources: {}", e),
                };
            }
        };

        if resources.is_empty() {
            return QueryResponse::Error {
                error: "No matching resources found".to_string(),
            };
        }

        // Select the first matching resource (could be randomized or load-balanced)
        let selected_resource = &resources[0];
        let resource_name = selected_resource
            .metadata
            .name
            .clone()
            .unwrap_or_else(|| "unknown".to_string());

        debug!("Selected resource: {}", resource_name);

        // Determine which approach to use: direct resource or service-based
        let (cluster_ip, port) = if let Some(address_path) = &mapping.address_path {
            // DIRECT RESOURCE APPROACH
            debug!(
                "Using direct resource approach with address_path: {}",
                address_path
            );

            // Extract address from resource
            let address = match self
                .k8s_client
                .extract_address(selected_resource, address_path)
            {
                Ok(addr) => addr,
                Err(e) => {
                    return QueryResponse::Error {
                        error: format!("Failed to extract address: {}", e),
                    };
                }
            };

            // Extract port from resource
            let port = match self.k8s_client.extract_port(
                selected_resource,
                mapping.port_path.as_deref(),
                mapping.port_name.as_deref(),
            ) {
                Ok(p) => p,
                Err(e) => {
                    return QueryResponse::Error {
                        error: format!("Failed to extract port: {}", e),
                    };
                }
            };

            debug!("Extracted address: {}, port: {}", address, port);
            (address, port)
        } else {
            // SERVICE-BASED APPROACH (Legacy)
            debug!("Using service-based approach");

            let service_selector = mapping.service_selector_label.as_ref().ok_or_else(|| {
                "service_selector_label is required for service-based approach".to_string()
            });
            let service_port_name = mapping.service_target_port_name.as_ref().ok_or_else(|| {
                "service_target_port_name is required for service-based approach".to_string()
            });

            if let (Ok(selector), Ok(port_name)) = (service_selector, service_port_name) {
                match self
                    .k8s_client
                    .find_service_for_resource(
                        &request.namespace,
                        &resource_name,
                        selector,
                        port_name,
                    )
                    .await
                {
                    Ok(Some(info)) => info,
                    Ok(None) => {
                        return QueryResponse::Error {
                            error: format!("No service found for resource: {}", resource_name),
                        };
                    }
                    Err(e) => {
                        return QueryResponse::Error {
                            error: format!("Failed to find service: {}", e),
                        };
                    }
                }
            } else {
                return QueryResponse::Error {
                    error: "Invalid configuration: service-based approach requires service_selector_label and service_target_port_name".to_string(),
                };
            }
        };

        // Generate a token
        let target = TokenTarget { cluster_ip, port };
        let token = self.token_cache.generate_token(target).await;

        info!("Generated token for resource: {}", resource_name);

        QueryResponse::Success { token }
    }
}

// Manual Clone implementation since TcpListener is not Clone
impl Clone for QueryServer {
    fn clone(&self) -> Self {
        Self {
            port: self.port,
            k8s_client: self.k8s_client.clone(),
            token_cache: self.token_cache.clone(),
            config: self.config.clone(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_query_request_deserialization() {
        let json = r#"{
            "resourceType": "gameserver",
            "namespace": "game-servers",
            "statusQuery": {
                "jsonPath": "status.state",
                "expectedValues": ["Allocated", "Ready"]
            },
            "labelSelector": {
                "game.example.com/map": "de_dust2"
            }
        }"#;

        let request: QueryRequest = serde_json::from_str(json).unwrap();
        assert_eq!(request.resource_type, "gameserver");
        assert_eq!(request.namespace, "game-servers");
        assert!(request.status_query.is_some());
        assert!(request.label_selector.is_some());

        let status_query = request.status_query.unwrap();
        assert_eq!(status_query.expected_values.len(), 2);
        assert_eq!(status_query.expected_values[0], "Allocated");
        assert_eq!(status_query.expected_values[1], "Ready");
    }

    #[test]
    fn test_query_response_serialization() {
        let response = QueryResponse::Success {
            token: "test-token-123".to_string(),
        };
        let json = serde_json::to_string(&response).unwrap();
        assert!(json.contains("test-token-123"));

        let response = QueryResponse::Error {
            error: "Test error".to_string(),
        };
        let json = serde_json::to_string(&response).unwrap();
        assert!(json.contains("Test error"));
    }
}
