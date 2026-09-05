# Scheduling, deadlines, and capacity

Work estimates contain input, maximum output, cached input, provider coefficient, normalized work,
source, and confidence. Exact provider tokenizers may be plugged in; the default approximation is
conservative and does not call proprietary tokenizer services.

The scheduler accounts service by estimated work rather than request count. Hierarchical weights
cover service class, tenant, and application. Idle shares are borrowed; when competing demand
returns, new admissions restore configured guarantees. Deadline pressure can outrank an older
far-deadline request, while monotonic aging prevents a continuously backlogged low class from being
ignored indefinitely under bounded higher-class demand.

Inference is non-preemptive: once a provider call starts, InferQoS normally lets it finish. QoS is
created by controlling new admission, not by pretending external generation can be paused.

Jain’s index `J=(Σxᵢ)²/(n·Σxᵢ²)` reports normalized tenant-allocation fairness. Capacity Efficiency
under SLO is `useful admitted work meeting its class deadline / configured capacity over the same
window`; raw utilization is reported separately.

