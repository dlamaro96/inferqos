# Contributing

Install stable Rust, Docker, and `just`, then run `just test`. Changes must be formatted, warning-free,
tested, documented, and signed off with `git commit -s`. Provider changes must preserve streaming
and pass the conformance tests. Significant public-contract changes begin as a lightweight RFC in
`docs/rfcs/`. Never include prompts, completions, credentials, customer traces, or generated build
artifacts in a contribution.

