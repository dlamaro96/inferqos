// SPDX-License-Identifier: Apache-2.0

use axum::{
    Router,
    body::Body,
    http::{Response, StatusCode, header},
    response::Redirect,
    routing::get,
};
use std::net::SocketAddr;
use tokio::net::TcpListener;

const HOME: &[u8] = include_bytes!("../../../web/site/index.html");
const DEMO: &[u8] = include_bytes!("../../../web/site/demo/index.html");
const CSS: &[u8] = include_bytes!("../../../web/site/site.css");
const SITE_JS: &[u8] = include_bytes!("../../../web/site/site.js");
const DEMO_JS: &[u8] = include_bytes!("../../../web/site/demo/demo.js");
const HERO: &[u8] = include_bytes!("../../../web/site/assets/capacity-control.webp");
const ROBOTS: &[u8] = include_bytes!("../../../web/site/robots.txt");
const SITEMAP: &[u8] = include_bytes!("../../../web/site/sitemap.xml");

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let listen = std::env::var("INFERQOS_SITE_LISTEN")
        .unwrap_or_else(|_| "0.0.0.0:8080".to_owned())
        .parse::<SocketAddr>()?;
    let listener = TcpListener::bind(listen).await?;
    println!("InferQoS website listening on http://{listen}");
    axum::serve(listener, router())
        .with_graceful_shutdown(shutdown_signal())
        .await?;
    Ok(())
}

fn router() -> Router {
    Router::new()
        .route(
            "/",
            get(|| async { asset(HOME, "text/html; charset=utf-8", false) }),
        )
        .route("/demo", get(|| async { Redirect::permanent("/demo/") }))
        .route(
            "/demo/",
            get(|| async { asset(DEMO, "text/html; charset=utf-8", false) }),
        )
        .route(
            "/site.css",
            get(|| async { asset(CSS, "text/css; charset=utf-8", true) }),
        )
        .route(
            "/site.js",
            get(|| async { asset(SITE_JS, "text/javascript; charset=utf-8", true) }),
        )
        .route(
            "/demo/demo.js",
            get(|| async { asset(DEMO_JS, "text/javascript; charset=utf-8", true) }),
        )
        .route(
            "/assets/capacity-control.webp",
            get(|| async { asset(HERO, "image/webp", true) }),
        )
        .route(
            "/robots.txt",
            get(|| async { asset(ROBOTS, "text/plain; charset=utf-8", false) }),
        )
        .route(
            "/sitemap.xml",
            get(|| async { asset(SITEMAP, "application/xml; charset=utf-8", false) }),
        )
        .route(
            "/healthz",
            get(|| async { asset(b"ok\n", "text/plain; charset=utf-8", false) }),
        )
        .fallback(|| async { not_found() })
}

fn asset(content: &'static [u8], content_type: &'static str, immutable: bool) -> Response<Body> {
    let cache = if immutable {
        "public, max-age=3600"
    } else {
        "no-cache"
    };
    Response::builder()
        .status(StatusCode::OK)
        .header(header::CONTENT_TYPE, content_type)
        .header(header::CACHE_CONTROL, cache)
        .header("content-security-policy", "default-src 'self'; img-src 'self'; script-src 'self'; style-src 'self'; connect-src 'self'; base-uri 'none'; form-action 'none'; frame-ancestors 'none'")
        .header("permissions-policy", "camera=(), microphone=(), geolocation=(), payment=(), usb=()")
        .header("referrer-policy", "no-referrer")
        .header("x-content-type-options", "nosniff")
        .header("x-frame-options", "DENY")
        .body(Body::from(content))
        .expect("static response headers are valid")
}

fn not_found() -> Response<Body> {
    let mut response = asset(b"Not found\n", "text/plain; charset=utf-8", false);
    *response.status_mut() = StatusCode::NOT_FOUND;
    response
}

async fn shutdown_signal() {
    let ctrl_c = async {
        tokio::signal::ctrl_c()
            .await
            .expect("failed to install Ctrl-C handler");
    };

    #[cfg(unix)]
    let terminate = async {
        tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate())
            .expect("failed to install SIGTERM handler")
            .recv()
            .await;
    };

    #[cfg(not(unix))]
    let terminate = std::future::pending::<()>();

    tokio::select! {
        () = ctrl_c => {},
        () = terminate => {},
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn responses_have_strict_browser_boundaries() {
        let response = asset(HOME, "text/html; charset=utf-8", false);
        assert_eq!(response.status(), StatusCode::OK);
        assert_eq!(response.headers()["x-frame-options"], "DENY");
        assert!(
            response.headers()["content-security-policy"]
                .to_str()
                .expect("CSP is text")
                .contains("frame-ancestors 'none'")
        );
        assert_eq!(response.headers()[header::CACHE_CONTROL], "no-cache");
    }

    #[test]
    fn missing_paths_do_not_fall_back_to_html() {
        assert_eq!(not_found().status(), StatusCode::NOT_FOUND);
    }
}
