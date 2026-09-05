# Finite Inference QoS Metadata 0.1

Status: experimental discussion draft; no standards-body endorsement is claimed.

An implementation accepts a service objective (class and relative deadline), authenticated
resource subject (tenant/application), queueability, and an opaque request identifier. It returns an
admission outcome: admit, wait, reject, or shadow projection. Public metadata describes objectives,
not scheduler buckets or algorithms. Content semantics are outside the scheduling contract.

