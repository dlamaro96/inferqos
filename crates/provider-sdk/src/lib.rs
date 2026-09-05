//! SDK for in-process providers. The scheduler only sees these provider-neutral contracts.
#![forbid(unsafe_code)]
pub use inferqos_core::{
    CoreError, EstimateSource, ProviderAdapter, ProviderResponse, ProxyRequest, ThrottleSignal,
    UpstreamHealth, WorkEstimate, WorkEstimator, WorkUnits,
};

/// Checks invariants common to every work estimate. Network behavior is covered by the repository's
/// provider conformance harness and deterministic fake provider.
pub fn validate_estimate(estimate: &WorkEstimate) -> Result<(), CoreError> {
    if !estimate.normalized_units.0.is_finite() || estimate.normalized_units.0 < 0.0 {
        return Err(CoreError::InvalidWork(estimate.normalized_units.0));
    }
    if !(0.0..=1.0).contains(&estimate.confidence) {
        return Err(CoreError::Provider(
            "estimate confidence must be in [0,1]".into(),
        ));
    }
    if estimate.cached_input_tokens > estimate.input_tokens {
        return Err(CoreError::Provider(
            "cached input cannot exceed input tokens".into(),
        ));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn rejects_impossible_cached_usage() {
        let e = WorkEstimate {
            input_tokens: 1,
            output_tokens: 1,
            cached_input_tokens: 2,
            provider_cost_coefficient: 1.0,
            normalized_units: WorkUnits(2.0),
            confidence: 1.0,
            source: EstimateSource::ExactTokenizer,
        };
        assert!(validate_estimate(&e).is_err());
    }
}
