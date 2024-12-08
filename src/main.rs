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
use std::{collections::HashMap, env, fs, sync::Arc};
use tokio::sync::Semaphore;
use tracing::{debug, error, info};
use tracing_subscriber;
use yaml_rust::YamlLoader;
use base64::{engine::general_purpose::STANDARD, Engine};

#[derive(Debug)]
struct AlertManager {
    url: String,
    client: Client,
}

#[tokio::main]
async fn main() {
    dotenv().ok();

    let log_level = env::var("RUST_LOG").unwrap_or_else(|_| "info".to_string());
    tracing_subscriber::fmt()
        .with_env_filter(log_level)
        .init();

    let default_alertmanager_url = env::var("ALERTMANAGER_URL").unwrap_or_else(|_| {
        panic!("ALERTMANAGER_URL environment variable is required for the default alertmanager")
    });

    let default_client = build_client(
        env::var("DEFAULT_USERNAME").ok(),
        env::var("DEFAULT_PASSWORD").ok(),
    );

    let enable_label_routing = env::var("ENABLE_LABEL_ROUTING")
        .map(|v| v.to_lowercase() == "true")
        .unwrap_or(false);

    let mut alertmanagers = HashMap::new();
    if enable_label_routing {
        let yaml_file_path = env::var("ALERTMANAGER_CONFIG").unwrap_or_else(|_| "alertmanagers.yaml".to_string());
        match load_alertmanagers(&yaml_file_path) {
            Ok(configured_alertmanagers) => alertmanagers.extend(configured_alertmanagers),
            Err(err) => {
                error!(
                    "Failed to load alertmanager configuration from {}: {}. Using only the default alertmanager.",
                    yaml_file_path, err
                );
            }
        }
    }

    // Add the default alertmanager, always available
    alertmanagers.insert(
        "default".to_string(),
        AlertManager {
            url: default_alertmanager_url,
            client: default_client,
        },
    );

    let semaphore = Arc::new(Semaphore::new(1000));

    let app = Router::new().route(
        "/api/v2/alerts",
        post({
            let alertmanagers = Arc::new(alertmanagers);
            let label_key = env::var("LABEL_KEY").unwrap_or_else(|_| "alertmanager".to_string());
            move |payload| {
                proxy_alerts(
                    payload,
                    alertmanagers.clone(),
                    semaphore.clone(),
                    label_key.clone(),
                    enable_label_routing,
                )
            }
        }),
    );

    let addr = ([0, 0, 0, 0], 8080).into();
    info!("Starting server on {}", addr);

    axum::Server::bind(&addr)
        .http1_keepalive(true)
        .serve(app.into_make_service())
        .await
        .unwrap();
}

fn build_client(username: Option<String>, password: Option<String>) -> Client {
    let mut builder = Client::builder().pool_max_idle_per_host(100).timeout(std::time::Duration::from_secs(10));

    if let (Some(user), Some(pass)) = (username, password) {
        let auth = format!("{}:{}", user, pass);
        let encoded_auth = STANDARD.encode(auth);
        builder = builder.default_headers({
            let mut headers = reqwest::header::HeaderMap::new();
            headers.insert(
                reqwest::header::AUTHORIZATION,
                reqwest::header::HeaderValue::from_str(&format!("Basic {}", encoded_auth))
                    .expect("Invalid basic auth header"),
            );
            headers
        });
    }

    builder.build().expect("Failed to create HTTP client")
}

fn load_alertmanagers(
    config_path: &str,
) -> Result<HashMap<String, AlertManager>, Box<dyn std::error::Error>> {
    let config = fs::read_to_string(config_path)?;
    let docs = YamlLoader::load_from_str(&config)?;
    let doc = &docs[0];

    let mut map = HashMap::new();

    for (key, value) in doc.as_hash().ok_or("Invalid YAML format")? {
        let key = key.as_str().ok_or("Invalid key format")?.to_string();
        let url = value["url"]
            .as_str()
            .ok_or("Missing 'url' field in config")?
            .to_string();

        let username = value["auth"]["username"].as_str().map(|s| env::var(s).unwrap_or_default());
        let password = value["auth"]["password"].as_str().map(|s| env::var(s).unwrap_or_default());

        let client = build_client(username, password);
        map.insert(key, AlertManager { url, client });
    }

    Ok(map)
}

async fn proxy_alerts(
    Json(payload): Json<Value>,
    alertmanagers: Arc<HashMap<String, AlertManager>>,
    semaphore: Arc<Semaphore>,
    label_key: String,
    enable_label_routing: bool,
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

    let key = if enable_label_routing {
        alerts
            .as_array()
            .and_then(|alerts| {
                alerts.iter().find_map(|alert| {
                    alert
                        .get("labels")
                        .and_then(|labels| labels.get(&label_key).and_then(|v| v.as_str()))
                })
            })
            .unwrap_or("default")
    } else {
        "default"
    };

    let alertmanager = alertmanagers.get(key).unwrap_or_else(|| {
        info!(
            "No specific alertmanager found for key '{}', using default",
            key
        );
        alertmanagers.get("default").unwrap()
    });

    info!("Using alertmanager at URL: {}", alertmanager.url);

    match alertmanager
        .client
        .post(&alertmanager.url)
        .json(&alerts)
        .send()
        .await
    {
        Ok(response) if response.status().is_success() => {
            debug!("Successfully forwarded alerts to {}", alertmanager.url);
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
