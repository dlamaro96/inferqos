---
title: "ADR 0010: Objective-oriented QoS API"
---

# ADR 0010: Objective-oriented QoS API

- Status: Accepted
- Date: 2026-09-05

Public clients request named service classes and deadlines through experimental vendor-prefixed
headers. They cannot access scheduler buckets or self-award priority. The management API is versioned
under `/api/v1` and remains separate from transparent proxy routes.
