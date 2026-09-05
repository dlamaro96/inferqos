//! Deterministic finite-capacity OpenAI-compatible provider for development and tests.
#![forbid(unsafe_code)]
use axum::{
    Router,
    body::Body,
    extract::{Request, State},
    http::{StatusCode, header},
    response::{IntoResponse, Response},
    routing::{any, get},
};
use bytes::Bytes;
use clap::Parser;
use futures_util::stream;
use std::{
    net::SocketAddr,
    sync::{
        Arc,
        atomic::{AtomicU64, Ordering},
    },
    time::Duration,
};

#[derive(Parser)]
struct Args {
    #[arg(long, default_value = "127.0.0.1:18080")]
    listen: SocketAddr,
    #[arg(long, default_value_t = 8)]
    concurrency: usize,
    #[arg(long, default_value_t = 50)]
    latency_ms: u64,
    #[arg(long, default_value_t = 12)]
    chunks: u64,
}
#[derive(Clone)]
struct App {
    semaphore: Arc<tokio::sync::Semaphore>,
    latency: Duration,
    chunks: u64,
    requests: Arc<AtomicU64>,
    throttles: Arc<AtomicU64>,
}
#[tokio::main]
async fn main() {
    let a = Args::parse();
    let app = App {
        semaphore: Arc::new(tokio::sync::Semaphore::new(a.concurrency)),
        latency: Duration::from_millis(a.latency_ms),
        chunks: a.chunks,
        requests: Arc::new(AtomicU64::new(0)),
        throttles: Arc::new(AtomicU64::new(0)),
    };
    println!(
        "fake finite provider listening on http://{} with concurrency {}",
        a.listen, a.concurrency
    );
    let listener = tokio::net::TcpListener::bind(a.listen)
        .await
        .expect("listen address must be available");
    axum::serve(
        listener,
        Router::new()
            .route("/health", get(|| async { "ok" }))
            .route("/metrics", get(metrics))
            .fallback(any(infer))
            .with_state(app),
    )
    .await
    .expect("fake provider server failed")
}
async fn infer(State(app): State<App>, request: Request) -> Response {
    app.requests.fetch_add(1, Ordering::Relaxed);
    let Ok(permit) = app.semaphore.clone().try_acquire_owned() else {
        app.throttles.fetch_add(1, Ordering::Relaxed);
        return (StatusCode::TOO_MANY_REQUESTS,[(header::RETRY_AFTER,"1")],axum::Json(serde_json::json!({"error":{"type":"capacity_exhausted","message":"deterministic fake provider is saturated"}}))).into_response();
    };
    let body = match axum::body::to_bytes(request.into_body(), 1024 * 1024).await {
        Ok(v) => v,
        Err(_) => return StatusCode::PAYLOAD_TOO_LARGE.into_response(),
    };
    let streaming = serde_json::from_slice::<serde_json::Value>(&body)
        .ok()
        .and_then(|v| v.get("stream").and_then(|v| v.as_bool()))
        .unwrap_or(false);
    if !streaming {
        tokio::time::sleep(app.latency).await;
        drop(permit);
        return axum::Json(serde_json::json!({"id":"fake-response","object":"chat.completion","choices":[{"message":{"role":"assistant","content":"finite capacity response"}}],"usage":{"prompt_tokens":16,"completion_tokens":8,"total_tokens":24}})).into_response();
    }
    let delay = app.latency;
    let chunks = app.chunks;
    let output = stream::unfold((0, Some(permit)), move |(i, permit)| async move {
        if i >= chunks {
            return None;
        }
        tokio::time::sleep(delay).await;
        let done = i + 1 == chunks;
        let text = if done {
            "data: [DONE]\n\n".to_string()
        } else {
            format!(
                "data: {{\"id\":\"fake\",\"choices\":[{{\"delta\":{{\"content\":\"chunk-{i} \"}}}}]}}\n\n"
            )
        };
        Some((
            Ok::<Bytes, std::io::Error>(Bytes::from(text)),
            (i + 1, permit),
        ))
    });
    let mut response = Response::new(Body::from_stream(output));
    response.headers_mut().insert(
        header::CONTENT_TYPE,
        header::HeaderValue::from_static("text/event-stream"),
    );
    response
}
async fn metrics(State(app): State<App>) -> String {
    format!(
        "fake_provider_requests_total {}\nfake_provider_throttles_total {}\n",
        app.requests.load(Ordering::Relaxed),
        app.throttles.load(Ordering::Relaxed)
    )
}
