use anyhow::{Context, Result};
use std::net::SocketAddr;
use std::sync::Arc;
use tokio::net::UdpSocket;
use tokio::sync::RwLock;
use tracing::{debug, error, info, warn};

use crate::config::Config;
use crate::k8s_client::K8sClient;
use crate::session::SessionManager;
use crate::token_cache::TokenCache;

/// Cached default endpoint target
#[derive(Clone, Debug)]
struct DefaultEndpointCache {
    address: String,
    port: u16,
}

/// Data Proxy for Phase 2 & 3 (TCP/UDP with session reset)
pub struct DataProxy {
    port: u16,
    token_cache: TokenCache,
    session_manager: SessionManager,
    config: Config,
    k8s_client: K8sClient,
    default_endpoint_cache: Arc<RwLock<Option<DefaultEndpointCache>>>,
}

impl DataProxy {
    /// Create a new data proxy
    pub fn new(
        port: u16,
        token_cache: TokenCache,
        session_manager: SessionManager,
        config: Config,
        k8s_client: K8sClient,
    ) -> Self {
        Self {
            port,
            token_cache,
            session_manager,
            config,
            k8s_client,
            default_endpoint_cache: Arc::new(RwLock::new(None)),
        }
    }

    /// Run the data proxy
    pub async fn run(&self) -> Result<()> {
        // Bind UDP socket
        let socket = Arc::new(
            UdpSocket::bind(format!("0.0.0.0:{}", self.port))
                .await
                .with_context(|| format!("Failed to bind data proxy to port {}", self.port))?,
        );

        info!("Data proxy listening on UDP port {}", self.port);

        // Main packet processing loop
        let mut buffer = vec![0u8; 65535]; // Max UDP packet size

        loop {
            match socket.recv_from(&mut buffer).await {
                Ok((len, client_addr)) => {
                    let packet_data = buffer[..len].to_vec();
                    let socket_clone = socket.clone();
                    let proxy = self.clone();

                    tokio::spawn(async move {
                        if let Err(e) = proxy
                            .handle_packet(socket_clone, client_addr, packet_data)
                            .await
                        {
                            error!("Error handling packet from {}: {}", client_addr, e);
                        }
                    });
                }
                Err(e) => {
                    error!("Error receiving UDP packet: {}", e);
                }
            }
        }
    }

    /// Handle a single packet
    async fn handle_packet(
        &self,
        socket: Arc<UdpSocket>,
        client_addr: SocketAddr,
        packet_data: Vec<u8>,
    ) -> Result<()> {
        // Get magic bytes
        let magic_bytes = self.config.get_magic_bytes()?;

        // Check if this is a control packet (R-3.2.3)
        if packet_data.starts_with(&magic_bytes) {
            self.handle_control_packet(client_addr, &packet_data, &magic_bytes)
                .await?;
            return Ok(());
        }

        // This is a data packet (R-3.2.4)
        self.handle_data_packet(socket, client_addr, packet_data)
            .await
    }

    /// Handle a control packet (session reset)
    async fn handle_control_packet(
        &self,
        client_addr: SocketAddr,
        packet_data: &[u8],
        magic_bytes: &[u8],
    ) -> Result<()> {
        debug!("Control packet received from {}", client_addr);

        // Strip magic bytes and extract token
        let token_bytes = &packet_data[magic_bytes.len()..];
        let token = String::from_utf8_lossy(token_bytes).to_string();

        // Look up the token
        match self.token_cache.lookup(&token).await {
            Some(target) => {
                // Valid token - update session
                let target_addr = target.to_socket_addr()?;
                self.session_manager.upsert(client_addr, target_addr);
                info!(
                    "Session reset: {} -> {} (token: {})",
                    client_addr,
                    target_addr,
                    &token[..8.min(token.len())]
                );
            }
            None => {
                // Invalid token - drop packet and log error
                warn!(
                    "Invalid token in control packet from {}: {}",
                    client_addr,
                    &token[..8.min(token.len())]
                );
            }
        }

        Ok(())
    }

    /// Handle a data packet (standard proxy)
    async fn handle_data_packet(
        &self,
        socket: Arc<UdpSocket>,
        client_addr: SocketAddr,
        packet_data: Vec<u8>,
    ) -> Result<()> {
        // Check if session exists
        if let Some(session) = self.session_manager.get(&client_addr) {
            // Session exists - proxy the packet
            self.proxy_packet(socket, client_addr, session.target_addr, packet_data)
                .await?;
            self.session_manager.touch(&client_addr);
        } else {
            // No session exists - this is the first packet
            self.handle_first_packet(socket, client_addr, packet_data)
                .await?;
        }

        Ok(())
    }

    /// Handle the first packet from a client (session establishment)
    async fn handle_first_packet(
        &self,
        socket: Arc<UdpSocket>,
        client_addr: SocketAddr,
        packet_data: Vec<u8>,
    ) -> Result<()> {
        // Try to interpret the entire packet as a token
        let potential_token = String::from_utf8_lossy(&packet_data).to_string();

        match self.token_cache.lookup(&potential_token).await {
            Some(target) => {
                // Valid token - create session and consume packet
                let target_addr = target.to_socket_addr()?;
                self.session_manager.upsert(client_addr, target_addr);
                info!(
                    "New session established: {} -> {} (token: {})",
                    client_addr,
                    target_addr,
                    &potential_token[..8.min(potential_token.len())]
                );
                // Token packet is consumed, not forwarded
            }
            None => {
                // Not a valid token - route to default endpoint
                debug!(
                    "No valid token found, routing to default endpoint for {}",
                    client_addr
                );

                // Check cache first
                let cached_endpoint = self.default_endpoint_cache.read().await;
                let target_addr = if let Some(cache) = cached_endpoint.as_ref() {
                    // Use cached endpoint
                    debug!(
                        "Using cached default endpoint: {}:{}",
                        cache.address, cache.port
                    );
                    format!("{}:{}", cache.address, cache.port).parse()?
                } else {
                    // Cache miss - need to query and cache
                    drop(cached_endpoint); // Release read lock

                    debug!("Cache miss, querying for default endpoint");
                    let (address, port) = self.query_default_endpoint().await?;

                    // Cache the result
                    let mut cache_write = self.default_endpoint_cache.write().await;
                    *cache_write = Some(DefaultEndpointCache {
                        address: address.clone(),
                        port,
                    });
                    drop(cache_write);

                    info!("Cached default endpoint: {}:{}", address, port);
                    format!("{}:{}", address, port).parse()?
                };

                self.session_manager.upsert(client_addr, target_addr);

                info!(
                    "New session to default endpoint: {} -> {}",
                    client_addr, target_addr
                );

                // Forward this first packet to the default endpoint
                self.proxy_packet(socket, client_addr, target_addr, packet_data)
                    .await?;
            }
        }

        Ok(())
    }

    /// Query Kubernetes for the default endpoint
    async fn query_default_endpoint(&self) -> Result<(String, u16)> {
        let default_endpoint = self.config.get_default_endpoint();

        // Look up the resource mapping for the default endpoint
        let mapping = self
            .config
            .resource_query_mapping
            .get(&default_endpoint.resource_type)
            .ok_or_else(|| {
                anyhow::anyhow!(
                    "Unknown resource type in default_endpoint: {}",
                    default_endpoint.resource_type
                )
            })?;

        debug!(
            "Querying for default endpoint: type={}, namespace={}, labels={:?}",
            default_endpoint.resource_type,
            default_endpoint.namespace,
            default_endpoint.label_selector
        );

        // Convert status query if present
        let status_query =
            default_endpoint
                .status_query
                .as_ref()
                .map(|sq| crate::k8s_client::StatusQuery {
                    json_path: sq.json_path.clone(),
                    expected_values: sq.expected_values.clone(),
                });

        // Query for matching resources
        let resources = self
            .k8s_client
            .query_resources(
                &default_endpoint.namespace,
                mapping,
                status_query.as_ref(),
                default_endpoint.label_selector.as_ref(),
            )
            .await?;

        debug!("Query returned {} resources", resources.len());

        if resources.is_empty() {
            anyhow::bail!("No matching resources found for default endpoint");
        }

        // Select the first matching resource
        let selected_resource = &resources[0];

        // Extract address and port using the same logic as query server
        let (cluster_ip, port) = if let Some(address_path) = &mapping.address_path {
            // Direct resource approach
            let address = self
                .k8s_client
                .extract_address(selected_resource, address_path)?;
            let port = self.k8s_client.extract_port(
                selected_resource,
                mapping.port_path.as_deref(),
                mapping.port_name.as_deref(),
            )?;
            (address, port)
        } else {
            // Service-based approach
            let resource_name = selected_resource
                .metadata
                .name
                .clone()
                .unwrap_or_else(|| "unknown".to_string());
            let service_selector = mapping.service_selector_label.as_ref().ok_or_else(|| {
                anyhow::anyhow!("service_selector_label required for service-based approach")
            })?;
            let service_port_name = mapping.service_target_port_name.as_ref().ok_or_else(|| {
                anyhow::anyhow!("service_target_port_name required for service-based approach")
            })?;

            self.k8s_client
                .find_service_for_resource(
                    &default_endpoint.namespace,
                    &resource_name,
                    service_selector,
                    service_port_name,
                )
                .await?
                .ok_or_else(|| anyhow::anyhow!("No service found for default endpoint resource"))?
        };

        Ok((cluster_ip, port))
    }

    /// Proxy a packet to the target
    async fn proxy_packet(
        &self,
        socket: Arc<UdpSocket>,
        client_addr: SocketAddr,
        target_addr: SocketAddr,
        packet_data: Vec<u8>,
    ) -> Result<()> {
        debug!(
            "Proxying packet: {} -> {} ({} bytes)",
            client_addr,
            target_addr,
            packet_data.len()
        );

        // Send packet to target
        socket.send_to(&packet_data, target_addr).await?;

        // Note: For full bi-directional proxying, we would need to:
        // 1. Create a dedicated socket for each session
        // 2. Listen for responses from the target
        // 3. Forward responses back to the client
        // This is a simplified implementation that handles client->target direction

        Ok(())
    }
}

// Manual Clone implementation
impl Clone for DataProxy {
    fn clone(&self) -> Self {
        Self {
            port: self.port,
            token_cache: self.token_cache.clone(),
            session_manager: self.session_manager.clone(),
            config: self.config.clone(),
            k8s_client: self.k8s_client.clone(),
            default_endpoint_cache: self.default_endpoint_cache.clone(),
        }
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn test_magic_bytes_detection() {
        let magic_bytes = vec![0xFF, 0xFF, 0xFF, 0xFF, 0x52, 0x45, 0x53, 0x45, 0x54];
        let mut packet = magic_bytes.clone();
        packet.extend_from_slice(b"test-token-123");

        assert!(packet.starts_with(&magic_bytes));

        let token_bytes = &packet[magic_bytes.len()..];
        let token = String::from_utf8_lossy(token_bytes);
        assert_eq!(token, "test-token-123");
    }
}
