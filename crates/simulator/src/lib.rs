//! Metadata-only deterministic workload replay and capacity analysis.
#![forbid(unsafe_code)]
use anyhow::{Context, Result, bail};
use serde::{Deserialize, Serialize};
use std::{collections::BTreeMap, path::Path};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TraceEvent {
    pub timestamp_ms: u64,
    #[serde(default)]
    pub provider: String,
    #[serde(default)]
    pub model: String,
    #[serde(default)]
    pub pool: String,
    #[serde(default = "default_name")]
    pub application: String,
    #[serde(default = "default_name")]
    pub tenant: String,
    #[serde(default = "default_class")]
    pub service_class: String,
    #[serde(default)]
    pub input_tokens: u64,
    #[serde(default)]
    pub cached_tokens: u64,
    #[serde(default)]
    pub requested_max_output: u64,
    #[serde(default)]
    pub actual_output: u64,
    #[serde(default)]
    pub latency_ms: u64,
    #[serde(default)]
    pub status: u16,
    #[serde(default)]
    pub retry_after_ms: u64,
    #[serde(default)]
    pub throttled: bool,
}
fn default_name() -> String {
    "default".into()
}
fn default_class() -> String {
    "standard".into()
}
#[derive(Debug, Clone, Serialize)]
pub struct ClassReport {
    pub requests: u64,
    pub queue_p50_ms: u64,
    pub queue_p95_ms: u64,
    pub queue_p99_ms: u64,
    pub deadline_violations: u64,
}
#[derive(Debug, Clone, Serialize)]
pub struct ReplayReport {
    pub configured_capacity: f64,
    pub average_effective_demand: f64,
    pub peak_effective_demand: f64,
    pub utilization_p50: f64,
    pub utilization_p95: f64,
    pub baseline_throttle_rate: f64,
    pub projected_throttle_rate: f64,
    pub fairness_jain_index: f64,
    pub starvation_incidents: u64,
    pub capacity_headroom: f64,
    pub potential_capacity_deferred_units: f64,
    pub estimated_cost_avoided: Option<f64>,
    pub confidence: String,
    pub recommendation: String,
    pub classes: BTreeMap<String, ClassReport>,
    pub assumptions: Vec<String>,
}

pub fn read_events(path: &Path) -> Result<Vec<TraceEvent>> {
    let extension = path.extension().and_then(|x| x.to_str()).unwrap_or("");
    match extension {
        "csv" => {
            let mut reader = csv::Reader::from_path(path)
                .with_context(|| format!("cannot read {}", path.display()))?;
            reader
                .deserialize()
                .map(|v| v.context("invalid CSV trace row"))
                .collect()
        }
        "jsonl" | "json" | "trace" => {
            let text = std::fs::read_to_string(path)
                .with_context(|| format!("cannot read {}", path.display()))?;
            text.lines()
                .filter(|l| !l.trim().is_empty())
                .enumerate()
                .map(|(i, l)| {
                    serde_json::from_str(l)
                        .with_context(|| format!("invalid JSON on line {}", i + 1))
                })
                .collect()
        }
        _ => bail!("unsupported trace format; use .jsonl, .trace, .json, or .csv"),
    }
}

pub fn replay(
    events: &[TraceEvent],
    capacity: f64,
    cost_per_unit: Option<f64>,
    capacity_increment: Option<f64>,
) -> Result<ReplayReport> {
    if events.is_empty() {
        bail!("trace contains no events")
    }
    if !capacity.is_finite() || capacity <= 0.0 {
        bail!("capacity must be positive")
    }
    let mut ordered = events.to_vec();
    ordered.sort_by_key(|e| e.timestamp_ms);
    let start = ordered[0].timestamp_ms;
    let end = ordered
        .last()
        .expect("nonempty")
        .timestamp_ms
        .max(start + 1);
    let duration_s = ((end - start) as f64 / 1000.0).max(1.0);
    let work: Vec<f64> = ordered
        .iter()
        .map(|e| {
            (e.input_tokens.saturating_sub(e.cached_tokens)
                + e.requested_max_output.max(e.actual_output)) as f64
        })
        .collect();
    let total: f64 = work.iter().sum();
    let avg = total / duration_s;
    let mut buckets: BTreeMap<u64, f64> = BTreeMap::new();
    for (e, w) in ordered.iter().zip(&work) {
        *buckets.entry((e.timestamp_ms - start) / 1000).or_default() += w;
    }
    let mut demand: Vec<f64> = buckets.values().copied().collect();
    demand.sort_by(f64::total_cmp);
    let peak = demand.last().copied().unwrap_or(0.0);
    let utils: Vec<f64> = demand.iter().map(|d| d / capacity).collect();
    let observed = ordered
        .iter()
        .filter(|e| e.throttled || e.status == 429)
        .count() as f64
        / ordered.len() as f64;
    let queueable = ordered
        .iter()
        .filter(|e| matches!(e.service_class.as_str(), "workflow" | "batch" | "standard"))
        .count() as f64
        / ordered.len() as f64;
    let excess_ratio = ((peak - capacity).max(0.0) / peak.max(1.0)).min(1.0);
    let projected = (observed * (1.0 - queueable * 0.9) + excess_ratio * (1.0 - queueable) * 0.1)
        .clamp(0.0, 1.0);
    let mut by_class: BTreeMap<String, Vec<u64>> = BTreeMap::new();
    for (e, w) in ordered.iter().zip(work) {
        let pressure = (w / capacity).max(0.0);
        let class_factor = match e.service_class.as_str() {
            "realtime" => 0.02,
            "interactive" => 0.08,
            "standard" => 0.3,
            "workflow" => 0.7,
            "batch" => 1.0,
            _ => 0.4,
        };
        let wait =
            ((pressure * 1000.0 * class_factor) as u64).saturating_sub(e.latency_ms.min(100));
        by_class
            .entry(e.service_class.clone())
            .or_default()
            .push(wait);
    }
    let classes = by_class
        .into_iter()
        .map(|(name, mut values)| {
            values.sort();
            let report = ClassReport {
                requests: values.len() as u64,
                queue_p50_ms: percentile(&values, 0.50),
                queue_p95_ms: percentile(&values, 0.95),
                queue_p99_ms: percentile(&values, 0.99),
                deadline_violations: values.iter().filter(|&&v| v > deadline_for(&name)).count()
                    as u64,
            };
            (name, report)
        })
        .collect();
    let mut tenant: BTreeMap<String, f64> = BTreeMap::new();
    for (e, w) in ordered.iter().zip(
        events
            .iter()
            .map(|e| (e.input_tokens + e.actual_output) as f64),
    ) {
        *tenant.entry(e.tenant.clone()).or_default() += w
    }
    let allocations: Vec<f64> = tenant.values().copied().collect();
    let fairness = jain(&allocations);
    let sustained =
        demand.iter().filter(|&&d| d > capacity).count() as f64 / demand.len().max(1) as f64;
    let deferred = capacity_increment.map_or(0.0, |inc| {
        if sustained < 0.2 && queueable > 0.2 {
            ((peak - capacity).max(0.0) / inc).ceil() * inc
        } else {
            0.0
        }
    });
    let recommendation = if avg > capacity * 0.9 && sustained > 0.5 {
        "Scheduling cannot solve sustained saturation; additional capacity is recommended."
    } else if peak > capacity && queueable > 0.2 {
        "Short peaks and queueable work indicate QoS scheduling may defer a capacity increase."
    } else {
        "Capacity appears adequate; use shadow mode to validate SLO impact before enforcement."
    }
    .into();
    Ok(ReplayReport {
        configured_capacity: capacity,
        average_effective_demand: avg,
        peak_effective_demand: peak,
        utilization_p50: percentile_f64(&utils, 0.5),
        utilization_p95: percentile_f64(&utils, 0.95),
        baseline_throttle_rate: observed,
        projected_throttle_rate: projected,
        fairness_jain_index: fairness,
        starvation_incidents: 0,
        capacity_headroom: (capacity - avg).max(0.0),
        potential_capacity_deferred_units: deferred,
        estimated_cost_avoided: cost_per_unit.map(|c| c * deferred),
        confidence: if ordered.len() > 1000 {
            "high"
        } else if ordered.len() > 100 {
            "medium"
        } else {
            "low"
        }
        .into(),
        recommendation,
        classes,
        assumptions: vec![
            "work units use uncached input plus max(actual output, requested max output)".into(),
            "one-second demand buckets approximate provider capacity windows".into(),
            "projected results are estimates, not guaranteed savings or SLOs".into(),
        ],
    })
}
fn percentile(v: &[u64], p: f64) -> u64 {
    v.get(((v.len().saturating_sub(1)) as f64 * p).round() as usize)
        .copied()
        .unwrap_or(0)
}
fn percentile_f64(v: &[f64], p: f64) -> f64 {
    v.get(((v.len().saturating_sub(1)) as f64 * p).round() as usize)
        .copied()
        .unwrap_or(0.0)
}
fn deadline_for(c: &str) -> u64 {
    match c {
        "realtime" => 500,
        "interactive" => 3000,
        "standard" => 10000,
        "workflow" => 60000,
        _ => 1_800_000,
    }
}
fn jain(x: &[f64]) -> f64 {
    if x.is_empty() {
        return 1.0;
    }
    let sum: f64 = x.iter().sum();
    let sq: f64 = x.iter().map(|v| v * v).sum();
    if sq == 0.0 {
        1.0
    } else {
        sum * sum / (x.len() as f64 * sq)
    }
}
pub fn terminal(report: &ReplayReport) -> String {
    let mut s = format!(
        "Capacity configured          {:.1} units\nAverage effective demand     {:.1} units/s\nPeak effective demand        {:.1} units/s\nObserved throttling          {:0.2}%\nProjected throttling         {:0.2}%\nFairness (Jain index)        {:.3}\nCapacity headroom            {:.1} units\nPotential capacity deferred  {:.1} units\nConfidence                   {}\n\n{}\n",
        report.configured_capacity,
        report.average_effective_demand,
        report.peak_effective_demand,
        report.baseline_throttle_rate * 100.0,
        report.projected_throttle_rate * 100.0,
        report.fairness_jain_index,
        report.capacity_headroom,
        report.potential_capacity_deferred_units,
        report.confidence,
        report.recommendation
    );
    for (c, r) in &report.classes {
        s.push_str(&format!(
            "{c:12} queue p50/p95/p99 {:>6}/{:>6}/{:>6} ms; deadline violations {}\n",
            r.queue_p50_ms, r.queue_p95_ms, r.queue_p99_ms, r.deadline_violations
        ));
    }
    s
}
pub fn html(report: &ReplayReport) -> Result<String> {
    let data = serde_json::to_string(report)?;
    Ok(format!(
        r#"<!doctype html><meta charset=utf-8><title>InferQoS replay</title><style>body{{font:16px system-ui;max-width:900px;margin:48px auto;color:#18212f}}pre{{padding:24px;background:#f4f6f8;border-radius:12px;white-space:pre-wrap}}</style><h1>InferQoS replay report</h1><p>Metadata-only projection. No prompts or completions were required.</p><pre id=r></pre><script>r.textContent=JSON.stringify({data},null,2)</script>"#
    ))
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn sustained_saturation_is_honest() {
        let events = (0..100)
            .map(|i| TraceEvent {
                timestamp_ms: i * 1000,
                provider: String::new(),
                model: String::new(),
                pool: String::new(),
                application: "a".into(),
                tenant: "t".into(),
                service_class: "interactive".into(),
                input_tokens: 100,
                cached_tokens: 0,
                requested_max_output: 100,
                actual_output: 100,
                latency_ms: 10,
                status: 200,
                retry_after_ms: 0,
                throttled: false,
            })
            .collect::<Vec<_>>();
        let r = replay(&events, 100.0, None, Some(50.0)).unwrap();
        assert!(r.recommendation.contains("additional capacity"));
    }
}
