# Problem types

Every error response has `Content-Type: application/problem+json` and a `type`
URL pointing at one of the sections below. The fragment is the slug.

## invalid-request

Status 400. The request is well-formed JSON but violates a rule: a coordinate off
the board, mixed move formats, an out-of-range board size or komi, a bad colour,
a `maxVisits` above the server limit, or a field KataGo itself rejected (for
example an illegal move or unknown rules name). `field` names the offending
field and `detail` explains it. Fix the request; do not retry unchanged.

## malformed-json

Status 400. The body could not be parsed as JSON. `detail` includes the parser
message. Fix the body.

## unsupported-media-type

Status 415. The request lacked `Content-Type: application/json`. Add the header.

## payload-too-large

Status 413. The body exceeds `server.max_body_bytes` (default 1 MiB). Shorten the
request or raise the limit.

## not-found

Status 404. No route matches the path. `instance` echoes the path.

## method-not-allowed

Status 405. The path exists but not for that method.

## analysis-timeout

Status 504. KataGo did not deliver a result within `katago.move_timeout_secs`
(per analysed turn). The server told KataGo to terminate the query. Lower
`maxVisits`, raise the timeout, or use a faster network or GPU.

## request-timeout

Status 504. The complete HTTP request exceeded `server.request_timeout_secs`.
The abandoned query is terminated in KataGo. Split large game analyses or raise
the limit.

## engine-unavailable

Status 503. The KataGo process is not running. The server restarts it with
exponential backoff up to `katago.max_restart_attempts`; retry after a few
seconds. If `/api/v1/health/live` also returns 503, the budget is spent and the
server must be restarted.

## analysis-terminated

Status 503. The search was terminated before it produced a result, typically
because the server is shutting down. Retry against a healthy instance.

## shutting-down

Status 503. The server received SIGTERM or SIGINT and accepts no new work.
Retry against another instance.

## overloaded

Status 503. More than `server.max_concurrent_requests` requests were in flight.
Retry with backoff, or raise the limit if the hardware allows.

## engine-error

Status 502. KataGo returned an error that is not attributable to the request,
or output the server could not parse. Check the server logs (KataGo's stderr is
captured). Report persistent cases as a bug.

## internal-error

Status 500. An unexpected failure inside the server (I/O or serialisation).
Check logs and report it.
