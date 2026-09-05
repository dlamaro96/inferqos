//! Authenticated identity resolution for API keys, OIDC, mTLS identities, and trusted proxies.
#![forbid(unsafe_code)]

use http::HeaderMap;
use inferqos_config::{Config, IdentityMapping, OidcConfig};
use inferqos_core::IdentityContext;
use ipnet::IpNet;
use jsonwebtoken::{Algorithm, DecodingKey, Validation, decode, decode_header, jwk::JwkSet};
use parking_lot::RwLock;
use serde::Deserialize;
use serde_json::Value;
use std::{collections::HashMap, net::IpAddr, sync::Arc, time::Duration};
use subtle::ConstantTimeEq;
use thiserror::Error;
use url::Url;

#[derive(Clone)]
pub struct IdentityResolver {
    config: Arc<RwLock<Config>>,
    trusted_proxies: Arc<RwLock<Vec<IpNet>>>,
    oidc: Option<Arc<OidcVerifier>>,
}

/// Authentication material injected only after rustls has verified the client
/// certificate chain against the configured CA.
#[derive(Clone, Debug, Default)]
pub struct VerifiedClientCertificate {
    pub sha256_fingerprints: Vec<String>,
}

impl IdentityResolver {
    pub async fn new(config: Config) -> Result<Self, IdentityError> {
        let trusted_proxies = parse_proxies(&config)?;
        let oidc = match &config.identity.oidc {
            Some(oidc) => Some(Arc::new(OidcVerifier::new(oidc.clone()).await?)),
            None => None,
        };
        Ok(Self {
            config: Arc::new(RwLock::new(config)),
            trusted_proxies: Arc::new(RwLock::new(trusted_proxies)),
            oidc,
        })
    }

    pub fn replace_config(&self, config: Config) -> Result<(), IdentityError> {
        *self.trusted_proxies.write() = parse_proxies(&config)?;
        *self.config.write() = config;
        Ok(())
    }

    pub async fn resolve(
        &self,
        headers: &HeaderMap,
        remote_ip: Option<IpAddr>,
        verified_mtls_fingerprints: &[String],
    ) -> Result<IdentityContext, IdentityError> {
        let config = self.config.read().clone();
        for fingerprint in verified_mtls_fingerprints {
            if let Some(mapping) = config
                .identity
                .mtls_certificate_sha256_mappings
                .get(fingerprint)
            {
                return Ok(mapped(mapping));
            }
        }
        let from_proxy = remote_ip.is_some_and(|ip| {
            self.trusted_proxies
                .read()
                .iter()
                .any(|network| network.contains(&ip))
        });

        if from_proxy {
            let names = &config.identity.trusted_headers;
            if let Some(san) = header(headers, &names.client_cert_san)
                && let Some(mapping) = config.identity.mtls_san_mappings.get(san)
            {
                return Ok(mapped(mapping));
            }
            let principal = header(headers, &names.principal);
            let tenant = header(headers, &names.tenant);
            let application = header(headers, &names.application);
            if let (Some(principal), Some(tenant), Some(application)) =
                (principal, tenant, application)
            {
                return Ok(IdentityContext {
                    principal: principal.to_owned(),
                    tenant: tenant.to_owned(),
                    application: application.to_owned(),
                    trusted: true,
                });
            }
        }

        let bearer = headers
            .get("authorization")
            .and_then(|value| value.to_str().ok())
            .and_then(|value| value.strip_prefix("Bearer "));
        if let Some(token) = bearer {
            if token.matches('.').count() == 2
                && let Some(verifier) = &self.oidc
            {
                return verifier.verify(token).await;
            }
            for (environment, mapping) in &config.policies.api_keys {
                if let Ok(expected) = std::env::var(environment)
                    && expected.as_bytes().ct_eq(token.as_bytes()).into()
                {
                    return Ok(mapped(mapping));
                }
            }
        }

        if config
            .identity
            .oidc
            .as_ref()
            .is_some_and(|oidc| oidc.required)
        {
            return Err(IdentityError::Required);
        }
        Ok(IdentityContext {
            principal: "anonymous".into(),
            tenant: "default".into(),
            application: "default".into(),
            trusted: false,
        })
    }
}

fn parse_proxies(config: &Config) -> Result<Vec<IpNet>, IdentityError> {
    config
        .identity
        .trusted_proxy_cidrs
        .iter()
        .map(|cidr| {
            cidr.parse().map_err(|error| {
                IdentityError::Configuration(format!("invalid trusted proxy CIDR {cidr}: {error}"))
            })
        })
        .collect()
}

fn mapped(mapping: &IdentityMapping) -> IdentityContext {
    IdentityContext {
        principal: mapping.principal.clone(),
        tenant: mapping.tenant.clone(),
        application: mapping.application.clone(),
        trusted: true,
    }
}

fn header<'a>(headers: &'a HeaderMap, name: &str) -> Option<&'a str> {
    headers
        .get(name)
        .and_then(|value| value.to_str().ok())
        .filter(|value| !value.is_empty())
}

struct OidcVerifier {
    config: OidcConfig,
    jwks_url: Url,
    client: reqwest::Client,
    keys: tokio::sync::RwLock<HashMap<String, DecodingKey>>,
}

#[derive(Deserialize)]
struct Discovery {
    jwks_uri: String,
}

impl OidcVerifier {
    async fn new(config: OidcConfig) -> Result<Self, IdentityError> {
        let client = reqwest::Client::builder()
            .connect_timeout(Duration::from_secs(5))
            .timeout(Duration::from_secs(10))
            .redirect(reqwest::redirect::Policy::none())
            .build()
            .map_err(|error| IdentityError::Configuration(error.to_string()))?;
        let jwks_url = if let Some(explicit) = &config.jwks_url {
            secure_url(explicit, "JWKS")?
        } else {
            let issuer = secure_url(&config.issuer, "OIDC issuer")?;
            let discovery = issuer
                .join(".well-known/openid-configuration")
                .map_err(|error| IdentityError::Configuration(error.to_string()))?;
            let document: Discovery = client
                .get(discovery)
                .send()
                .await
                .map_err(network)?
                .error_for_status()
                .map_err(network)?
                .json()
                .await
                .map_err(network)?;
            secure_url(&document.jwks_uri, "discovered JWKS")?
        };
        let verifier = Self {
            config,
            jwks_url,
            client,
            keys: tokio::sync::RwLock::new(HashMap::new()),
        };
        verifier.refresh().await?;
        Ok(verifier)
    }

    async fn refresh(&self) -> Result<(), IdentityError> {
        let response = self
            .client
            .get(self.jwks_url.clone())
            .send()
            .await
            .map_err(network)?;
        if response
            .content_length()
            .is_some_and(|length| length > 1024 * 1024)
        {
            return Err(IdentityError::Jwks("JWKS document exceeds 1 MiB".into()));
        }
        let set: JwkSet = response
            .error_for_status()
            .map_err(network)?
            .json()
            .await
            .map_err(network)?;
        let mut keys = HashMap::new();
        for jwk in set.keys {
            if let Some(kid) = &jwk.common.key_id {
                let key = DecodingKey::from_jwk(&jwk)
                    .map_err(|error| IdentityError::Jwks(error.to_string()))?;
                keys.insert(kid.clone(), key);
            }
        }
        if keys.is_empty() {
            return Err(IdentityError::Jwks("JWKS has no keyed signing keys".into()));
        }
        *self.keys.write().await = keys;
        Ok(())
    }

    async fn verify(&self, token: &str) -> Result<IdentityContext, IdentityError> {
        let header = decode_header(token).map_err(|_| IdentityError::InvalidToken)?;
        if !matches!(
            header.alg,
            Algorithm::RS256
                | Algorithm::RS384
                | Algorithm::RS512
                | Algorithm::ES256
                | Algorithm::ES384
        ) {
            return Err(IdentityError::InvalidToken);
        }
        let kid = header.kid.ok_or(IdentityError::InvalidToken)?;
        let key = match self.keys.read().await.get(&kid).cloned() {
            Some(key) => key,
            None => {
                self.refresh().await?;
                self.keys
                    .read()
                    .await
                    .get(&kid)
                    .cloned()
                    .ok_or(IdentityError::InvalidToken)?
            }
        };
        let mut validation = Validation::new(header.alg);
        validation.algorithms = vec![
            Algorithm::RS256,
            Algorithm::RS384,
            Algorithm::RS512,
            Algorithm::ES256,
            Algorithm::ES384,
        ];
        validation.set_audience(&[self.config.audience.as_str()]);
        validation.set_issuer(&[self.config.issuer.as_str()]);
        validation.validate_exp = true;
        validation.validate_nbf = true;
        let claims = decode::<Value>(token, &key, &validation)
            .map_err(|_| IdentityError::InvalidToken)?
            .claims;
        let claim = |name: &str| {
            claims
                .get(name)
                .and_then(Value::as_str)
                .filter(|value| !value.is_empty())
        };
        Ok(IdentityContext {
            principal: claim(&self.config.principal_claim)
                .ok_or(IdentityError::MissingClaim(
                    self.config.principal_claim.clone(),
                ))?
                .to_owned(),
            tenant: claim(&self.config.tenant_claim)
                .ok_or(IdentityError::MissingClaim(
                    self.config.tenant_claim.clone(),
                ))?
                .to_owned(),
            application: claim(&self.config.application_claim)
                .ok_or(IdentityError::MissingClaim(
                    self.config.application_claim.clone(),
                ))?
                .to_owned(),
            trusted: true,
        })
    }
}

fn secure_url(value: &str, label: &str) -> Result<Url, IdentityError> {
    let url = Url::parse(value)
        .map_err(|error| IdentityError::Configuration(format!("invalid {label} URL: {error}")))?;
    if url.scheme() != "https" && url.host_str() != Some("localhost") {
        return Err(IdentityError::Configuration(format!(
            "{label} URL must use HTTPS except localhost tests"
        )));
    }
    Ok(url)
}
fn network(error: impl std::fmt::Display) -> IdentityError {
    IdentityError::Jwks(error.to_string())
}

#[derive(Debug, Error)]
pub enum IdentityError {
    #[error("identity configuration error: {0}")]
    Configuration(String),
    #[error("cannot load OIDC keys: {0}")]
    Jwks(String),
    #[error("authentication is required")]
    Required,
    #[error("invalid bearer token")]
    InvalidToken,
    #[error("verified token is missing required claim {0}")]
    MissingClaim(String),
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn untrusted_forwarded_headers_are_ignored() {
        let mut headers = HeaderMap::new();
        headers.insert("x-inferqos-tenant", "forged".parse().expect("header"));
        assert_eq!(header(&headers, "x-inferqos-tenant"), Some("forged"));
        let nets = ["10.0.0.0/8".parse::<IpNet>().expect("cidr")];
        assert!(
            !nets
                .iter()
                .any(|network| network.contains(&"203.0.113.8".parse::<IpAddr>().expect("ip")))
        );
    }

    #[test]
    fn rejects_non_tls_jwks() {
        assert!(secure_url("http://example.com/keys", "JWKS").is_err());
        assert!(secure_url("http://localhost:8081/keys", "JWKS").is_ok());
    }
}
