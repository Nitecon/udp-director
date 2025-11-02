use anyhow::Result;
use tracing::{info, warn};
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt};

mod config;
mod k8s_client;
mod proxy;
mod query_server;
mod resource_monitor;
mod session;
mod token_cache;

use config::Config;
use k8s_client::K8sClient;
use proxy::DataProxy;
use query_server::QueryServer;
use resource_monitor::ResourceMonitor;
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

    // Verify default endpoint configuration
    verify_default_endpoint(&config, &k8s_client).await;

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

    // Start Resource Monitor
    let monitor_handle = {
        let resource_monitor = ResourceMonitor::new(
            config.clone(),
            k8s_client.clone(),
            session_manager.clone(),
            10, // Check every 10 seconds
        );
        tokio::spawn(async move {
            if let Err(e) = resource_monitor.run().await {
                warn!("Resource monitor error: {}", e);
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
        _ = monitor_handle => warn!("Resource monitor terminated"),
    }

    Ok(())
}

/// Verify and display the default endpoint configuration
async fn verify_default_endpoint(config: &Config, k8s_client: &K8sClient) {
    use tracing::error;

    let default_endpoint = config.get_default_endpoint();

    info!("=== Default Endpoint Configuration ===");
    info!("  Resource Type: {}", default_endpoint.resource_type);
    info!("  Namespace: {}", default_endpoint.namespace);

    if let Some(labels) = &default_endpoint.label_selector {
        info!("  Label Selector:");
        for (key, value) in labels {
            info!("    {} = {}", key, value);
        }
    }

    if let Some(status_query) = &default_endpoint.status_query {
        info!("  Status Query:");
        info!("    JSONPath: {}", status_query.json_path);
        info!("    Expected Values: {:?}", status_query.expected_values);
    }

    // Try to resolve the default endpoint
    match config
        .resource_query_mapping
        .get(&default_endpoint.resource_type)
    {
        Some(mapping) => {
            info!("  Resource Mapping Found:");
            info!("    Group: {}", mapping.group);
            info!("    Resource: {}", mapping.resource);

            // Convert status query
            let status_query =
                default_endpoint
                    .status_query
                    .as_ref()
                    .map(|sq| k8s_client::StatusQuery {
                        json_path: sq.json_path.clone(),
                        expected_values: sq.expected_values.clone(),
                    });

            // Query for matching resources
            match k8s_client
                .query_resources(
                    &default_endpoint.namespace,
                    mapping,
                    status_query.as_ref(),
                    default_endpoint.label_selector.as_ref(),
                )
                .await
            {
                Ok(resources) => {
                    if resources.is_empty() {
                        warn!("  ⚠️  No matching resources found for default endpoint!");
                        warn!("  Clients without tokens will fail to connect.");
                    } else {
                        info!("  ✓ Found {} matching resource(s)", resources.len());

                        // Display the first matching resource
                        if let Some(resource) = resources.first() {
                            let resource_name =
                                resource.metadata.name.as_deref().unwrap_or("unknown");

                            info!("  Selected Resource: {}", resource_name);

                            // Try to extract address and port
                            if let Some(address_path) = &mapping.address_path {
                                match k8s_client.extract_address(resource, address_path) {
                                    Ok(address) => {
                                        match k8s_client.extract_port(
                                            resource,
                                            mapping.port_path.as_deref(),
                                            mapping.port_name.as_deref(),
                                        ) {
                                            Ok(port) => {
                                                info!("  Default Target: {}:{}", address, port);
                                            }
                                            Err(e) => {
                                                error!("  ✗ Failed to extract port: {}", e);
                                            }
                                        }
                                    }
                                    Err(e) => {
                                        error!("  ✗ Failed to extract address: {}", e);
                                    }
                                }
                            }
                        }
                    }
                }
                Err(e) => {
                    error!("  ✗ Failed to query default endpoint resources: {}", e);
                    error!("  Namespace: {}", default_endpoint.namespace);
                    error!(
                        "  Resource: {}/{}/{}",
                        mapping.group, mapping.version, mapping.resource
                    );
                    error!("  This may be a permissions issue. Check RBAC configuration.");
                    error!("  Clients without tokens will fail to connect.");
                }
            }
        }
        None => {
            error!(
                "  ✗ Resource type '{}' not found in resourceQueryMapping!",
                default_endpoint.resource_type
            );
            error!("  Default endpoint is misconfigured!");
        }
    }

    info!("======================================");
}
