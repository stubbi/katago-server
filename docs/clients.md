# Client snippets

Generate a typed client from `GET /api/v1/openapi.json` with any OpenAPI 3.1
generator when you need more than the snippets below.

## Python

```python
import requests

BASE = "http://localhost:2718"

def analyse(moves, **kwargs):
    r = requests.post(f"{BASE}/api/v1/analysis", json={"moves": moves, **kwargs}, timeout=120)
    if r.status_code >= 400:
        problem = r.json()  # RFC 9457
        raise RuntimeError(f"{problem['title']}: {problem['detail']} (field={problem.get('field')})")
    return r.json()

result = analyse(["D4", "Q16", "R4"], komi=7.5, rules="chinese", maxVisits=50, includeOwnership=True)
best = result["moveInfos"][0]
print(best["moveCoord"], f"{best['winrate']:.1%}", best["scoreLead"])

review = requests.post(f"{BASE}/api/v1/analysis/game",
                       json={"moves": ["D4", "Q16", "R4"], "maxVisits": 20}, timeout=600).json()
for turn in review["turns"]:
    root = turn["rootInfo"]
    print(turn["turnNumber"], root["currentPlayer"], f"{root['winrate']:.1%}")
```

## JavaScript / TypeScript

```ts
const BASE = "http://localhost:2718";

async function analyse(body: Record<string, unknown>) {
  const res = await fetch(`${BASE}/api/v1/analysis`, {
    method: "POST",
    headers: { "Content-Type": "application/json" },
    body: JSON.stringify(body),
  });
  if (!res.ok) {
    const problem = await res.json(); // { type, title, status, detail, field?, requestId? }
    throw new Error(`${problem.title}: ${problem.detail}`);
  }
  return res.json();
}

const result = await analyse({ moves: ["D4", "Q16"], komi: 7.5, maxVisits: 50, includeOwnership: true });
console.log(result.moveInfos[0].moveCoord, result.rootInfo.winrate);
```

## Rust

```rust
// reqwest = { version = "0.12", features = ["json"] }, serde_json = "1"
let client = reqwest::Client::new();
let res = client
    .post("http://localhost:2718/api/v1/analysis/game")
    .json(&serde_json::json!({ "moves": ["D4", "Q16", "R4"], "maxVisits": 20 }))
    .send()
    .await?;
if !res.status().is_success() {
    let problem: serde_json::Value = res.json().await?;
    anyhow::bail!("{}: {}", problem["title"], problem["detail"]);
}
let review: serde_json::Value = res.json().await?;
for turn in review["turns"].as_array().unwrap() {
    println!("{} {}", turn["turnNumber"], turn["rootInfo"]["winrate"]);
}
```

## Handicap games

Place the handicap stones with `initialStones`; White then moves first by default.

```json
{ "initialStones": [["B", "D4"], ["B", "Q16"], ["B", "D16"]], "moves": ["Q4", "R6"], "komi": 0.5, "rules": "chinese" }
```

## Human-style analysis

Requires a `human-*` or `combo-*` image (or `katago.human_model_path`):

```json
{ "moves": ["D4", "Q16"], "maxVisits": 20, "includePolicy": true, "overrideSettings": { "humanSLProfile": "rank_5k" } }
```

The response gains `humanPolicy`, `moveInfos[].humanPrior` and `rootInfo.humanWinrate`.
