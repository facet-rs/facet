//! HTTP server for serving benchmark reports.

use crate::hyperlink;
use axum::{Router, routing::get_service};
use std::net::{IpAddr, SocketAddr};
use std::path::Path;
use tower_http::services::ServeDir;

/// Check if an IP address is a private/LAN address
fn is_lan_address(ip: &IpAddr) -> bool {
    match ip {
        IpAddr::V4(ipv4) => {
            let octets = ipv4.octets();
            // 10.0.0.0/8
            octets[0] == 10
            // 172.16.0.0/12
            || (octets[0] == 172 && (16..=31).contains(&octets[1]))
            // 192.168.0.0/16
            || (octets[0] == 192 && octets[1] == 168)
            // 127.0.0.0/8 (localhost)
            || octets[0] == 127
        }
        IpAddr::V6(ipv6) => {
            // Link-local (fe80::/10)
            ipv6.segments()[0] & 0xffc0 == 0xfe80
            // Unique local (fc00::/7)
            || ipv6.segments()[0] & 0xfe00 == 0xfc00
            // Loopback (::1)
            || ipv6.is_loopback()
        }
    }
}

/// Get all LAN IP addresses from network interfaces
fn get_lan_addresses() -> Vec<IpAddr> {
    if_addrs::get_if_addrs()
        .unwrap_or_default()
        .into_iter()
        .map(|iface| iface.addr.ip())
        .filter(is_lan_address)
        .collect()
}

/// Start a simple HTTP server to serve the report directory
pub async fn serve(report_dir: &Path, port: u16) -> std::io::Result<()> {
    let app = Router::new().fallback_service(get_service(ServeDir::new(report_dir)));

    let addr = SocketAddr::from(([0, 0, 0, 0], port));

    println!("🌐 Starting HTTP server on port {}", port);
    println!();
    println!("   Available at:");

    let lan_addrs = get_lan_addresses();
    for ip in &lan_addrs {
        let bracket = if ip.is_ipv6() { ("[", "]") } else { ("", "") };
        let url = format!(
            "http://{}{}{}:{}/report.html",
            bracket.0, ip, bracket.1, port
        );
        println!("     {}", hyperlink(&url));
    }

    if lan_addrs.is_empty() {
        let url = format!("http://localhost:{}/report.html", port);
        println!("     {}", hyperlink(&url));
    }

    println!();
    println!("   Press Ctrl+C to stop");
    println!();

    let listener = tokio::net::TcpListener::bind(addr).await?;
    axum::serve(listener, app)
        .with_graceful_shutdown(shutdown_signal())
        .await
}

async fn shutdown_signal() {
    tokio::signal::ctrl_c()
        .await
        .expect("Failed to install Ctrl+C handler");
    println!("\nShutting down server...");
}
