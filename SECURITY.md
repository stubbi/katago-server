# Security policy

## Supported versions

The latest minor release receives fixes. Older releases are not patched;
upgrade to the current image tag or chart version.

## Reporting a vulnerability

Use GitHub's private vulnerability reporting on
https://github.com/goban-app/katago-server/security/advisories/new. Do not open
a public issue. You should hear back within a week.

## Scope and deployment notes

- The server has no authentication or TLS. Run it behind a reverse proxy, an
  ingress with auth, or a network policy. Never expose it directly to the
  internet.
- CORS allows any origin by default; set `server.cors_allowed_origins` to your
  front-end origins.
- Abuse limits: `server.max_body_bytes`, `server.max_concurrent_requests`,
  `server.request_timeout_secs`, `katago.move_timeout_secs` and
  `katago.max_visits_limit`. Set the visit limit on any shared deployment.
- `overrideSettings` is passed to KataGo unchanged (apart from the visit limit).
  It can change search behaviour but cannot reach the filesystem.
- Images run as a non-root user and contain only the server, KataGo and the
  network files. Dependencies are checked with `cargo-deny` and updated by
  Dependabot.
