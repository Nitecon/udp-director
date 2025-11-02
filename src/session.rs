use dashmap::DashMap;
use std::net::SocketAddr;
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::time::interval;
use tracing::{debug, info};

/// Session information for a client connection
#[derive(Debug, Clone)]
pub struct Session {
    pub target_addr: SocketAddr,
    pub last_activity: Instant,
}

impl Session {
    /// Create a new session
    pub fn new(target_addr: SocketAddr) -> Self {
        Self {
            target_addr,
            last_activity: Instant::now(),
        }
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

/// Session manager for tracking active client sessions
#[derive(Clone)]
pub struct SessionManager {
    sessions: Arc<DashMap<SocketAddr, Session>>,
    #[allow(dead_code)]
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

    /// Get an existing session
    pub fn get(&self, client_addr: &SocketAddr) -> Option<Session> {
        self.sessions.get(client_addr).map(|entry| entry.clone())
    }

    /// Update or create a session (for session reset)
    pub fn upsert(&self, client_addr: SocketAddr, target_addr: SocketAddr) {
        self.sessions
            .entry(client_addr)
            .and_modify(|session| {
                session.target_addr = target_addr;
                session.touch();
            })
            .or_insert_with(|| Session::new(target_addr));

        debug!("Session upserted: {} -> {}", client_addr, target_addr);
    }

    /// Touch a session to update its last activity
    pub fn touch(&self, client_addr: &SocketAddr) {
        if let Some(mut entry) = self.sessions.get_mut(client_addr) {
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
            self.sessions.retain(|client_addr, session| {
                if session.is_timed_out(self.timeout_seconds) {
                    debug!("Session timed out: {}", client_addr);
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
        let session = manager.get(&client_addr).unwrap();
        assert_eq!(session.target_addr, target_addr);
        assert_eq!(manager.count(), 1);
    }

    #[tokio::test]
    async fn test_session_upsert() {
        let manager = SessionManager::new(300);
        let client_addr: SocketAddr = "127.0.0.1:12345".parse().unwrap();
        let target_addr1: SocketAddr = "10.0.0.1:7777".parse().unwrap();
        let target_addr2: SocketAddr = "10.0.0.2:7777".parse().unwrap();

        manager.upsert(client_addr, target_addr1);
        let session = manager.get(&client_addr).unwrap();
        assert_eq!(session.target_addr, target_addr1);

        manager.upsert(client_addr, target_addr2);
        let session = manager.get(&client_addr).unwrap();
        assert_eq!(session.target_addr, target_addr2);
        assert_eq!(manager.count(), 1);
    }

    #[tokio::test]
    async fn test_session_timeout() {
        let manager = SessionManager::new(1); // 1 second timeout
        let client_addr: SocketAddr = "127.0.0.1:12345".parse().unwrap();
        let target_addr: SocketAddr = "10.0.0.1:7777".parse().unwrap();

        manager.upsert(client_addr, target_addr);
        let session = manager.get(&client_addr).unwrap();
        assert!(!session.is_timed_out(1));

        tokio::time::sleep(Duration::from_secs(2)).await;
        let session = manager.get(&client_addr).unwrap();
        assert!(session.is_timed_out(1));
    }
}
