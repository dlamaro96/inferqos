# ADR 0004: Hierarchical weighted deficit scheduling with deadline pressure

- Status: Accepted
- Date: 2026-09-05

The scheduler uses hierarchical weighted deficit accounting across service class, tenant, and
application. Each queue receives quantum proportional to its configured weight, unused service
is borrowable, and work estimates are charged rather than request counts. An urgency term boosts
requests as their monotonic deadline approaches; queue age provides bounded starvation relief.
Running inference is not preempted. Deterministic sequence numbers break ties.

The chosen design is work-conserving and has predictable O(n) selection over active queues; the
implementation caps active queues and queued bytes so this bound is operationally meaningful.

