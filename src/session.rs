use dashmap::DashMap;
use std::collections::HashMap;
use std::net::SocketAddr;
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::time::interval;
use tracing::{debug, info};

use crate::config::Protocol;

/// Session information for a client connection with multi-port support
#[derive(Debug, Clone)]
pub struct Session {
    pub target_ip: String,
    /// Port mappings: (proxy_port, protocol) -> target_port
    pub port_mappings: HashMap<(u16, Protocol), u16>,
    pub last_activity: Instant,
}

impl Session {
    /// Create a new session with a single port (backwards compatibility)
    pub fn new(target_addr: SocketAddr) -> Self {
        let mut port_mappings = HashMap::new();
        port_mappings.insert((target_addr.port(), Protocol::Udp), target_addr.port());
        Self {
            target_ip: target_addr.ip().to_string(),
            port_mappings,
            last_activity: Instant::now(),
        }
    }

    /// Create a new multi-port session
    pub fn new_multi_port(target_ip: String, port_mappings: HashMap<(u16, Protocol), u16>) -> Self {
        Self {
            target_ip,
            port_mappings,
            last_activity: Instant::now(),
        }
    }

    /// Get target address for a specific proxy port and protocol
    pub fn get_target_addr(
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

        format!("{}:{}", self.target_ip, target_port)
            .parse()
            .map_err(|e| {
                std::io::Error::new(
                    std::io::ErrorKind::InvalidInput,
                    format!("Invalid socket address: {}", e),
                )
            })
    }

    /// Update the last activity timestamp
    pub fn touch(&mut self) {
        self.last_activity = Instant::now();
    }

    /// Check if the session has timed out
    pub fn is_timed_out(&self, timeout_seconds: u64) -> bool {
        self.last_activity.elapsed() > Duration::from_secs(timeout_seconds)
    }
}

/// Session manager for tracking active client sessions with multi-port support
#[derive(Clone)]
pub struct SessionManager {
    /// Key: (client_addr, proxy_port, protocol) -> Session
    sessions: Arc<DashMap<(SocketAddr, u16, Protocol), Session>>,
    timeout_seconds: u64,
}

impl SessionManager {
    /// Create a new session manager
    pub fn new(timeout_seconds: u64) -> Self {
        let manager = Self {
            sessions: Arc::new(DashMap::new()),
            timeout_seconds,
        };

        // Start cleanup task
        let cleanup_manager = manager.clone();
        tokio::spawn(async move {
            cleanup_manager.cleanup_loop().await;
        });

        manager
    }

    /// Get an existing session for a specific port and protocol
    pub fn get(
        &self,
        client_addr: &SocketAddr,
        proxy_port: u16,
        protocol: Protocol,
    ) -> Option<Session> {
        self.sessions
            .get(&(*client_addr, proxy_port, protocol))
            .map(|entry| entry.clone())
    }

    /// Update or create a session (for session reset) - single port version
    pub fn upsert(&self, client_addr: SocketAddr, target_addr: SocketAddr) {
        let session = Session::new(target_addr);
        self.sessions.insert(
            (client_addr, target_addr.port(), Protocol::Udp),
            session.clone(),
        );
        debug!("Session upserted: {} -> {}", client_addr, target_addr);
    }

    /// Update or create a multi-port session
    pub fn upsert_multi_port(
        &self,
        client_addr: SocketAddr,
        target_ip: String,
        port_mappings: HashMap<(u16, Protocol), u16>,
    ) {
        let session = Session::new_multi_port(target_ip.clone(), port_mappings.clone());

        // Insert session for all port mappings
        for (proxy_port, protocol) in port_mappings.keys() {
            self.sessions
                .insert((client_addr, *proxy_port, *protocol), session.clone());
        }

        debug!(
            "Multi-port session upserted: {} -> {} ({} ports)",
            client_addr,
            target_ip,
            port_mappings.len()
        );
    }

    /// Touch a session to update its last activity
    pub fn touch(&self, client_addr: &SocketAddr, proxy_port: u16, protocol: Protocol) {
        if let Some(mut entry) = self.sessions.get_mut(&(*client_addr, proxy_port, protocol)) {
            entry.touch();
        }
    }

    /// Get the number of active sessions
    pub fn count(&self) -> usize {
        self.sessions.len()
    }

    /// Clear all sessions (called during shutdown)
    pub fn clear_all(&self) {
        let count = self.sessions.len();
        self.sessions.clear();
        if count > 0 {
            info!("Cleared {} active sessions during shutdown", count);
        }
    }

    /// Cleanup loop to remove timed-out sessions
    async fn cleanup_loop(&self) {
        let mut cleanup_interval = interval(Duration::from_secs(30));

        loop {
            cleanup_interval.tick().await;

            let mut removed_count = 0;
            self.sessions.retain(|key, session| {
                if session.is_timed_out(self.timeout_seconds) {
                    debug!("Session timed out: {:?}", key);
                    removed_count += 1;
                    false
                } else {
                    true
                }
            });

            if removed_count > 0 {
                info!(
                    "Cleaned up {} timed-out sessions. Active sessions: {}",
                    removed_count,
                    self.count()
                );
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_session_creation() {
        let manager = SessionManager::new(300);
        let client_addr: SocketAddr = "127.0.0.1:12345".parse().unwrap();
        let target_addr: SocketAddr = "10.0.0.1:7777".parse().unwrap();

        manager.upsert(client_addr, target_addr);
        let session = manager.get(&client_addr, 7777, Protocol::Udp).unwrap();
        assert_eq!(session.target_ip, "10.0.0.1");
        assert_eq!(manager.count(), 1);
    }

    #[tokio::test]
    async fn test_session_upsert() {
        let manager = SessionManager::new(300);
        let client_addr: SocketAddr = "127.0.0.1:12345".parse().unwrap();
        let target_addr1: SocketAddr = "10.0.0.1:7777".parse().unwrap();
        let target_addr2: SocketAddr = "10.0.0.2:7777".parse().unwrap();

        manager.upsert(client_addr, target_addr1);
        let session = manager.get(&client_addr, 7777, Protocol::Udp).unwrap();
        assert_eq!(session.target_ip, "10.0.0.1");

        manager.upsert(client_addr, target_addr2);
        let session = manager.get(&client_addr, 7777, Protocol::Udp).unwrap();
        assert_eq!(session.target_ip, "10.0.0.2");
        assert_eq!(manager.count(), 1);
    }

    #[tokio::test]
    async fn test_session_timeout() {
        let manager = SessionManager::new(1); // 1 second timeout
        let client_addr: SocketAddr = "127.0.0.1:12345".parse().unwrap();
        let target_addr: SocketAddr = "10.0.0.1:7777".parse().unwrap();

        manager.upsert(client_addr, target_addr);
        let session = manager.get(&client_addr, 7777, Protocol::Udp).unwrap();
        assert!(!session.is_timed_out(1));

        tokio::time::sleep(Duration::from_secs(2)).await;
        let session = manager.get(&client_addr, 7777, Protocol::Udp).unwrap();
        assert!(session.is_timed_out(1));
    }
}
