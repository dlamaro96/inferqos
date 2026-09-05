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
    #[cfg(feature = "cloud-auth")]
    Azure(std::sync::Arc<dyn azure_core::credentials::TokenCredential>),
    #[cfg(feature = "cloud-auth")]
    Aws(AwsAuth),
    #[cfg(feature = "cloud-auth")]
    Google(google_cloud_auth::credentials::Credentials),
}

#[cfg(feature = "cloud-auth")]
struct AwsAuth {
    credentials: aws_credential_types::provider::SharedCredentialsProvider,
    region: String,
}

impl HttpProvider {
    pub async fn from_config(config: &PoolConfig) -> Result<Self, CoreError> {
        if matches!(config.provider, ProviderKind::ExternalGrpc) {
            return Err(CoreError::Provider(
                "external-grpc pools must be constructed by the provider protocol runtime".into(),
            ));
        }
        let endpoint = Url::parse(&config.endpoint)
            .map_err(|e| CoreError::Provider(format!("invalid upstream endpoint: {e}")))?;
        if endpoint.scheme() != "https"
            && !matches!(&config.provider, ProviderKind::Fake)
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
        let name = match config.provider {
            ProviderKind::AzureOpenai => "azure-openai",
            ProviderKind::AwsBedrock => "aws-bedrock",
            ProviderKind::GcpVertex => "gcp-vertex",
            ProviderKind::OpenaiCompatible => "openai-compatible",
            ProviderKind::Fake => "fake",
            ProviderKind::ExternalGrpc => unreachable!("checked above"),
        };
        let auth = resolve_auth(&config.auth, &config.provider, config.region.as_deref()).await?;
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

async fn resolve_auth(
    auth: &AuthConfig,
    provider: &ProviderKind,
    region: Option<&str>,
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
        AuthConfig::Ambient => ambient_auth(provider, region).await,
    }
}

#[cfg(feature = "cloud-auth")]
async fn ambient_auth(
    provider: &ProviderKind,
    region: Option<&str>,
) -> Result<ResolvedAuth, CoreError> {
    match provider {
        ProviderKind::AzureOpenai => {
            let credential: std::sync::Arc<dyn azure_core::credentials::TokenCredential> =
                if std::env::var_os("AZURE_FEDERATED_TOKEN_FILE").is_some() {
                    azure_identity::WorkloadIdentityCredential::new(None).map_err(|error| {
                        CoreError::Provider(format!(
                            "Azure workload identity is unavailable: {error}"
                        ))
                    })?
                } else {
                    azure_identity::ManagedIdentityCredential::new(None).map_err(|error| {
                        CoreError::Provider(format!(
                            "Azure managed identity is unavailable: {error}"
                        ))
                    })?
                };
            Ok(ResolvedAuth::Azure(credential))
        }
        ProviderKind::AwsBedrock => {
            let sdk = aws_config::defaults(aws_config::BehaviorVersion::latest())
                .load()
                .await;
            let credentials = sdk.credentials_provider().ok_or_else(|| {
                CoreError::Provider("AWS default credential chain produced no provider".into())
            })?;
            let region = region
                .map(str::to_owned)
                .or_else(|| sdk.region().map(|value| value.as_ref().to_owned()))
                .ok_or_else(|| {
                    CoreError::Provider(
                        "AWS Bedrock requires pool.region or an AWS default region".into(),
                    )
                })?;
            Ok(ResolvedAuth::Aws(AwsAuth {
                credentials,
                region,
            }))
        }
        ProviderKind::GcpVertex => {
            let credentials = google_cloud_auth::credentials::Builder::default()
                .with_scopes(["https://www.googleapis.com/auth/cloud-platform"])
                .build()
                .map_err(|error| {
                    CoreError::Provider(format!("Google ADC is unavailable: {error}"))
                })?;
            Ok(ResolvedAuth::Google(credentials))
        }
        _ => Err(CoreError::Provider(
            "ambient auth is only available for Azure OpenAI, AWS Bedrock, and GCP Vertex".into(),
        )),
    }
}

#[cfg(not(feature = "cloud-auth"))]
async fn ambient_auth(
    _provider: &ProviderKind,
    _region: Option<&str>,
) -> Result<ResolvedAuth, CoreError> {
    Err(CoreError::Provider("this binary was built without the cloud-auth feature; install an official release or rebuild with --features cloud-auth".into()))
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
        let mut outbound = builder
            .body(request.body)
            .build()
            .map_err(|error| CoreError::Provider(error.to_string()))?;
        apply_auth(&self.auth, &mut outbound).await?;
        let response = self
            .client
            .execute(outbound)
            .await
            .map_err(|e| CoreError::Provider(e.to_string()))?;
        let status = StatusCode::from_u16(response.status().as_u16())
            .map_err(|e| CoreError::Provider(e.to_string()))?;
        let headers = response.headers().clone();
        let (tx, rx) = mpsc::channel(16);
        let (usage_tx, usage_rx) = tokio::sync::watch::channel(None);
        let coefficient = self.coefficient;
        tokio::spawn(async move {
            let mut stream = response.bytes_stream();
            let mut metadata = Vec::new();
            while let Some(chunk) = stream.next().await {
                match chunk {
                    Ok(bytes) => {
                        if metadata.len().saturating_add(bytes.len()) <= 1024 * 1024 {
                            metadata.extend_from_slice(&bytes);
                        }
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
            let _ = usage_tx.send(parse_usage(&metadata, coefficient));
        });
        Ok(ProviderResponse {
            status,
            headers,
            body: rx,
            usage: usage_rx,
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

async fn apply_auth(auth: &ResolvedAuth, request: &mut reqwest::Request) -> Result<(), CoreError> {
    match auth {
        ResolvedAuth::None => {}
        ResolvedAuth::Header(name, value) => {
            request.headers_mut().insert(name, value.clone());
        }
        ResolvedAuth::Bearer(value) => {
            request.headers_mut().insert("authorization", value.clone());
        }
        #[cfg(feature = "cloud-auth")]
        ResolvedAuth::Azure(credential) => {
            let token = credential
                .get_token(&["https://cognitiveservices.azure.com/.default"], None)
                .await
                .map_err(|error| {
                    CoreError::Provider(format!(
                        "Azure managed identity token acquisition failed: {error}"
                    ))
                })?;
            let value = HeaderValue::from_str(&format!("Bearer {}", token.token.secret()))
                .map_err(|_| {
                    CoreError::Provider("Azure token contains invalid header bytes".into())
                })?;
            request.headers_mut().insert("authorization", value);
        }
        #[cfg(feature = "cloud-auth")]
        ResolvedAuth::Google(credentials) => {
            let headers = credentials
                .headers(http::Extensions::new())
                .await
                .map_err(|error| {
                    CoreError::Provider(format!("Google ADC token acquisition failed: {error}"))
                })?;
            let google_cloud_auth::credentials::CacheableResource::New { data, .. } = headers
            else {
                return Err(CoreError::Provider(
                    "Google ADC unexpectedly returned no headers without an entity tag".into(),
                ));
            };
            for (name, value) in data.iter() {
                request.headers_mut().insert(name, value.clone());
            }
        }
        #[cfg(feature = "cloud-auth")]
        ResolvedAuth::Aws(aws) => sign_aws(request, aws).await?,
    }
    Ok(())
}

#[cfg(feature = "cloud-auth")]
async fn sign_aws(request: &mut reqwest::Request, aws: &AwsAuth) -> Result<(), CoreError> {
    use aws_credential_types::provider::ProvideCredentials;
    use aws_sigv4::http_request::{SignableBody, SignableRequest, SigningSettings, sign};
    use std::time::SystemTime;

    let credentials = aws
        .credentials
        .provide_credentials()
        .await
        .map_err(|error| {
            CoreError::Provider(format!("AWS default credential chain failed: {error}"))
        })?;
    let identity = credentials.into();
    let params: aws_sigv4::http_request::SigningParams<'_> =
        aws_sigv4::sign::v4::SigningParams::builder()
            .identity(&identity)
            .region(&aws.region)
            .name("bedrock")
            .time(SystemTime::now())
            .settings(SigningSettings::default())
            .build()
            .map_err(|error| {
                CoreError::Provider(format!("AWS signing configuration failed: {error}"))
            })?
            .into();
    let body = request
        .body()
        .and_then(reqwest::Body::as_bytes)
        .unwrap_or_default();
    let header_values: Vec<(String, String)> = request
        .headers()
        .iter()
        .filter_map(|(name, value)| {
            value
                .to_str()
                .ok()
                .map(|value| (name.as_str().to_owned(), value.to_owned()))
        })
        .collect();
    let signable = SignableRequest::new(
        request.method().as_str(),
        request.url().as_str(),
        header_values
            .iter()
            .map(|(name, value)| (name.as_str(), value.as_str())),
        SignableBody::Bytes(body),
    )
    .map_err(|error| CoreError::Provider(format!("AWS request cannot be signed: {error}")))?;
    let (instructions, _) = sign(signable, &params)
        .map_err(|error| CoreError::Provider(format!("AWS SigV4 signing failed: {error}")))?
        .into_parts();
    for (name, value) in instructions.headers() {
        let name = HeaderName::from_bytes(name.as_bytes()).map_err(|_| {
            CoreError::Provider("AWS signer returned an invalid header name".into())
        })?;
        let value = HeaderValue::from_str(value)
            .map_err(|_| CoreError::Provider("AWS signer returned invalid header bytes".into()))?;
        request.headers_mut().insert(name, value);
    }
    for (name, value) in instructions.params() {
        request.url_mut().query_pairs_mut().append_pair(name, value);
    }
    Ok(())
}

fn parse_usage(body: &[u8], coefficient: f64) -> Option<WorkEstimate> {
    fn from_value(value: &serde_json::Value, coefficient: f64) -> Option<WorkEstimate> {
        let usage = value
            .get("usage")
            .or_else(|| value.get("response")?.get("usage"))?;
        let input = usage
            .get("input_tokens")
            .or_else(|| usage.get("prompt_tokens"))?
            .as_u64()?;
        let output = usage
            .get("output_tokens")
            .or_else(|| usage.get("completion_tokens"))
            .and_then(serde_json::Value::as_u64)
            .unwrap_or(0);
        let cached = usage
            .get("input_tokens_details")
            .or_else(|| usage.get("prompt_tokens_details"))
            .and_then(|details| details.get("cached_tokens"))
            .and_then(serde_json::Value::as_u64)
            .unwrap_or(0);
        let normalized = input.saturating_sub(cached).saturating_add(output) as f64 * coefficient;
        Some(WorkEstimate {
            input_tokens: input,
            output_tokens: output,
            cached_input_tokens: cached,
            provider_cost_coefficient: coefficient,
            normalized_units: WorkUnits(normalized),
            confidence: 1.0,
            source: EstimateSource::ProviderMetadata,
        })
    }
    if let Ok(value) = serde_json::from_slice::<serde_json::Value>(body)
        && let Some(usage) = from_value(&value, coefficient)
    {
        return Some(usage);
    }
    std::str::from_utf8(body)
        .ok()?
        .lines()
        .rev()
        .find_map(|line| {
            let json = line.strip_prefix("data:")?.trim();
            if json == "[DONE]" {
                return None;
            }
            serde_json::from_str::<serde_json::Value>(json)
                .ok()
                .and_then(|value| from_value(&value, coefficient))
        })
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
    #[test]
    fn extracts_cached_usage() {
        let body = br#"{"usage":{"input_tokens":100,"output_tokens":20,"input_tokens_details":{"cached_tokens":40}}}"#;
        let usage = parse_usage(body, 2.0).expect("usage");
        assert_eq!(usage.cached_input_tokens, 40);
        assert_eq!(usage.normalized_units.0, 160.0);
    }
}
