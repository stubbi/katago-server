# API reference

Base path: `/api/v1`. All request and response bodies are JSON in camelCase. The
interactive reference at `/docs` and the document at `/api/v1/openapi.json` are
generated from the code and are authoritative for schemas.

## POST /api/v1/analysis

Analyses the position after all `moves` have been played.

### Request body

| Field | Type | Default | Notes |
|---|---|---|---|
| `moves` | array | `[]` | Either all bare coordinates (`"D4"`, `"pass"`) or all `[colour, coordinate]` pairs (`["W", "D4"]`). Mixing the two forms is rejected. Bare moves alternate colours starting with `initialPlayer`. |
| `rules` | string or object | Japanese for integer or 6.5 komi, Chinese otherwise | A KataGo rules name (`chinese`, `japanese`, `korean`, `aga`, `tromp-taylor`, ...) or a KataGo rules object. Names are lower-cased. Unknown names are rejected by KataGo and returned as 400 with `field: rules`. |
| `komi` | number | `7.5` | Multiple of 0.5, magnitude at most 150. |
| `boardXSize`, `boardYSize` | integer | `19` | 2 to 25. |
| `initialStones` | array of `[colour, coordinate]` | none | Handicap or setup stones. No passes, no duplicates. |
| `initialPlayer` | `"B"` or `"W"` | `"W"` if `initialStones` is non-empty, else `"B"` | Forwarded to KataGo and used to colour bare moves. |
| `analyzeTurns` | array of integers | final position | Each value from 0 to `moves.length`. Duplicates are removed. For this endpoint only the last result is returned; use `/analysis/game` for several turns. |
| `maxVisits` | integer >= 1 | server `default_max_visits` (10) | Rejected if above the server's `max_visits_limit`. |
| `rootPolicyTemperature` | number > 0 | KataGo default | |
| `rootFpuReductionMax` | number | KataGo default | |
| `analysisPVLen` | integer >= 1 | KataGo default | `analysisPvLen` is accepted as an alias. |
| `includeOwnership` | bool | false | Adds `ownership`. |
| `includeOwnershipStdev` | bool | false | Adds `ownershipStdev`. |
| `includeMovesOwnership` | bool | false | Adds `ownership` to each `moveInfos` entry. |
| `includePolicy` | bool | false | Adds `policy`. |
| `includePVVisits` | bool | false | Adds `pvVisits` and `pvEdgeVisits`. `includePvVisits` is accepted as an alias. |
| `avoidMoves`, `allowMoves` | array of `{player, moves, untilDepth}` | none | `player` is `B`/`W`, `untilDepth` >= 1, coordinates validated for the board. |
| `overrideSettings` | object | none | Passed to KataGo unchanged. `maxVisits` inside it is checked against `max_visits_limit`. |
| `priority` | integer | none | KataGo scheduling priority. |
| `requestId` | string, at most 128 chars | none | Echoed back as `id`. Never sent to KataGo; the server uses its own UUID internally. |

Colours accept `B`, `W`, `b`, `w`, `black`, `white`. Coordinates use GTP letters
(`A` to `Z`, skipping `I`) and rows counted from the bottom, case-insensitive.

`reportDuringSearchEvery` is not supported; partial results are never returned.
Unknown fields are ignored.

### Response

```json
{
  "id": "requestId or generated UUID",
  "turnNumber": 3,
  "isDuringSearch": false,
  "moveInfos": [
    {
      "moveCoord": "D16", "visits": 21, "winrate": 0.52, "scoreMean": 1.9, "scoreStdev": 12.1,
      "scoreLead": 1.9, "scoreSelfplay": 2.3, "utility": 0.03, "utilityLcb": -0.1, "lcb": 0.49,
      "prior": 0.18, "order": 0, "edgeVisits": 21, "edgeWeight": 21.4, "weight": 21.4,
      "playSelectionValue": 21.4, "pv": ["D16", "C14"], "pvVisits": [21, 9], "pvEdgeVisits": [21, 9],
      "ownership": [0.02, -0.11], "humanPrior": 0.2
    }
  ],
  "rootInfo": {
    "winrate": 0.51, "scoreLead": 1.5, "scoreSelfplay": 1.7, "scoreStdev": 12.0, "utility": 0.02,
    "visits": 50, "currentPlayer": "W", "weight": 50.2, "rawWinrate": 0.5, "rawLead": 1.2,
    "rawScoreSelfplay": 1.3, "rawScoreSelfplayStdev": 12.9, "rawStScoreError": 0.5,
    "rawStWrError": 0.03, "rawNoResultProb": 0.0008, "rawVarTimeLeft": 50.1,
    "symHash": "…", "thisHash": "…",
    "humanWinrate": 0.5, "humanScoreMean": 1.0, "humanScoreStdev": 11.0
  },
  "ownership": [0.02, -0.11],
  "ownershipStdev": [0.03],
  "policy": [0.00001],
  "humanPolicy": [0.0001]
}
```

- `winrate` and `scoreLead` are from the perspective of the side to move (`reportAnalysisWinratesAs = SIDETOMOVE` in the shipped configs).
- `ownership` and `ownershipStdev` have `boardXSize * boardYSize` entries, row by row from the top. Positive means the side to move.
- `policy` and `humanPolicy` have `boardXSize * boardYSize + 1` entries; the last one is pass.
- Optional fields are omitted when KataGo did not report them. `noResults: true` never appears in a 200 response; a terminated search is a 503 `analysis-terminated`.
- `human*` fields need a Human SL network (`human-*` or `combo-*` images, or `katago.human_model_path`) and `"overrideSettings": {"humanSLProfile": "rank_5k"}` or similar.

## POST /api/v1/analysis/game

Same request body. `analyzeTurns` defaults to every turn from 0 to `moves.length`.
KataGo analyses all turns in one query, so this is much cheaper than one request per move.

```json
{
  "id": "requestId or generated UUID",
  "boardXSize": 19,
  "boardYSize": 19,
  "turns": [ { "turnNumber": 0, "...": "an /analysis response" }, { "turnNumber": 1, "...": "..." } ]
}
```

`turns` is ordered by `turnNumber`. Each entry carries the same `id`. The engine
timeout (`katago.move_timeout_secs`) applies per turn as an inactivity limit; the
whole request is bounded by `server.request_timeout_secs`.

## GET /api/v1/health

Returns 200 once KataGo has loaded its network and answered its first query, 503 otherwise.

```json
{
  "status": "healthy | starting | unhealthy",
  "timestamp": "2026-09-07T10:00:00Z",
  "uptime": 3600,
  "katago": { "alive": true, "ready": true, "restarts": 0, "version": "1.18.2" }
}
```

## GET /api/v1/health/live

`{"status": "live"}` with 200 while the process can still serve or recover. 503
(`dead`) only when KataGo is down and the restart budget (`max_restart_attempts`)
is spent. Use this for Kubernetes liveness.

## GET /api/v1/health/ready

`{"status": "ready"}` with 200 once KataGo is ready, else 503 (`not-ready`). Use
this for readiness and startup probes.

## GET /api/v1/version

```json
{
  "server": { "name": "katago-server", "version": "1.8.0", "gitSha": "abc1234" },
  "katago": { "version": "1.18.2", "gitHash": "…" },
  "model": { "name": "kata1-b28c512nbt-s12043015936-d5616446734.bin.gz", "humanModel": "b18c384nbt-humanv0.bin.gz" }
}
```

`katago` is omitted until KataGo has answered its first query. This endpoint never blocks on KataGo.

## POST /api/v1/cache/clear

Clears the neural network cache and waits for KataGo's acknowledgement.
Returns `{"status": "cleared", "timestamp": "..."}`.

## GET /metrics

Prometheus text exposition. See [deployment.md](deployment.md#observability) for the metric list.

## GET /api/v1/openapi.json and GET /docs

The OpenAPI 3.1 document and an interactive reference rendered from it.

## GET /

`{"name": "katago-server", "version": "...", "docs": "/docs", "openapi": "/api/v1/openapi.json", "health": "/api/v1/health"}`.

## Headers

Every response carries `x-request-id`. Clients may send their own; otherwise a UUID is generated.
CORS is enabled for the origins in `server.cors_allowed_origins` (default: any).

## Errors

Every error is `application/problem+json` (RFC 9457):

```json
{
  "type": "https://github.com/goban-app/katago-server/blob/main/docs/problems.md#invalid-request",
  "title": "Invalid Request",
  "status": 400,
  "detail": "moves[1] (\"Z99\") is not on a 19x19 board (columns A-T, skipping I, rows 1-19, or \"pass\")",
  "instance": "/api/v1/analysis",
  "requestId": "game-7",
  "field": "moves"
}
```

`instance`, `requestId` and `field` are present when known.

| Slug | Status | When |
|---|---|---|
| `invalid-request` | 400 | Validation failed, or KataGo rejected a field (`field` says which). |
| `malformed-json` | 400 | Body is not valid JSON. |
| `unsupported-media-type` | 415 | Missing `Content-Type: application/json`. |
| `payload-too-large` | 413 | Body exceeds `server.max_body_bytes`. |
| `not-found` | 404 | Unknown path. |
| `method-not-allowed` | 405 | Wrong method for the path. |
| `analysis-timeout` | 504 | KataGo produced no result within `katago.move_timeout_secs`. The query is terminated in KataGo. |
| `request-timeout` | 504 | The whole request exceeded `server.request_timeout_secs`. |
| `engine-unavailable` | 503 | KataGo is not running; a restart is in progress or the budget is spent. |
| `analysis-terminated` | 503 | The search was terminated before producing a result (shutdown). |
| `shutting-down` | 503 | The server is stopping. |
| `overloaded` | 503 | More than `server.max_concurrent_requests` in flight. |
| `engine-error` | 502 | KataGo returned an error not tied to the request, or unparseable output. |
| `internal-error` | 500 | Unexpected server failure. |

Details and client guidance per slug: [problems.md](problems.md).
