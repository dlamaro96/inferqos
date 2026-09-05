//! Executable v1 external-provider gRPC protocol with UDS and mTLS transports.
#![forbid(unsafe_code)]

use async_trait::async_trait;
use http::{HeaderMap, HeaderName, HeaderValue, StatusCode, Uri};
use hyper_util::rt::TokioIo;
use inferqos_core::{
    CoreError, EstimateSource, ProviderAdapter, ProviderResponse, ProxyRequest, UpstreamHealth,
    WorkEstimate, WorkUnits,
};
use std::{collections::HashMap, path::PathBuf, time::Duration};
use tokio::net::UnixStream;
use tokio::sync::{mpsc, watch};
use tonic::transport::{Certificate, Channel, ClientTlsConfig, Endpoint, Identity};
use tower::service_fn;

pub mod v1 {
    tonic::include_proto!("inferqos.provider.v1");
}

#[derive(Debug, Clone)]
pub enum AdapterEndpoint {
    Unix {
        path: PathBuf,
    },
    Tls {
        uri: String,
        domain: String,
        ca_pem: PathBuf,
        client_cert_pem: Option<PathBuf>,
        client_key_pem: Option<PathBuf>,
    },
    Loopback {
        uri: String,
    },
}

pub struct ExternalProviderClient {
    client: v1::provider_adapter_client::ProviderAdapterClient<Channel>,
}

impl ExternalProviderClient {
    pub async fn connect(endpoint: AdapterEndpoint) -> Result<Self, CoreError> {
        let channel = match endpoint {
            AdapterEndpoint::Unix { path } => Endpoint::try_from("http://[::]:50051")
                .map_err(protocol)?
                .connect_with_connector(service_fn(move |_| {
                    let path = path.clone();
                    async move { UnixStream::connect(path).await.map(TokioIo::new) }
                }))
                .await
                .map_err(protocol)?,
            AdapterEndpoint::Loopback { uri } => {
                let parsed: Uri = uri.parse().map_err(protocol)?;
                let host = parsed
                    .host()
                    .ok_or_else(|| CoreError::Provider("adapter URI has no host".into()))?;
                if !matches!(host, "localhost" | "127.0.0.1" | "::1") {
                    return Err(CoreError::Provider(
                        "plaintext external adapters are restricted to loopback".into(),
                    ));
                }
                Endpoint::from_shared(uri)
                    .map_err(protocol)?
                    .connect_timeout(Duration::from_secs(5))
                    .connect()
                    .await
                    .map_err(protocol)?
            }
            AdapterEndpoint::Tls {
                uri,
                domain,
                ca_pem,
                client_cert_pem,
                client_key_pem,
            } => {
                if client_cert_pem.is_some() != client_key_pem.is_some() {
                    return Err(CoreError::Provider(
                        "external adapter mTLS requires both client certificate and key".into(),
                    ));
                }
                let mut tls = ClientTlsConfig::new().domain_name(domain).ca_certificate(
                    Certificate::from_pem(std::fs::read(ca_pem).map_err(protocol)?),
                );
                if let (Some(cert), Some(key)) = (client_cert_pem, client_key_pem) {
                    tls = tls.identity(Identity::from_pem(
                        std::fs::read(cert).map_err(protocol)?,
                        std::fs::read(key).map_err(protocol)?,
                    ));
                }
                Endpoint::from_shared(uri)
                    .map_err(protocol)?
                    .tls_config(tls)
                    .map_err(protocol)?
                    .connect_timeout(Duration::from_secs(5))
                    .connect()
                    .await
                    .map_err(protocol)?
            }
        };
        Ok(Self {
            client: v1::provider_adapter_client::ProviderAdapterClient::new(channel)
                .max_decoding_message_size(16 * 1024 * 1024)
                .max_encoding_message_size(16 * 1024 * 1024),
        })
    }
}

#[async_trait]
impl ProviderAdapter for ExternalProviderClient {
    fn name(&self) -> &'static str {
        "external-grpc"
    }

    async fn estimate(&self, request: &ProxyRequest) -> Result<WorkEstimate, CoreError> {
        let response = self
            .client
            .clone()
            .estimate(v1::EstimateRequest {
                method: request.method.to_string(),
                path: request.uri.to_string(),
                safe_headers: safe_headers(&request.headers),
                body: request.body.clone(),
            })
            .await
            .map_err(protocol)?
            .into_inner();
        from_wire(response)
    }

    async fn dispatch(&self, request: ProxyRequest) -> Result<ProviderResponse, CoreError> {
        let mut stream = self
            .client
            .clone()
            .dispatch(v1::DispatchRequest {
                request_id: request
                    .headers
                    .get("x-inferqos-request-id")
                    .and_then(|value| value.to_str().ok())
                    .unwrap_or_default()
                    .to_owned(),
                method: request.method.to_string(),
                path: request.uri.to_string(),
                safe_headers: safe_headers(&request.headers),
                body: request.body,
            })
            .await
            .map_err(protocol)?
            .into_inner();
        let first = stream.message().await.map_err(protocol)?.ok_or_else(|| {
            CoreError::Provider("external adapter returned an empty stream".into())
        })?;
        let status = StatusCode::from_u16(first.status as u16).map_err(protocol)?;
        let headers = header_map(first.safe_headers)?;
        let (body_tx, body_rx) = mpsc::channel(16);
        let (usage_tx, usage_rx) = watch::channel(first.usage.map(from_wire).transpose()?);
        tokio::spawn(async move {
            if !first.data.is_empty() && body_tx.send(Ok(first.data)).await.is_err() {
                return;
            }
            while let Ok(Some(chunk)) = stream.message().await {
                if let Some(usage) = chunk.usage.and_then(|value| from_wire(value).ok()) {
                    let _ = usage_tx.send(Some(usage));
                }
                if !chunk.data.is_empty() && body_tx.send(Ok(chunk.data)).await.is_err() {
                    break;
                }
                if chunk.end {
                    break;
                }
            }
        });
        Ok(ProviderResponse {
            status,
            headers,
            body: body_rx,
            usage: usage_rx,
        })
    }

    async fn health(&self) -> Result<UpstreamHealth, CoreError> {
        let response = self
            .client
            .clone()
            .health(v1::HealthRequest {})
            .await
            .map_err(protocol)?
            .into_inner();
        Ok(UpstreamHealth {
            healthy: response.healthy,
            detail: response.detail,
        })
    }
}

fn safe_headers(headers: &HeaderMap) -> HashMap<String, String> {
    headers
        .iter()
        .filter(|(name, _)| {
            !matches!(
                name.as_str(),
                "authorization" | "api-key" | "cookie" | "set-cookie"
            )
        })
        .filter_map(|(name, value)| {
            value
                .to_str()
                .ok()
                .map(|value| (name.to_string(), value.to_owned()))
        })
        .collect()
}

fn header_map(values: HashMap<String, String>) -> Result<HeaderMap, CoreError> {
    values
        .into_iter()
        .map(|(name, value)| {
            Ok((
                HeaderName::from_bytes(name.as_bytes()).map_err(protocol)?,
                HeaderValue::from_str(&value).map_err(protocol)?,
            ))
        })
        .collect()
}

fn from_wire(value: v1::WorkEstimate) -> Result<WorkEstimate, CoreError> {
    if !value.normalized_units.is_finite()
        || value.normalized_units < 0.0
        || !(0.0..=1.0).contains(&value.confidence)
    {
        return Err(CoreError::Provider(
            "external adapter returned an invalid work estimate".into(),
        ));
    }
    let source = match value.source.as_str() {
        "exact_tokenizer" => EstimateSource::ExactTokenizer,
        "compatible_tokenizer" => EstimateSource::CompatibleTokenizer,
        "trusted_client" => EstimateSource::TrustedClient,
        "provider_metadata" => EstimateSource::ProviderMetadata,
        _ => EstimateSource::Approximation,
    };
    Ok(WorkEstimate {
        input_tokens: value.input_tokens,
        output_tokens: value.output_tokens,
        cached_input_tokens: value.cached_input_tokens,
        provider_cost_coefficient: value.provider_cost_coefficient,
        normalized_units: WorkUnits(value.normalized_units),
        confidence: value.confidence,
        source,
    })
}

fn protocol(error: impl std::fmt::Display) -> CoreError {
    CoreError::Provider(error.to_string())
}

/// Build a rustls mTLS configuration for a remotely exposed adapter server.
pub fn server_mtls(
    cert_pem: &[u8],
    key_pem: &[u8],
    client_ca_pem: &[u8],
) -> tonic::transport::ServerTlsConfig {
    tonic::transport::ServerTlsConfig::new()
        .identity(Identity::from_pem(cert_pem, key_pem))
        .client_ca_root(Certificate::from_pem(client_ca_pem))
}

/// Generated servers implement this trait and are mounted with `ProviderAdapterServer::new`.
pub use v1::provider_adapter_server::{
    ProviderAdapter as ExternalProviderService, ProviderAdapterServer,
};
