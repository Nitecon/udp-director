use moka::future::Cache;
use std::collections::HashMap;
use std::net::SocketAddr;
use std::sync::Arc;
use std::time::Duration;
use uuid::Uuid;

use crate::config::Protocol;

/// Target information for a token with multi-port support
#[derive(Debug, Clone)]
pub struct TokenTarget {
    pub cluster_ip: String,
    /// Port mappings: (proxy_port, protocol) -> target_port
    pub port_mappings: HashMap<(u16, Protocol), u16>,
}

impl TokenTarget {
    /// Create a new TokenTarget with a single port (backwards compatibility)
    pub fn single_port(cluster_ip: String, port: u16) -> Self {
        let mut port_mappings = HashMap::new();
        port_mappings.insert((port, Protocol::Udp), port);
        Self {
            cluster_ip,
            port_mappings,
        }
    }

    /// Create a new TokenTarget with multiple ports
    pub fn multi_port(cluster_ip: String, port_mappings: HashMap<(u16, Protocol), u16>) -> Self {
        Self {
            cluster_ip,
            port_mappings,
        }
    }

    /// Convert to a SocketAddr for a specific proxy port and protocol
    pub fn to_socket_addr_for_port(
        &self,
        proxy_port: u16,
        protocol: Protocol,
    ) -> Result<SocketAddr, std::io::Error> {
        let target_port = self
            .port_mappings
            .get(&(proxy_port, protocol))
            .ok_or_else(|| {
                std::io::Error::new(
                    std::io::ErrorKind::NotFound,
                    format!(
                        "No port mapping found for proxy port {} ({})",
                        proxy_port, protocol
                    ),
                )
            })?;

        format!("{}:{}", self.cluster_ip, target_port)
            .parse()
            .map_err(|e| {
                std::io::Error::new(
                    std::io::ErrorKind::InvalidInput,
                    format!("Invalid socket address: {}", e),
                )
            })
    }

    /// Convert to a SocketAddr (backwards compatibility - uses first available port)
    pub fn to_socket_addr(&self) -> Result<SocketAddr, std::io::Error> {
        if let Some(((_proxy_port, _protocol), target_port)) = self.port_mappings.iter().next() {
            format!("{}:{}", self.cluster_ip, target_port)
                .parse()
                .map_err(|e| {
                    std::io::Error::new(
                        std::io::ErrorKind::InvalidInput,
                        format!("Invalid socket address: {}", e),
                    )
                })
        } else {
            Err(std::io::Error::new(
                std::io::ErrorKind::NotFound,
                "No port mappings available",
            ))
        }
    }
}

/// Token cache with TTL support
#[derive(Clone)]
pub struct TokenCache {
    cache: Arc<Cache<String, TokenTarget>>,
}

impl TokenCache {
    /// Create a new token cache with the specified TTL in seconds
    pub fn new(ttl_seconds: u64) -> Self {
        let cache = Cache::builder()
            .time_to_live(Duration::from_secs(ttl_seconds))
            .build();

        Self {
            cache: Arc::new(cache),
        }
    }

    /// Generate a new token and store the target
    pub async fn generate_token(&self, target: TokenTarget) -> String {
        let token = Uuid::new_v4().to_string();
        self.cache.insert(token.clone(), target).await;
        token
    }
    /// Look up a token and return the target if valid
    pub async fn lookup(&self, token: &str) -> Option<TokenTarget> {
        self.cache.get(token).await
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_token_generation_and_lookup() {
        let cache = TokenCache::new(60);
        let target = TokenTarget::single_port("10.0.0.1".to_string(), 7777);

        let token = cache.generate_token(target.clone()).await;
        assert!(!token.is_empty());

        let retrieved = cache.lookup(&token).await;
        assert!(retrieved.is_some());

        let retrieved_target = retrieved.unwrap();
        assert_eq!(retrieved_target.cluster_ip, "10.0.0.1");
        assert_eq!(retrieved_target.port_mappings.len(), 1);
    }

    #[tokio::test]
    async fn test_token_ttl() {
        let cache = TokenCache::new(1); // 1 second TTL
        let target = TokenTarget::single_port("10.0.0.1".to_string(), 7777);

        let token = cache.generate_token(target).await;
        assert!(cache.lookup(&token).await.is_some());

        // Wait for TTL to expire
        tokio::time::sleep(Duration::from_secs(2)).await;
        assert!(cache.lookup(&token).await.is_none());
    }
}
