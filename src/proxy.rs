use anyhow::{Context, Result};
use std::collections::HashMap;
use std::net::SocketAddr;
use std::sync::Arc;
use tokio::net::UdpSocket;
use tokio::sync::RwLock;
use tracing::{debug, error, info, warn};

use crate::config::{Config, DataPortConfig, Protocol};
use crate::k8s_client::K8sClient;
use crate::session::SessionManager;
use crate::token_cache::TokenCache;

/// Cached default endpoint target with multi-port support
#[derive(Clone, Debug)]
pub(crate) struct DefaultEndpointCache {
    address: String,
    /// Port mappings: (proxy_port, protocol) -> target_port
    port_mappings: HashMap<(u16, Protocol), u16>,
}

/// Data Proxy for Phase 2 & 3 (TCP/UDP with session reset) - Multi-port support
pub struct DataProxy {
    data_ports: Vec<DataPortConfig>,
    token_cache: TokenCache,
    session_manager: SessionManager,
    config: Config,
    k8s_client: K8sClient,
    default_endpoint_cache: Arc<RwLock<Option<DefaultEndpointCache>>>,
}

/// Shared cache for default endpoint that can be invalidated
#[derive(Clone)]
pub struct DefaultEndpointCacheHandle {
    cache: Arc<RwLock<Option<DefaultEndpointCache>>>,
}

impl DefaultEndpointCacheHandle {
    /// Create a new cache handle
    pub fn new() -> Self {
        Self {
            cache: Arc::new(RwLock::new(None)),
        }
    }

    /// Invalidate the cache
    pub async fn invalidate(&self) {
        let mut cache = self.cache.write().await;
        *cache = None;
    }

    /// Get the cache
    pub fn get_cache(&self) -> Arc<RwLock<Option<DefaultEndpointCache>>> {
        self.cache.clone()
    }
}

impl DataProxy {
    /// Create a new multi-port data proxy
    pub fn new(
        token_cache: TokenCache,
        session_manager: SessionManager,
        config: Config,
        k8s_client: K8sClient,
        cache_handle: DefaultEndpointCacheHandle,
    ) -> Self {
        let data_ports = config.get_data_ports();
        Self {
            data_ports,
            token_cache,
            session_manager,
            config,
            k8s_client,
            default_endpoint_cache: cache_handle.get_cache(),
        }
    }

    /// Run the multi-port data proxy
    pub async fn run(&self) -> Result<()> {
        let mut tasks = vec![];

        // Bind and spawn a task for each data port
        for port_config in &self.data_ports {
            let socket = Arc::new(
                UdpSocket::bind(format!("0.0.0.0:{}", port_config.port))
                    .await
                    .with_context(|| {
                        format!(
                            "Failed to bind data proxy to port {} ({})",
                            port_config.port, port_config.protocol
                        )
                    })?,
            );

            info!(
                "Data proxy listening on {} port {}",
                port_config.protocol, port_config.port
            );

            let proxy = self.clone();
            let port = port_config.port;
            let protocol = port_config.protocol;

            let task = tokio::spawn(async move { proxy.run_socket(socket, port, protocol).await });

            tasks.push(task);
        }

        // Wait for all tasks to complete
        futures::future::join_all(tasks).await;
        Ok(())
    }

    /// Run a single socket listener
    async fn run_socket(
        &self,
        socket: Arc<UdpSocket>,
        proxy_port: u16,
        protocol: Protocol,
    ) -> Result<()> {
        let mut buffer = vec![0u8; 65535]; // Max UDP packet size

        loop {
            match socket.recv_from(&mut buffer).await {
                Ok((len, client_addr)) => {
                    let packet_data = buffer[..len].to_vec();
                    let socket_clone = socket.clone();
                    let proxy = self.clone();

                    tokio::spawn(async move {
                        if let Err(e) = proxy
                            .handle_packet_with_port(
                                socket_clone,
                                client_addr,
                                packet_data,
                                proxy_port,
                                protocol,
                            )
                            .await
                        {
                            error!(
                                "Error handling packet from {} on port {} ({}): {}",
                                client_addr, proxy_port, protocol, e
                            );
                        }
                    });
                }
                Err(e) => {
                    error!(
                        "Error receiving packet on port {} ({}): {}",
                        proxy_port, protocol, e
                    );
                }
            }
        }
    }

    /// Handle a single packet with port and protocol information
    async fn handle_packet_with_port(
        &self,
        socket: Arc<UdpSocket>,
        client_addr: SocketAddr,
        packet_data: Vec<u8>,
        proxy_port: u16,
        protocol: Protocol,
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
        self.handle_data_packet(socket, client_addr, packet_data, proxy_port, protocol)
            .await
    }

    /// Handle a control packet (session reset) - supports multi-port
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
                // Valid token - update multi-port session
                self.session_manager.upsert_multi_port(
                    client_addr,
                    target.cluster_ip.clone(),
                    target.port_mappings.clone(),
                );
                info!(
                    "Multi-port session reset: {} -> {} ({} ports, token: {})",
                    client_addr,
                    target.cluster_ip,
                    target.port_mappings.len(),
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

    /// Handle a data packet (standard proxy) with port-specific routing
    async fn handle_data_packet(
        &self,
        socket: Arc<UdpSocket>,
        client_addr: SocketAddr,
        packet_data: Vec<u8>,
        proxy_port: u16,
        protocol: Protocol,
    ) -> Result<()> {
        // Check if session exists for this port and protocol
        if let Some(session) = self.session_manager.get(&client_addr, proxy_port, protocol) {
            // Session exists - get target address for this port
            let target_addr = session.get_target_addr(proxy_port, protocol)?;

            // Proxy the packet
            self.proxy_packet(socket, client_addr, target_addr, packet_data)
                .await?;
            self.session_manager
                .touch(&client_addr, proxy_port, protocol);
        } else {
            // No session exists - this is the first packet
            self.handle_first_packet(socket, client_addr, packet_data, proxy_port, protocol)
                .await?;
        }

        Ok(())
    }

    /// Handle the first packet from a client (session establishment) with port-specific routing
    async fn handle_first_packet(
        &self,
        socket: Arc<UdpSocket>,
        client_addr: SocketAddr,
        packet_data: Vec<u8>,
        proxy_port: u16,
        protocol: Protocol,
    ) -> Result<()> {
        // Try to interpret the entire packet as a token
        let potential_token = String::from_utf8_lossy(&packet_data).to_string();

        match self.token_cache.lookup(&potential_token).await {
            Some(target) => {
                // Valid token - create multi-port session
                self.session_manager.upsert_multi_port(
                    client_addr,
                    target.cluster_ip.clone(),
                    target.port_mappings.clone(),
                );
                info!(
                    "New multi-port session established: {} -> {} ({} ports, token: {})",
                    client_addr,
                    target.cluster_ip,
                    target.port_mappings.len(),
                    &potential_token[..8.min(potential_token.len())]
                );
                // Token packet is consumed, not forwarded
            }
            None => {
                // Not a valid token - route to default endpoint
                debug!(
                    "No valid token found, routing to default endpoint for {} on port {} ({})",
                    client_addr, proxy_port, protocol
                );

                // Check cache first
                let cached_endpoint = self.default_endpoint_cache.read().await;
                let target_addr = if let Some(cache) = cached_endpoint.as_ref() {
                    // Use cached endpoint for this port
                    if let Some(target_port) = cache.port_mappings.get(&(proxy_port, protocol)) {
                        let addr = format!("{}:{}", cache.address, target_port).parse()?;
                        debug!(
                            "Using cached default endpoint: {} for port {} ({})",
                            addr, proxy_port, protocol
                        );
                        addr
                    } else {
                        // Port not in cache, need to query
                        drop(cached_endpoint);
                        let (address, port_mappings) = self.query_default_endpoint().await?;
                        let target_port =
                            port_mappings.get(&(proxy_port, protocol)).ok_or_else(|| {
                                anyhow::anyhow!(
                                    "Default endpoint does not support port {} ({})",
                                    proxy_port,
                                    protocol
                                )
                            })?;
                        format!("{}:{}", address, target_port).parse()?
                    }
                } else {
                    // Cache miss - need to query and cache
                    drop(cached_endpoint); // Release read lock

                    debug!("Cache miss, querying for default endpoint");
                    let (address, port_mappings) = self.query_default_endpoint().await?;

                    // Cache the result
                    let mut cache_write = self.default_endpoint_cache.write().await;
                    *cache_write = Some(DefaultEndpointCache {
                        address: address.clone(),
                        port_mappings: port_mappings.clone(),
                    });
                    drop(cache_write);

                    info!(
                        "Cached default endpoint: {} ({} ports)",
                        address,
                        port_mappings.len()
                    );

                    let target_port =
                        port_mappings.get(&(proxy_port, protocol)).ok_or_else(|| {
                            anyhow::anyhow!(
                                "Default endpoint does not support port {} ({})",
                                proxy_port,
                                protocol
                            )
                        })?;
                    format!("{}:{}", address, target_port).parse()?
                };

                // Create single-port session for default endpoint
                self.session_manager.upsert(client_addr, target_addr);

                info!(
                    "New session to default endpoint: {} -> {} (port {} {})",
                    client_addr, target_addr, proxy_port, protocol
                );

                // Forward this first packet to the default endpoint
                self.proxy_packet(socket, client_addr, target_addr, packet_data)
                    .await?;
            }
        }

        Ok(())
    }

    /// Query Kubernetes for the default endpoint with multi-port support
    async fn query_default_endpoint(&self) -> Result<(String, HashMap<(u16, Protocol), u16>)> {
        let default_endpoint = self.config.get_default_endpoint();

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

        let status_query =
            default_endpoint
                .status_query
                .as_ref()
                .map(|sq| crate::k8s_client::StatusQuery {
                    json_path: sq.json_path.clone(),
                    expected_values: sq.expected_values.clone(),
                });

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

        let selected_resource = &resources[0];
        self.extract_endpoint_target_multi_port(
            selected_resource,
            mapping,
            &default_endpoint.namespace,
        )
        .await
    }

    /// Extract target address and ports from a resource (multi-port)
    async fn extract_endpoint_target_multi_port(
        &self,
        resource: &kube::api::DynamicObject,
        mapping: &crate::config::ResourceMapping,
        _namespace: &str,
    ) -> Result<(String, HashMap<(u16, Protocol), u16>)> {
        let address_path = mapping
            .address_path
            .as_ref()
            .ok_or_else(|| anyhow::anyhow!("address_path is required for default endpoint"))?;

        let address = self.k8s_client.extract_address(
            resource,
            address_path,
            mapping.address_type.as_deref(),
        )?;

        // Check if multi-port configuration exists
        if let Some(port_mappings_config) = &mapping.ports {
            // Multi-port approach
            let ports_map = self
                .k8s_client
                .extract_ports(resource, port_mappings_config)?;

            // Build port mappings
            let data_ports = self.config.get_data_ports();
            let mut port_mappings = HashMap::new();

            for data_port_config in &data_ports {
                if let Some(target_port) = ports_map.get(&data_port_config.name) {
                    port_mappings.insert(
                        (data_port_config.port, data_port_config.protocol),
                        *target_port,
                    );
                }
            }

            Ok((address, port_mappings))
        } else {
            // Single port approach (backwards compatibility)
            let port = self.k8s_client.extract_port(
                resource,
                mapping.port_path.as_deref(),
                mapping.port_name.as_deref(),
            )?;

            let mut port_mappings = HashMap::new();
            let data_ports = self.config.get_data_ports();
            for data_port_config in &data_ports {
                port_mappings.insert((data_port_config.port, data_port_config.protocol), port);
            }

            Ok((address, port_mappings))
        }
    }

    /// Extract target address and port from a resource
    #[allow(dead_code)]
    async fn extract_endpoint_target(
        &self,
        resource: &kube::api::DynamicObject,
        mapping: &crate::config::ResourceMapping,
        namespace: &str,
    ) -> Result<(String, u16)> {
        if let Some(address_path) = &mapping.address_path {
            self.extract_direct_endpoint(resource, mapping, address_path)
        } else {
            self.extract_service_endpoint(resource, mapping, namespace)
                .await
        }
    }

    /// Extract target using direct resource approach
    #[allow(dead_code)]
    fn extract_direct_endpoint(
        &self,
        resource: &kube::api::DynamicObject,
        mapping: &crate::config::ResourceMapping,
        address_path: &str,
    ) -> Result<(String, u16)> {
        let address = self.k8s_client.extract_address(
            resource,
            address_path,
            mapping.address_type.as_deref(),
        )?;
        let port = self.k8s_client.extract_port(
            resource,
            mapping.port_path.as_deref(),
            mapping.port_name.as_deref(),
        )?;
        Ok((address, port))
    }

    /// Extract endpoint using service-based approach
    #[allow(dead_code)]
    async fn extract_service_endpoint(
        &self,
        resource: &kube::api::DynamicObject,
        mapping: &crate::config::ResourceMapping,
        namespace: &str,
    ) -> Result<(String, u16)> {
        let resource_name = resource
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
                namespace,
                &resource_name,
                service_selector,
                service_port_name,
            )
            .await?
            .ok_or_else(|| anyhow::anyhow!("No service found for default endpoint resource"))
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
            data_ports: self.data_ports.clone(),
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
