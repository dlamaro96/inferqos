# Differentiation

| Category | Primary question | Relationship to InferQoS |
|---|---|---|
| API gateway | Is this caller allowed, and where should traffic go? | Keep it; place InferQoS before or after it. |
| LLM gateway | Which model/provider and what gateway features apply? | Keep routing explicit; InferQoS schedules capacity. |
| Rate limiter | How many requests arrived? | InferQoS estimates variable work and finite capacity. |
| Provider throttling | Is the provider already saturated? | InferQoS admits earlier using local state and feedback. |
| GPU scheduler | Which model server/GPU executes work? | Below InferQoS and out of scope. |
| Kubernetes inference routing | Which service replica receives a request? | Orthogonal to business-aware admission. |

These are category comparisons, not claims that every product in a category behaves identically.

