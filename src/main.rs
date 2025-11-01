use anyhow::Result;
use tracing::{info, warn};
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt};

mod config;
mod k8s_client;
mod proxy;
mod query_server;
mod session;
mod token_cache;

use config::Config;
use k8s_client::K8sClient;
use proxy::DataProxy;
use query_server::QueryServer;
use session::SessionManager;
use token_cache::TokenCache;

#[tokio::main]
async fn main() -> Result<()> {
    // Initialize tracing
    tracing_subscriber::registry()
        .with(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "udp_director=info".into()),
        )
        .with(tracing_subscriber::fmt::layer())
        .init();

    info!("Starting UDP Director");

    // Load initial configuration
    let config = Config::load().await?;
    info!("Configuration loaded successfully");

    // Initialize Kubernetes client
    let k8s_client = K8sClient::new().await?;
    info!("Kubernetes client initialized");

    // Initialize shared state
    let token_cache = TokenCache::new(config.token_ttl_seconds);
    let session_manager = SessionManager::new(config.session_timeout_seconds);

    // Start config watcher
    let config_handle = {
        let config_clone = config.clone();
        tokio::spawn(async move {
            if let Err(e) = config_clone.watch_for_changes().await {
                warn!("Config watcher error: {}", e);
            }
        })
    };

    // Start Query Server (Phase 1)
    let query_handle = {
        let query_server = QueryServer::new(
            config.query_port,
            k8s_client.clone(),
            token_cache.clone(),
            config.clone(),
        );
        tokio::spawn(async move {
            if let Err(e) = query_server.run().await {
                warn!("Query server error: {}", e);
            }
        })
    };

    // Start Data Proxy (Phase 2 & 3)
    let proxy_handle = {
        let data_proxy = DataProxy::new(
            config.data_port,
            token_cache.clone(),
            session_manager.clone(),
            config.clone(),
            k8s_client.clone(),
        );
        tokio::spawn(async move {
            if let Err(e) = data_proxy.run().await {
                warn!("Data proxy error: {}", e);
            }
        })
    };

    info!("UDP Director is running");
    info!("Query port: {}", config.query_port);
    info!("Data port: {}", config.data_port);

    // Wait for all tasks
    tokio::select! {
        _ = config_handle => warn!("Config watcher terminated"),
        _ = query_handle => warn!("Query server terminated"),
        _ = proxy_handle => warn!("Data proxy terminated"),
    }

    Ok(())
}
