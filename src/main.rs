use axum::{
    extract::Json,
    http::StatusCode,
    response::IntoResponse,
    routing::post,
    Router,
};
use dotenv::dotenv;
use reqwest::Client;
use serde_json::{json, Value};
use std::{env, sync::Arc};
use tokio::sync::Semaphore;
use tracing::{error, debug};
use tracing_subscriber;

#[tokio::main]
async fn main() {
    dotenv().ok();

    let log_level = env::var("RUST_LOG").unwrap_or_else(|_| "info".to_string());
    tracing_subscriber::fmt()
        .with_env_filter(log_level)
        .init();

    let alertmanager_url = env::var("ALERTMANAGER_URL")
        .expect("ALERTMANAGER_URL environment variable is required");

    let http_client = Arc::new(
        Client::builder()
            .pool_max_idle_per_host(100)
            .timeout(std::time::Duration::from_secs(10))
            .build()
            .expect("Failed to create HTTP client"),
    );

    let semaphore = Arc::new(Semaphore::new(1000));

    let app = Router::new()
        .route(
            "/api/v2/alerts",
            post(move |payload| {
                proxy_alerts(
                    payload,
                    http_client.clone(),
                    alertmanager_url.clone(),
                    semaphore.clone(),
                )
            }),
        );

    let addr = ([0, 0, 0, 0], 8080).into();
    debug!("Starting server on {}", addr);

    axum::Server::bind(&addr)
        .http1_keepalive(true)
        .serve(app.into_make_service())
        .await
        .unwrap();
}

async fn proxy_alerts(
    Json(payload): Json<Value>,
    client: Arc<Client>,
    target_url: String,
    semaphore: Arc<Semaphore>,
) -> impl IntoResponse {
    let _permit = semaphore.acquire_owned().await.unwrap();

    let alerts = match payload.get("alerts") {
        Some(alerts) => alerts,
        None => {
            return (
                StatusCode::BAD_REQUEST,
                Json(json!({ "error": "Missing 'alerts' field" })),
            );
        }
    };

    debug!("Received alerts: {:?}", alerts);

    match client
        .post(&target_url)
        .json(&alerts)
        .send()
        .await
    {
        Ok(response) if response.status().is_success() => {
            debug!("Successfully forwarded alerts to {}", target_url);
            (StatusCode::OK, Json(json!({ "status": "forwarded" })))
        }
        Ok(response) => {
            let status = response.status();
            let body = response.text().await.unwrap_or_else(|_| "Unknown error".to_string());
            error!(
                "Failed to forward alerts: {} - {}",
                status,
                body
            );
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(json!({ "error": format!("Upstream returned {}: {}", status, body) })),
            )
        }
        Err(err) => {
            error!("Error sending request: {}", err);
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(json!({ "error": "Failed to forward alerts" })),
            )
        }
    }
}
