"""Dependency-free InferQoS request metadata helpers."""
from contextlib import contextmanager
from contextvars import ContextVar

_headers: ContextVar[dict[str, str]] = ContextVar("inferqos_headers", default={})

def headers() -> dict[str, str]:
    return dict(_headers.get())

@contextmanager
def qos(service_class: str, deadline_ms: int | None = None, queueable: bool = True):
    if service_class not in {"realtime", "interactive", "standard", "workflow", "batch"}:
        raise ValueError("unknown InferQoS service class")
    value = {"X-InferQoS-Class": service_class, "X-InferQoS-Queueable": str(queueable).lower()}
    if deadline_ms is not None:
        if deadline_ms <= 0:
            raise ValueError("deadline_ms must be positive")
        value["X-InferQoS-Deadline-Ms"] = str(deadline_ms)
    token = _headers.set(value)
    try:
        yield value
    finally:
        _headers.reset(token)

def interactive(deadline_ms: int = 3000):
    return qos("interactive", deadline_ms)

