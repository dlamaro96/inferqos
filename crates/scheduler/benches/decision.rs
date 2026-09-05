use inferqos_core::{
    AdmissionRequest, EstimateSource, IdentityContext, ServiceClass, WorkEstimate, WorkUnits,
};
use inferqos_scheduler::{Scheduler, SchedulerConfig, VirtualClock};
use std::{
    sync::Arc,
    time::{Duration, Instant},
};
use uuid::Uuid;
fn main() {
    let clock = Arc::new(VirtualClock::default());
    let scheduler = Scheduler::new(clock, SchedulerConfig::default());
    let start = Instant::now();
    let n = 100_000;
    for i in 0..n {
        let r = AdmissionRequest {
            id: Uuid::new_v4(),
            identity: IdentityContext {
                principal: "bench".into(),
                tenant: format!("t{}", i % 100),
                application: "app".into(),
                trusted: true,
            },
            requested_class: ServiceClass::Standard,
            effective_class: ServiceClass::Standard,
            pool: "p".into(),
            estimate: WorkEstimate {
                input_tokens: 100,
                output_tokens: 50,
                cached_input_tokens: 0,
                provider_cost_coefficient: 1.0,
                normalized_units: WorkUnits(150.0),
                confidence: 1.0,
                source: EstimateSource::ExactTokenizer,
            },
            deadline: Duration::from_secs(10),
            queueable: true,
        };
        scheduler.enqueue(r, 0, 20, 1, 1).unwrap();
        scheduler.pop_next();
    }
    let elapsed = start.elapsed();
    println!(
        "scheduler decisions: {n}; total: {elapsed:?}; mean: {:?}",
        elapsed / n
    );
    assert!(elapsed / n < Duration::from_millis(1));
}
