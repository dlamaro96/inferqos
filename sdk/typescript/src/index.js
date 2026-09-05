const classes = new Set(["realtime", "interactive", "standard", "workflow", "batch"]);
export function qosHeaders(serviceClass, { deadlineMs, queueable = true } = {}) {
  if (!classes.has(serviceClass)) throw new TypeError("unknown InferQoS service class");
  if (deadlineMs !== undefined && (!Number.isInteger(deadlineMs) || deadlineMs <= 0)) throw new TypeError("deadlineMs must be a positive integer");
  const headers = { "X-InferQoS-Class": serviceClass, "X-InferQoS-Queueable": String(queueable) };
  if (deadlineMs !== undefined) headers["X-InferQoS-Deadline-Ms"] = String(deadlineMs);
  return headers;
}

