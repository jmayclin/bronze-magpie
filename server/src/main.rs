use axum::{Router, response::{Html, IntoResponse}, routing::get};
use bronze_magpie::server_asset;
// https://docs.rs/axum/latest/axum/response/index.html

#[tokio::main]
async fn main() {
    // build our application with a single route
    let app = Router::new()
    .route("/",get(home))
    .route("/projects/",get(projects))
    .route("/{*filepath}", get(server_asset));

    // run our app with hyper, listening globally on port 3000
    let listener = tokio::net::TcpListener::bind("0.0.0.0:3000").await.unwrap();
    axum::serve(listener, app).await.unwrap();
}

/// Html wrapper denotes the auto-wrapped content type stuff
async fn home() -> Html<String> {
    let index_bytes = website::asset("index.html").unwrap();
    let index_string = String::from_utf8(index_bytes).unwrap();
    Html(index_string)
}

/// Html wrapper denotes the auto-wrapped content type stuff
async fn projects() -> Html<String> {
    let index_bytes = website::asset("projects/index.html").unwrap();
    let index_string = String::from_utf8(index_bytes).unwrap();
    Html(index_string)
}