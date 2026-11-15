//! The web service: logging, settings, the database, the router, and a
//! graceful shutdown.
use std::env;
use std::error::Error;

use shoes::products::ProductService;
use shoes::reviews::ReviewsClient;
use shoes::settings::Settings;
use sqlx::postgres::PgPoolOptions;
use tokio::net::TcpListener;
use tokio::signal;
use tracing::info;
use tracing_subscriber::EnvFilter;

/// Sends log events to stderr. `RUST_LOG` picks what to log (default: info
/// from this crate and from tower-http), and `LOG_FORMAT=json` switches from
/// readable lines to one JSON object per line, for log collectors.
fn init_tracing() {
    let filter = EnvFilter::try_from_default_env()
        .unwrap_or_else(|_| EnvFilter::new("shoes=info,tower_http=info"));
    let builder = tracing_subscriber::fmt()
        .with_env_filter(filter)
        .with_writer(std::io::stderr);
    if env::var("LOG_FORMAT").as_deref() == Ok("json") {
        builder.json().init();
    } else {
        builder.init();
    }
}

/// Completes on Ctrl+C (SIGINT) or SIGTERM, which is what `podman stop`
/// and Kubernetes send.
async fn shutdown_signal() {
    let ctrl_c = async {
        signal::ctrl_c().await.expect("can listen for Ctrl+C");
    };
    let terminate = async {
        signal::unix::signal(signal::unix::SignalKind::terminate())
            .expect("can listen for SIGTERM")
            .recv()
            .await;
    };
    // Whichever finishes first; the other future is dropped
    tokio::select! {
        () = ctrl_c => {},
        () = terminate => {},
    }
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn Error>> {
    init_tracing();
    let settings = Settings::load()?;
    info!(
        profile = settings.profile,
        addr = %settings.server.addr,
        max_connections = settings.database.max_connections,
        "settings loaded"
    );

    let pool = PgPoolOptions::new()
        .max_connections(settings.database.max_connections)
        .connect(&settings.database.url)
        .await?;
    // Applies the files in migrations/ that this database hasn't run yet.
    // `migrate!` embeds them in the binary at compile time.
    sqlx::migrate!().run(&pool).await?;
    info!("database ready");

    let reviews = ReviewsClient::new(&settings.reviews.url, settings.reviews.timeout())?;
    let products = ProductService::new(pool.clone(), reviews);
    let listener = TcpListener::bind(settings.server.addr).await?;
    info!("listening on http://{}", listener.local_addr()?);
    // Runs until shutdown_signal completes, then finishes the requests in flight
    axum::serve(listener, shoes::app(products))
        .with_graceful_shutdown(shutdown_signal())
        .await?;
    pool.close().await; // waits for connections to be returned, then closes them
    info!("stopped");
    Ok(())
}
