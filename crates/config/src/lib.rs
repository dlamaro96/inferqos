//! Strict, versioned configuration with semantic validation.
#![forbid(unsafe_code)]

use inferqos_core::ServiceClass;
use schemars::{JsonSchema, schema_for};
use serde::{Deserialize, Serialize};
use std::{
    collections::{BTreeMap, BTreeSet},
    net::SocketAddr,
    path::Path,
    time::Duration,
};
use thiserror::Error;

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Config {
    #[serde(rename = "apiVersion")]
    pub api_version: String,
    pub kind: String,
    #[serde(default)]
    pub mode: Mode,
    pub server: ServerConfig,
    pub admin: AdminConfig,
    #[serde(default)]
    pub coordinator: CoordinatorConfig,
    pub service_classes: BTreeMap<String, ServiceClassConfig>,
    pub pools: BTreeMap<String, PoolConfig>,
    #[serde(default)]
    pub policies: Policies,
    #[serde(default)]
    pub identity: IdentityConfig,
    #[serde(default)]
    pub limits: Limits,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct IdentityConfig {
    #[serde(default)]
    pub oidc: Option<OidcConfig>,
    #[serde(default)]
    pub trusted_proxy_cidrs: Vec<String>,
    #[serde(default)]
    pub trusted_headers: TrustedIdentityHeaders,
    #[serde(default)]
    pub mtls_san_mappings: BTreeMap<String, IdentityMapping>,
    /// Lowercase, colon-free SHA-256 fingerprints of directly verified client
    /// certificates. Fingerprints avoid trusting mutable certificate subjects.
    #[serde(default)]
    pub mtls_certificate_sha256_mappings: BTreeMap<String, IdentityMapping>,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct OidcConfig {
    pub issuer: String,
    pub audience: String,
    #[serde(default)]
    pub jwks_url: Option<String>,
    #[serde(default = "default_subject_claim")]
    pub principal_claim: String,
    #[serde(default = "default_tenant_claim")]
    pub tenant_claim: String,
    #[serde(default = "default_application_claim")]
    pub application_claim: String,
    #[serde(default)]
    pub required: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct TrustedIdentityHeaders {
    #[serde(default = "default_principal_header")]
    pub principal: String,
    #[serde(default = "default_tenant_header")]
    pub tenant: String,
    #[serde(default = "default_application_header")]
    pub application: String,
    #[serde(default = "default_client_san_header")]
    pub client_cert_san: String,
}

impl Default for TrustedIdentityHeaders {
    fn default() -> Self {
        Self {
            principal: default_principal_header(),
            tenant: default_tenant_header(),
            application: default_application_header(),
            client_cert_san: default_client_san_header(),
        }
    }
}

#[derive(Debug, Clone, Copy, Default, Serialize, Deserialize, JsonSchema, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum Mode {
    #[default]
    Shadow,
    Enforce,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ServerConfig {
    pub listen: SocketAddr,
    /// Direct data-plane TLS. When configured, client certificates are required
    /// and verified against `client_ca_file` before SAN-to-identity mapping.
    #[serde(default)]
    pub tls: Option<ServerTlsConfig>,
    #[serde(default = "default_body_bytes")]
    pub max_body_bytes: usize,
    #[serde(default = "default_spool_threshold")]
    pub spool_threshold_bytes: usize,
    #[serde(default = "default_spool_directory")]
    pub spool_directory: std::path::PathBuf,
    #[serde(default = "default_reload_interval")]
    #[serde(with = "humantime_serde")]
    #[schemars(with = "String")]
    pub config_reload_interval: Duration,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ServerTlsConfig {
    pub cert_file: std::path::PathBuf,
    pub key_file: std::path::PathBuf,
    pub client_ca_file: std::path::PathBuf,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct AdminConfig {
    pub listen: SocketAddr,
    #[serde(default)]
    pub bearer_token_env: Option<String>,
    #[serde(default)]
    pub expose_decisions: bool,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, JsonSchema)]
#[serde(tag = "type", rename_all = "kebab-case", deny_unknown_fields)]
pub enum CoordinatorConfig {
    #[default]
    Memory,
    Valkey {
        url_env: String,
        #[serde(with = "humantime_serde")]
        #[schemars(with = "String")]
        lease_ttl: Duration,
    },
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ServiceClassConfig {
    pub weight: u32,
    #[serde(with = "humantime_serde")]
    #[schemars(with = "String")]
    pub default_deadline: Duration,
    #[serde(with = "humantime_serde")]
    #[schemars(with = "String")]
    pub max_queue: Duration,
    pub max_queued: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct PoolConfig {
    pub provider: ProviderKind,
    pub endpoint: String,
    pub model: Option<String>,
    pub deployment: Option<String>,
    #[serde(default)]
    pub region: Option<String>,
    #[serde(default)]
    pub external_adapter: Option<ExternalAdapterConfig>,
    pub capacity_units: f64,
    #[serde(default)]
    pub auth: AuthConfig,
    #[serde(default)]
    pub allowed_hosts: BTreeSet<String>,
    #[serde(default = "default_safety")]
    pub initial_safety_factor: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "kebab-case")]
pub enum ProviderKind {
    AzureOpenai,
    AwsBedrock,
    GcpVertex,
    OpenaiCompatible,
    Fake,
    ExternalGrpc,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(tag = "transport", rename_all = "kebab-case", deny_unknown_fields)]
pub enum ExternalAdapterConfig {
    Unix {
        path: std::path::PathBuf,
    },
    Loopback {
        uri: String,
    },
    Tls {
        uri: String,
        domain: String,
        ca_file: std::path::PathBuf,
        #[serde(default)]
        client_cert_file: Option<std::path::PathBuf>,
        #[serde(default)]
        client_key_file: Option<std::path::PathBuf>,
    },
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, JsonSchema)]
#[serde(tag = "type", rename_all = "kebab-case", deny_unknown_fields)]
pub enum AuthConfig {
    #[default]
    None,
    ApiKey {
        env: String,
        #[serde(default = "default_auth_header")]
        header: String,
    },
    Bearer {
        env: String,
    },
    Ambient,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Policies {
    #[serde(default)]
    pub tenants: BTreeMap<String, TenantPolicy>,
    #[serde(default)]
    pub applications: BTreeMap<String, ApplicationPolicy>,
    #[serde(default)]
    pub api_keys: BTreeMap<String, IdentityMapping>,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct TenantPolicy {
    #[serde(default = "one")]
    pub weight: u32,
    #[serde(default)]
    pub guaranteed_share: f64,
    #[serde(default = "one_f64")]
    pub max_share: f64,
    #[serde(default = "default_concurrency")]
    pub max_concurrency: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ApplicationPolicy {
    pub tenant: String,
    pub allowed_classes: BTreeSet<String>,
    #[serde(default = "one")]
    pub weight: u32,
    #[serde(default = "default_concurrency")]
    pub max_concurrency: usize,
    #[serde(default)]
    pub permitted_pools: BTreeSet<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct IdentityMapping {
    pub principal: String,
    pub tenant: String,
    pub application: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Limits {
    #[serde(default = "default_queue_depth")]
    pub total_queue_depth: usize,
    #[serde(default = "default_queue_bytes")]
    pub total_queue_bytes: usize,
    #[serde(default = "default_decisions")]
    pub decision_history: usize,
    #[serde(default = "default_replicas")]
    pub expected_replicas: usize,
    #[serde(default)]
    pub allow_unsafe_uncoordinated_ha: bool,
}
impl Default for Limits {
    fn default() -> Self {
        Self {
            total_queue_depth: default_queue_depth(),
            total_queue_bytes: default_queue_bytes(),
            decision_history: default_decisions(),
            expected_replicas: 1,
            allow_unsafe_uncoordinated_ha: false,
        }
    }
}

impl Config {
    pub fn from_path(path: &Path) -> Result<Self, ConfigError> {
        let raw = match std::fs::read_to_string(path) {
            Ok(raw) => raw,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                std::env::var("INFERQOS_CONFIG_YAML").map_err(|_| ConfigError::Read(error))?
            }
            Err(error) => return Err(ConfigError::Read(error)),
        };
        let expanded = expand_env(&raw)?;
        let config: Self = serde_yaml::from_str(&expanded).map_err(ConfigError::Parse)?;
        config.validate()?;
        Ok(config)
    }
    pub fn validate(&self) -> Result<(), ConfigError> {
        if self.api_version != "inferqos.io/v1alpha1" || self.kind != "InferQoSConfig" {
            return Err(ConfigError::Semantic(
                "apiVersion must be inferqos.io/v1alpha1 and kind must be InferQoSConfig".into(),
            ));
        }
        if self.pools.is_empty() {
            return Err(ConfigError::Semantic(
                "at least one capacity pool is required".into(),
            ));
        }
        if self.server.max_body_bytes == 0
            || self.server.spool_threshold_bytes > self.server.max_body_bytes
        {
            return Err(ConfigError::Semantic(
                "body limits must be positive and spool threshold cannot exceed max body bytes"
                    .into(),
            ));
        }
        if let Some(tls) = &self.server.tls {
            for (label, path) in [
                ("server TLS certificate", &tls.cert_file),
                ("server TLS private key", &tls.key_file),
                ("mTLS client CA", &tls.client_ca_file),
            ] {
                if path.as_os_str().is_empty() {
                    return Err(ConfigError::Semantic(format!(
                        "{label} path cannot be empty"
                    )));
                }
            }
        }
        for required in [
            ServiceClass::Realtime,
            ServiceClass::Interactive,
            ServiceClass::Standard,
            ServiceClass::Workflow,
            ServiceClass::Batch,
        ] {
            if !self.service_classes.contains_key(&required.to_string()) {
                return Err(ConfigError::Semantic(format!(
                    "missing built-in service class {required}"
                )));
            }
        }
        for (name, class) in &self.service_classes {
            if class.weight == 0 || class.max_queued == 0 {
                return Err(ConfigError::Semantic(format!(
                    "service class {name} must have non-zero weight and max_queued"
                )));
            }
        }
        for (name, pool) in &self.pools {
            if !pool.capacity_units.is_finite() || pool.capacity_units <= 0.0 {
                return Err(ConfigError::Semantic(format!(
                    "pool {name} capacity_units must be positive"
                )));
            }
            if !(1.0..=4.0).contains(&pool.initial_safety_factor) {
                return Err(ConfigError::Semantic(format!(
                    "pool {name} initial_safety_factor must be in [1,4]"
                )));
            }
        }
        for (name, tenant) in &self.policies.tenants {
            if !(0.0..=1.0).contains(&tenant.guaranteed_share)
                || !(tenant.guaranteed_share..=1.0).contains(&tenant.max_share)
            {
                return Err(ConfigError::Semantic(format!(
                    "tenant {name} shares must satisfy 0 <= guaranteed <= max <= 1"
                )));
            }
        }
        for (name, app) in &self.policies.applications {
            if !self.policies.tenants.contains_key(&app.tenant) {
                return Err(ConfigError::Semantic(format!(
                    "application {name} references unknown tenant {}",
                    app.tenant
                )));
            }
            for class in &app.allowed_classes {
                if !self.service_classes.contains_key(class) {
                    return Err(ConfigError::Semantic(format!(
                        "application {name} references unknown class {class}"
                    )));
                }
            }
        }
        for (name, pool) in &self.pools {
            if matches!(pool.provider, ProviderKind::ExternalGrpc)
                != pool.external_adapter.is_some()
            {
                return Err(ConfigError::Semantic(format!(
                    "pool {name} must configure external_adapter exactly when provider is external-grpc"
                )));
            }
            if let Some(ExternalAdapterConfig::Tls {
                client_cert_file,
                client_key_file,
                ..
            }) = &pool.external_adapter
                && client_cert_file.is_some() != client_key_file.is_some()
            {
                return Err(ConfigError::Semantic(format!(
                    "pool {name} external adapter mTLS requires both client_cert_file and client_key_file"
                )));
            }
        }
        for cidr in &self.identity.trusted_proxy_cidrs {
            cidr.parse::<ipnet::IpNet>().map_err(|error| {
                ConfigError::Semantic(format!("trusted proxy CIDR {cidr:?} is invalid: {error}"))
            })?;
        }
        for fingerprint in self.identity.mtls_certificate_sha256_mappings.keys() {
            if fingerprint.len() != 64
                || !fingerprint.bytes().all(|value| value.is_ascii_hexdigit())
            {
                return Err(ConfigError::Semantic(format!(
                    "mTLS certificate fingerprint {fingerprint:?} must contain exactly 64 hexadecimal characters"
                )));
            }
        }
        if let Some(oidc) = &self.identity.oidc {
            let issuer = url::Url::parse(&oidc.issuer).map_err(|error| {
                ConfigError::Semantic(format!("OIDC issuer is invalid: {error}"))
            })?;
            if issuer.scheme() != "https" && issuer.host_str() != Some("localhost") {
                return Err(ConfigError::Semantic(
                    "OIDC issuer must use HTTPS except for localhost tests".into(),
                ));
            }
            if oidc.audience.is_empty() {
                return Err(ConfigError::Semantic(
                    "OIDC audience cannot be empty".into(),
                ));
            }
        }
        if self.limits.expected_replicas > 1
            && matches!(self.coordinator, CoordinatorConfig::Memory)
            && !self.limits.allow_unsafe_uncoordinated_ha
        {
            return Err(ConfigError::Semantic("multiple replicas sharing capacity require the Valkey coordinator; set allow_unsafe_uncoordinated_ha only for isolated pools".into()));
        }
        Ok(())
    }
    pub fn json_schema() -> String {
        serde_json::to_string_pretty(&schema_for!(Config))
            .expect("schema serialization is infallible")
    }
}

fn expand_env(input: &str) -> Result<String, ConfigError> {
    let mut out = String::with_capacity(input.len());
    let mut rest = input;
    while let Some(start) = rest.find("${") {
        out.push_str(&rest[..start]);
        let after = &rest[start + 2..];
        let end = after
            .find('}')
            .ok_or_else(|| ConfigError::Environment("unclosed environment substitution".into()))?;
        let key = &after[..end];
        if key.is_empty() || !key.chars().all(|c| c.is_ascii_alphanumeric() || c == '_') {
            return Err(ConfigError::Environment(format!(
                "invalid environment variable name {key:?}"
            )));
        }
        let value = std::env::var(key).map_err(|_| {
            ConfigError::Environment(format!("required environment variable {key} is not set"))
        })?;
        out.push_str(&value);
        rest = &after[end + 1..];
    }
    out.push_str(rest);
    Ok(out)
}

fn one() -> u32 {
    1
}
fn one_f64() -> f64 {
    1.0
}
fn default_concurrency() -> usize {
    100
}
fn default_body_bytes() -> usize {
    16 * 1024 * 1024
}
fn default_spool_threshold() -> usize {
    256 * 1024
}
fn default_spool_directory() -> std::path::PathBuf {
    std::env::temp_dir().join("inferqos-spool")
}
fn default_reload_interval() -> Duration {
    Duration::from_secs(2)
}
fn default_safety() -> f64 {
    1.15
}
fn default_auth_header() -> String {
    "api-key".into()
}
fn default_queue_depth() -> usize {
    10_000
}
fn default_queue_bytes() -> usize {
    256 * 1024 * 1024
}
fn default_decisions() -> usize {
    2_048
}
fn default_replicas() -> usize {
    1
}
fn default_subject_claim() -> String {
    "sub".into()
}
fn default_tenant_claim() -> String {
    "tenant".into()
}
fn default_application_claim() -> String {
    "application".into()
}
fn default_principal_header() -> String {
    "x-inferqos-principal".into()
}
fn default_tenant_header() -> String {
    "x-inferqos-tenant".into()
}
fn default_application_header() -> String {
    "x-inferqos-application".into()
}
fn default_client_san_header() -> String {
    "x-forwarded-client-cert-san".into()
}

#[derive(Debug, Error)]
pub enum ConfigError {
    #[error("cannot read configuration: {0}")]
    Read(std::io::Error),
    #[error("invalid configuration syntax: {0}")]
    Parse(serde_yaml::Error),
    #[error("invalid configuration: {0}")]
    Semantic(String),
    #[error("environment expansion failed: {0}")]
    Environment(String),
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn unknown_fields_fail() {
        let err =
            serde_yaml::from_str::<ServerConfig>("listen: 127.0.0.1:1\nextra: true").unwrap_err();
        assert!(err.to_string().contains("unknown field"));
    }
    #[test]
    fn schema_has_version() {
        assert!(Config::json_schema().contains("apiVersion"));
    }
}
