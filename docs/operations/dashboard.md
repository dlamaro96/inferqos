# Operations dashboard

The embedded dashboard is served from the management listener at `/ui`. It displays metadata only:

- process readiness, version, uptime, drain state, and active requests;
- queue depth and buffered metadata bytes;
- configured and reserved capacity per pool;
- adaptive safety factor, estimate error, confidence, and observations;
- recent admission, queue, rejection, and throttle counters;
- service-class queue outcomes derived from the bounded decision history;
- recent request IDs, effective classes, tenant/application mappings, estimated work, and outcomes.

It never displays prompts, completions, API keys, access tokens, raw user IDs, or configuration
secrets. Disabling decision history removes the recent-decision and class-outcome data.

## Access

The admin listener defaults to loopback. Keep it internal in production. If it is exposed beyond a
trusted management network, configure `admin.bearer_token_env`. The dashboard asks for the token and
keeps it only in the current page memory; it does not write the token to local or session storage.

All dashboard assets are compiled into the main binary and protected by the management plane's
Content Security Policy. No CDN, font service, analytics endpoint, or hosted InferQoS service is
contacted.

## Color and accessibility

The dashboard supports light and dark modes, keyboard navigation, visible focus, reduced motion,
responsive tables, live error states, and semantic status announcements. The chosen mode is stored
locally in the browser and contains no operational information.
