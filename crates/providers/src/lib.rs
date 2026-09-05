//! Built-in provider adapters. Provider-specific authentication and URLs stop at this boundary.
#![forbid(unsafe_code)]
use async_trait::async_trait;
use futures_util::StreamExt;
use http::{HeaderName, HeaderValue, StatusCode};
use inferqos_config::{AuthConfig, PoolConfig, ProviderKind};
use inferqos_core::{
    CoreError, EstimateSource, HEADER_APPLICATION, HEADER_CLASS, HEADER_DEADLINE_MS,
    HEADER_QUEUEABLE, HEADER_TENANT, ProviderAdapter, ProviderResponse, ProxyRequest,
    UpstreamHealth, WorkEstimate, WorkUnits,
};
use reqwest::Client;
use std::{collections::HashSet, time::Duration};
use tokio::sync::mpsc;
use url::Url;

pub struct HttpProvider {
    name: &'static str,
    endpoint: Url,
    client: Client,
    auth: ResolvedAuth,
    allowed_hosts: HashSet<String>,
    coefficient: f64,
}
enum ResolvedAuth {
    None,
    Header(HeaderName, HeaderValue),
    Bearer(HeaderValue),
}

impl HttpProvider {
    pub fn from_config(config: &PoolConfig) -> Result<Self, CoreError> {
        let endpoint = Url::parse(&config.endpoint)
            .map_err(|e| CoreError::Provider(format!("invalid upstream endpoint: {e}")))?;
        if endpoint.scheme() != "https"
            && endpoint.host_str() != Some("127.0.0.1")
            && endpoint.host_str() != Some("localhost")
        {
            return Err(CoreError::Provider(
                "upstream must use HTTPS except for loopback development endpoints".into(),
            ));
        }
        if !config.allowed_hosts.is_empty()
            && !endpoint
                .host_str()
                .is_some_and(|h| config.allowed_hosts.contains(h))
        {
            return Err(CoreError::Provider(format!(
                "upstream host {} is not in allowed_hosts",
                endpoint.host_str().unwrap_or("<none>")
            )));
        }
        let (name, default_ambient) = match config.provider {
            ProviderKind::AzureOpenai => {
                ("azure-openai", Some(("api-key", "AZURE_OPENAI_API_KEY")))
            }
            ProviderKind::AwsBedrock => (
                "aws-bedrock",
                Some(("authorization", "AWS_BEARER_TOKEN_BEDROCK")),
            ),
            ProviderKind::GcpVertex => (
                "gcp-vertex",
                Some(("authorization", "GOOGLE_OAUTH_ACCESS_TOKEN")),
            ),
            ProviderKind::OpenaiCompatible => ("openai-compatible", None),
            ProviderKind::Fake => ("fake", None),
        };
        let auth = resolve_auth(&config.auth, default_ambient)?;
        let client = Client::builder()
            .http2_adaptive_window(true)
            .pool_idle_timeout(Duration::from_secs(90))
            .connect_timeout(Duration::from_secs(5))
            .build()
            .map_err(|e| CoreError::Provider(e.to_string()))?;
        Ok(Self {
            name,
            endpoint,
            client,
            auth,
            allowed_hosts: config.allowed_hosts.iter().cloned().collect(),
            coefficient: 1.0,
        })
    }
    fn target(&self, path: &str) -> Result<Url, CoreError> {
        let relative = path.trim_start_matches('/');
        let url = self
            .endpoint
            .join(relative)
            .map_err(|e| CoreError::Provider(format!("cannot join upstream URL: {e}")))?;
        if url.host_str() != self.endpoint.host_str() {
            return Err(CoreError::Provider(
                "dynamic upstream host changes are forbidden".into(),
            ));
        }
        if !self.allowed_hosts.is_empty()
            && !url
                .host_str()
                .is_some_and(|h| self.allowed_hosts.contains(h))
        {
            return Err(CoreError::Provider(
                "resolved upstream is outside allowed_hosts".into(),
            ));
        }
        Ok(url)
    }
}

fn resolve_auth(
    auth: &AuthConfig,
    ambient: Option<(&str, &str)>,
) -> Result<ResolvedAuth, CoreError> {
    let from_header = |header: &str, env: &str, bearer: bool| -> Result<ResolvedAuth, CoreError> {
        let value = std::env::var(env).map_err(|_| {
            CoreError::Provider(format!(
                "authentication environment variable {env} is not set"
            ))
        })?;
        if bearer {
            let encoded = HeaderValue::from_str(&format!("Bearer {value}"))
                .map_err(|_| CoreError::Provider(format!("{env} contains invalid header bytes")))?;
            Ok(ResolvedAuth::Bearer(encoded))
        } else {
            Ok(ResolvedAuth::Header(
                HeaderName::from_bytes(header.as_bytes())
                    .map_err(|_| CoreError::Provider("invalid auth header name".into()))?,
                HeaderValue::from_str(&value).map_err(|_| {
                    CoreError::Provider(format!("{env} contains invalid header bytes"))
                })?,
            ))
        }
    };
    match auth {
        AuthConfig::None => Ok(ResolvedAuth::None),
        AuthConfig::ApiKey { env, header } => from_header(header, env, false),
        AuthConfig::Bearer { env } => from_header("authorization", env, true),
        AuthConfig::Ambient => {
            let(h,e)=ambient.ok_or_else(||CoreError::Provider("ambient auth is not defined for this provider; configure an API key or bearer token environment variable".into()))?;
            from_header(h, e, h == "authorization")
        }
    }
}

#[async_trait]
impl ProviderAdapter for HttpProvider {
    fn name(&self) -> &'static str {
        self.name
    }
    async fn estimate(&self, request: &ProxyRequest) -> Result<WorkEstimate, CoreError> {
        let parsed: serde_json::Value =
            serde_json::from_slice(&request.body).unwrap_or(serde_json::Value::Null);
        let mut text_bytes = 0usize;
        fn count(v: &serde_json::Value, n: &mut usize) {
            match v {
                serde_json::Value::String(s) => *n += s.len(),
                serde_json::Value::Array(a) => a.iter().for_each(|x| count(x, n)),
                serde_json::Value::Object(o) => o.values().for_each(|x| count(x, n)),
                _ => {}
            }
        }
        if let Some(v) = parsed.get("input").or_else(|| parsed.get("messages")) {
            count(v, &mut text_bytes)
        } else {
            count(&parsed, &mut text_bytes)
        }
        let input = (text_bytes / 4).max(1) as u64;
        let output = parsed
            .get("max_output_tokens")
            .or_else(|| parsed.get("max_tokens"))
            .and_then(|v| v.as_u64())
            .unwrap_or(512);
        let units = (input + output) as f64 * self.coefficient;
        Ok(WorkEstimate {
            input_tokens: input,
            output_tokens: output,
            cached_input_tokens: 0,
            provider_cost_coefficient: self.coefficient,
            normalized_units: WorkUnits(units),
            confidence: 0.55,
            source: EstimateSource::Approximation,
        })
    }
    async fn dispatch(&self, request: ProxyRequest) -> Result<ProviderResponse, CoreError> {
        let target = self.target(request.uri.path_and_query().map_or("/", |p| p.as_str()))?;
        let mut builder = self.client.request(request.method, target);
        for (name, value) in &request.headers {
            if is_forwardable(name.as_str()) {
                builder = builder.header(name, value);
            }
        }
        builder = match &self.auth {
            ResolvedAuth::None => builder,
            ResolvedAuth::Header(k, v) => builder.header(k, v),
            ResolvedAuth::Bearer(v) => builder.header("authorization", v),
        };
        let response = builder
            .body(request.body)
            .send()
            .await
            .map_err(|e| CoreError::Provider(e.to_string()))?;
        let status = StatusCode::from_u16(response.status().as_u16())
            .map_err(|e| CoreError::Provider(e.to_string()))?;
        let headers = response.headers().clone();
        let (tx, rx) = mpsc::channel(16);
        tokio::spawn(async move {
            let mut stream = response.bytes_stream();
            while let Some(chunk) = stream.next().await {
                match chunk {
                    Ok(bytes) => {
                        if tx.send(Ok(bytes)).await.is_err() {
                            break;
                        }
                    }
                    Err(e) => {
                        let _ = tx.send(Err(CoreError::Provider(e.to_string()))).await;
                        break;
                    }
                }
            }
        });
        Ok(ProviderResponse {
            status,
            headers,
            body: rx,
            usage: None,
        })
    }
    async fn health(&self) -> Result<UpstreamHealth, CoreError> {
        let mut url = self.endpoint.clone();
        url.set_path("/");
        match self.client.head(url).send().await {
            Ok(r) => Ok(UpstreamHealth {
                healthy: r.status().as_u16() < 500,
                detail: format!("HTTP {}", r.status()),
            }),
            Err(e) => Ok(UpstreamHealth {
                healthy: false,
                detail: e.to_string(),
            }),
        }
    }
}
fn is_forwardable(name: &str) -> bool {
    !matches!(
        name,
        "connection"
            | "keep-alive"
            | "proxy-authenticate"
            | "proxy-authorization"
            | "te"
            | "trailer"
            | "transfer-encoding"
            | "upgrade"
            | "host"
            | "authorization"
            | "api-key"
            | HEADER_CLASS
            | HEADER_DEADLINE_MS
            | HEADER_TENANT
            | HEADER_APPLICATION
            | HEADER_QUEUEABLE
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn strips_qos_and_hop_headers() {
        assert!(!is_forwardable("connection"));
        assert!(!is_forwardable(HEADER_CLASS));
        assert!(is_forwardable("traceparent"));
    }
}
