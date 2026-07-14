//! One application binary with serving, migration, and Dibs tooling modes.

use dibs::SquelServiceImpl;
use dibs_proto::SquelServiceDispatcher;
use std::{net::SocketAddr, str::FromStr, sync::Arc};
use tokio::net::TcpListener;
use tokio_postgres::NoTls;
use tokio_tungstenite::accept_async;
use tracing::{error, info};
use tracing_subscriber::{EnvFilter, layer::SubscriberExt as _, util::SubscriberInitExt as _};
use vox_websocket::WsLink;

#[derive(Debug, Clone, Copy, Default)]
enum Mode {
    #[default]
    Serve,
    Migrate,
    Dibs,
}

impl FromStr for Mode {
    type Err = String;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        match value {
            "serve" => Ok(Self::Serve),
            "migrate" => Ok(Self::Migrate),
            "dibs" => Ok(Self::Dibs),
            other => Err(format!(
                "unknown MY_APP_MODE {other:?}; expected serve, migrate, or dibs"
            )),
        }
    }
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let _ = dotenvy::dotenv();
    tracing_subscriber::registry()
        .with(tracing_subscriber::fmt::layer())
        .with(EnvFilter::from_default_env())
        .init();

    // This real symbol reference retains the schema and migration inventory.
    my_app_db::ensure_linked();

    let mode = match std::env::var("MY_APP_MODE") {
        Ok(value) => value.parse()?,
        Err(std::env::VarError::NotPresent) => Mode::default(),
        Err(error) => return Err(error.into()),
    };

    match mode {
        Mode::Serve => serve().await,
        Mode::Migrate => migrate().await,
        Mode::Dibs => serve_dibs().await,
    }
}

async fn migrate() -> Result<(), Box<dyn std::error::Error>> {
    let database_url = database_url()?;
    let (mut client, connection) = tokio_postgres::connect(&database_url, NoTls).await?;
    tokio::spawn(async move {
        if let Err(error) = connection.await {
            error!(%error, "database connection failed");
        }
    });

    let applied = dibs::MigrationRunner::new(&mut client).migrate().await?;
    info!(count = applied.len(), "database migrations complete");
    Ok(())
}

async fn serve_dibs() -> Result<(), Box<dyn std::error::Error>> {
    let addr: SocketAddr = std::env::var("DIBS_LISTEN_ADDR")
        .unwrap_or_else(|_| "127.0.0.1:7764".to_string())
        .parse()?;
    dibs::serve(addr).await?;
    Ok(())
}

async fn serve() -> Result<(), Box<dyn std::error::Error>> {
    let database_url = database_url()?;
    let (client, connection) = tokio_postgres::connect(&database_url, NoTls).await?;
    let client = Arc::new(client);

    tokio::spawn(async move {
        if let Err(error) = connection.await {
            error!(%error, "database connection failed");
        }
    });

    let addr: SocketAddr = std::env::var("LISTEN_ADDR")
        .unwrap_or_else(|_| "127.0.0.1:9000".to_string())
        .parse()?;
    let listener = TcpListener::bind(addr).await?;
    info!(%addr, "SquelService listening");

    loop {
        let (stream, peer_addr) = listener.accept().await?;
        let client = client.clone();
        tokio::spawn(async move {
            let result = async {
                let ws_stream = accept_async(stream).await?;
                let link = WsLink::new(ws_stream);
                let dispatcher = SquelServiceDispatcher::new(SquelServiceImpl::new(client));
                let connection = vox::acceptor_on(link)
                    .on_lane(dispatcher)
                    .establish_connection()
                    .await
                    .map_err(std::io::Error::other)?;
                connection.closed().await;
                Ok::<_, Box<dyn std::error::Error + Send + Sync>>(())
            }
            .await;

            if let Err(error) = result {
                error!(%peer_addr, %error, "SquelService session failed");
            }
        });
    }
}

fn database_url() -> Result<String, std::env::VarError> {
    std::env::var("DATABASE_URL")
}
