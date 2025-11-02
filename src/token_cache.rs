use moka::future::Cache;
use std::net::SocketAddr;
use std::sync::Arc;
use std::time::Duration;
use uuid::Uuid;

/// Target information for a token
#[derive(Debug, Clone)]
pub struct TokenTarget {
    pub cluster_ip: String,
    pub port: u16,
}

impl TokenTarget {
    /// Convert to a SocketAddr
    pub fn to_socket_addr(&self) -> Result<SocketAddr, std::io::Error> {
        format!("{}:{}", self.cluster_ip, self.port)
            .parse()
            .map_err(|e| {
                std::io::Error::new(
                    std::io::ErrorKind::InvalidInput,
                    format!("Invalid socket address: {}", e),
                )
            })
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
        let target = TokenTarget {
            cluster_ip: "10.0.0.1".to_string(),
            port: 7777,
        };

        let token = cache.generate_token(target.clone()).await;
        assert!(!token.is_empty());

        let retrieved = cache.lookup(&token).await;
        assert!(retrieved.is_some());

        let retrieved_target = retrieved.unwrap();
        assert_eq!(retrieved_target.cluster_ip, "10.0.0.1");
        assert_eq!(retrieved_target.port, 7777);
    }

    #[tokio::test]
    async fn test_token_ttl() {
        let cache = TokenCache::new(1); // 1 second TTL
        let target = TokenTarget {
            cluster_ip: "10.0.0.1".to_string(),
            port: 7777,
        };

        let token = cache.generate_token(target).await;
        assert!(cache.lookup(&token).await.is_some());

        // Wait for TTL to expire
        tokio::time::sleep(Duration::from_secs(2)).await;
        assert!(cache.lookup(&token).await.is_none());
    }
}
