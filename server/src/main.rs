use std::net::SocketAddr;

use axum::{
    Router,
    extract::{ConnectInfo, Request},
    http::header,
    middleware::{self, Next},
    response::{Html, IntoResponse, Response},
    routing::get,
};
use bronze_magpie::{
    server_asset,
    tls::{self, TlsConnectionInfo},
};
use tracing::Level;
use tracing_appender::rolling::{RollingFileAppender, Rotation};
// https://docs.rs/axum/latest/axum/response/index.html
async fn access_log(req: Request, next: Next) -> Response {
    let ip_port = req
        .extensions()
        .get::<ConnectInfo<TlsConnectionInfo>>()
        .map(|info| format!("{info:?}"))
        .unwrap_or("unknown".to_string());

    let user_agent = req
        .headers()
        .get(header::USER_AGENT)
        .and_then(|val| val.to_str().ok())
        .unwrap_or("UNKNOWN")
        .to_string();

    let uri = req.uri().clone();

    let response = next.run(req).await;

    tracing::info!(
        client_addr = %ip_port,
        user_agent = %user_agent,
        uri = %uri
    );

    response
}

#[tokio::main]
async fn main() {
    let file_appender = RollingFileAppender::builder()
        .rotation(Rotation::DAILY)
        .filename_prefix("bronzemagpie")
        .max_log_files(7)
        .build("logs")
        .unwrap();
    let (non_blocking, _guard) = tracing_appender::non_blocking(file_appender);
    tracing_subscriber::fmt()
        .with_writer(non_blocking)
        .with_max_level(Level::DEBUG)
        .with_ansi(false)
        .init();

    // build our application with a single route
    let app = Router::new()
        .route("/", get(home))
        .route("/projects/", get(projects))
        .route("/{*filepath}", get(server_asset))
        .layer(middleware::from_fn(access_log));

    // run our app with hyper, listening globally on port 3000
    let tls_listener = tls::TlsListener::new().await;

    // let listener = tokio::net::TcpListener::bind("0.0.0.0:3000").await.unwrap();
    //tokio_rustls::TlsAcceptor::
    axum::serve(
        tls_listener,
        app.into_make_service_with_connect_info::<TlsConnectionInfo>(),
    )
    .await
    .unwrap();
}

/// Html wrapper denotes the auto-wrapped content type stuff
async fn home() -> Html<&'static str> {
    let index_bytes = website::asset("index.html").unwrap();
    let index_string = str::from_utf8(index_bytes).unwrap();
    Html(index_string)
}

/// Html wrapper denotes the auto-wrapped content type stuff
async fn projects() -> Html<&'static str> {
    let index_bytes = website::asset("projects/index.html").unwrap();
    let index_string = str::from_utf8(index_bytes).unwrap();
    //let index_string = String::from_utf8(index_bytes).unwrap();
    Html(index_string)
}
