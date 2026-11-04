//! `net.minecraft.server.jsonrpc`: the management server (JSON-RPC 2.0 over a websocket, bearer-secret
//! authentication, optional TLS). The server state it needs is behind [`api::ManagementApi`].

pub mod api;
pub mod dto;
pub mod methods;
pub mod notifications;
pub mod security;
pub mod server;

use std::sync::Arc;

pub use api::{ClientInfo, ManagementApi};
pub use server::ManagementServer;

use crate::server::dedicated::properties::DedicatedServerProperties;
use crate::server::notifications::{NotificationManager, NotificationService};

/// `HostAndPort.toString()`: IPv6 literals are bracketed.
fn host_and_port(host: &str, port: i32) -> String {
    if host.contains(':') && !host.starts_with('[') { format!("[{host}]:{port}") } else { format!("{host}:{port}") }
}

/// `JsonRpc.create`: `Ok(None)` when `management-server-enabled` is false. `Err` is the Java exception text of the
/// failures vanilla throws (invalid secret, TLS misconfiguration) or of the bind failure.
pub fn create(
    properties: &DedicatedServerProperties,
    notification_manager: &Arc<NotificationManager>,
    api: Arc<dyn ManagementApi>,
) -> Result<Option<ManagementServer>, String> {
    if !properties.management_server_enabled {
        return Ok(None);
    }
    let secret = &properties.management_server_secret;
    if !security::is_valid_secret(secret) {
        return Err("java.lang.IllegalStateException: Invalid management server secret, must be 40 alphanumeric characters".to_owned());
    }
    let (host, port) = (&properties.management_server_host, properties.management_server_port);
    let auth = security::AuthenticationHandler::new(secret.clone(), &properties.management_server_allowed_origins);
    log::info!("Starting json RPC server on {}", host_and_port(host, port));
    let mut server = ManagementServer::new(host, port, auth).map_err(|e| format!("java.io.IOException: {e}"))?;
    let service: Arc<dyn NotificationService> = Arc::new(notifications::JsonRpcNotificationService::new(api.clone(), server.connections()));
    notification_manager.register_service(service);
    let tls = if properties.management_server_tls_enabled {
        match security::create_tls_acceptor(&properties.management_server_tls_keystore, &properties.management_server_tls_keystore_password) {
            Ok(acceptor) => Some(acceptor),
            Err(cause) => {
                security::print_tls_instructions();
                return Err(format!("java.lang.IllegalStateException: Failed to configure TLS for the server management protocol\nCaused by: {cause}"));
            }
        }
    } else {
        None
    };
    server.start(api, tls).map_err(|e| format!("java.net.BindException: {e}"))?;
    // the heartbeat notifies through the manager so every registered service sees it, like vanilla
    server.schedule_heartbeat(notification_manager.clone(), *properties.status_heartbeat_interval.get() as i64);
    Ok(Some(server))
}
