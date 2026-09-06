//! Public request and response types for the `/api/v1` REST API.
//!
//! Response types derive `Deserialize` as well because they double as the
//! parse target for KataGo's own JSON output, which uses the same names.

use serde::{Deserialize, Serialize};
use utoipa::ToSchema;

fn default_board_size() -> u8 {
    19
}

/// A move: either a bare coordinate (colours alternate) or an explicit
/// `[colour, coordinate]` pair. Mixing both forms in one request is rejected.
#[derive(Debug, Clone, Deserialize, ToSchema)]
#[serde(untagged)]
pub enum MoveInput {
    /// Coordinate only, e.g. `"D4"` or `"pass"`.
    Simple(String),
    /// Explicit colour and coordinate, e.g. `["W", "D4"]`.
    WithColor([String; 2]),
}

impl MoveInput {
    /// The coordinate part of the move.
    pub fn coord(&self) -> &str {
        match self {
            Self::Simple(c) | Self::WithColor([_, c]) => c,
        }
    }

    /// The explicit colour, if the move carries one.
    pub fn color(&self) -> Option<&str> {
        match self {
            Self::Simple(_) => None,
            Self::WithColor([color, _]) => Some(color),
        }
    }
}

/// Rules: either a KataGo rules name or a full rules object.
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
#[serde(untagged)]
pub enum Rules {
    /// A named rule set such as `"chinese"`, `"japanese"`, `"tromp-taylor"`, `"aga"`.
    Named(String),
    /// A KataGo rules object, e.g. `{"ko":"SIMPLE","scoring":"AREA","tax":"NONE",...}`.
    #[schema(value_type = Object)]
    Custom(serde_json::Map<String, serde_json::Value>),
}

/// Restricts which moves the search may consider for a player.
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct MoveFilter {
    /// `"B"` or `"W"`.
    #[schema(example = "B")]
    pub player: String,
    /// Coordinates the filter applies to.
    #[schema(example = json!(["C3", "D4"]))]
    pub moves: Vec<String>,
    /// How many plies deep into the search the filter applies (1 = root only).
    #[schema(example = 1)]
    pub until_depth: u32,
}

/// Request body for position and game analysis.
#[derive(Debug, Clone, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct AnalysisRequest {
    /// Moves played so far, in order.
    #[serde(default)]
    #[schema(example = json!(["D4", "Q16", "R4"]))]
    pub moves: Vec<MoveInput>,

    /// Rule set. Defaults to Japanese for integer or 6.5 komi, Chinese otherwise.
    #[serde(default)]
    #[schema(example = "chinese")]
    pub rules: Option<Rules>,

    /// Komi. Defaults to 7.5.
    #[serde(default)]
    #[schema(example = 7.5)]
    pub komi: Option<f64>,

    /// Board width, 2..=25.
    #[serde(default = "default_board_size")]
    #[schema(example = 19, minimum = 2, maximum = 25)]
    pub board_x_size: u8,

    /// Board height, 2..=25.
    #[serde(default = "default_board_size")]
    #[schema(example = 19, minimum = 2, maximum = 25)]
    pub board_y_size: u8,

    /// Stones placed before the first move, e.g. handicap stones.
    #[serde(default)]
    #[schema(example = json!([["B", "D4"], ["B", "Q16"]]))]
    pub initial_stones: Option<Vec<[String; 2]>>,

    /// Player to move first. Defaults to White when initial stones are present, else Black.
    #[serde(default)]
    #[schema(example = "B")]
    pub initial_player: Option<String>,

    /// Turns to analyse (0 = before the first move). Position analysis analyses the
    /// final position only; game analysis defaults to every turn.
    #[serde(default)]
    pub analyze_turns: Option<Vec<u32>>,

    /// Search visits per position. Falls back to the server default.
    #[serde(default)]
    #[schema(example = 100, minimum = 1)]
    pub max_visits: Option<u32>,

    /// Root policy temperature; values above 1 broaden the search.
    #[serde(default)]
    pub root_policy_temperature: Option<f64>,

    /// Maximum first-play-urgency reduction at the root.
    #[serde(default)]
    pub root_fpu_reduction_max: Option<f64>,

    /// Number of moves to return in each principal variation.
    #[serde(default, rename = "analysisPVLen", alias = "analysisPvLen")]
    pub analysis_pv_len: Option<u32>,

    /// Include predicted ownership of every intersection.
    #[serde(default)]
    pub include_ownership: Option<bool>,

    /// Include the standard deviation of ownership predictions.
    #[serde(default)]
    pub include_ownership_stdev: Option<bool>,

    /// Include ownership after each candidate move.
    #[serde(default)]
    pub include_moves_ownership: Option<bool>,

    /// Include the raw neural network policy (board points plus pass).
    #[serde(default)]
    pub include_policy: Option<bool>,

    /// Include visit counts along each principal variation.
    #[serde(default, rename = "includePVVisits", alias = "includePvVisits")]
    pub include_pv_visits: Option<bool>,

    /// Moves the search must not consider.
    #[serde(default)]
    pub avoid_moves: Option<Vec<MoveFilter>>,

    /// The only moves the search may consider.
    #[serde(default)]
    pub allow_moves: Option<Vec<MoveFilter>>,

    /// Per-request overrides of KataGo analysis settings (e.g. `humanSLProfile`).
    #[serde(default)]
    #[schema(value_type = Option<Object>, example = json!({"humanSLProfile": "rank_5k"}))]
    pub override_settings: Option<serde_json::Map<String, serde_json::Value>>,

    /// Scheduling priority; higher runs first.
    #[serde(default)]
    pub priority: Option<i64>,

    /// Client-chosen identifier echoed back as `id`.
    #[serde(default)]
    #[schema(example = "my-request-42")]
    pub request_id: Option<String>,
}

/// Analysis of one candidate move.
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct MoveInfo {
    /// The candidate move.
    #[serde(alias = "move")]
    pub move_coord: String,
    /// Search visits spent on this move.
    pub visits: u64,
    /// Win probability for the side to move after playing this move.
    pub winrate: f64,
    /// Expected final score difference.
    pub score_mean: f64,
    /// Standard deviation of the expected score.
    #[serde(default)]
    pub score_stdev: f64,
    /// Expected score lead.
    pub score_lead: f64,
    /// Expected self-play score.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub score_selfplay: Option<f64>,
    /// Combined utility.
    #[serde(default)]
    pub utility: f64,
    /// Lower confidence bound on utility.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub utility_lcb: Option<f64>,
    /// Lower confidence bound on winrate.
    pub lcb: f64,
    /// Neural network prior probability.
    pub prior: f64,
    /// Human SL network prior (requires a human model).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub human_prior: Option<f64>,
    /// Rank among candidates, 0 = best.
    pub order: u64,
    /// Visits through the edge leading to this move.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub edge_visits: Option<u64>,
    /// Weight through the edge leading to this move.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub edge_weight: Option<f64>,
    /// Total weight of this subtree.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub weight: Option<f64>,
    /// Value used by KataGo when choosing a move to play.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub play_selection_value: Option<f64>,
    /// Another candidate this move is a symmetry of, if any.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub is_symmetry_of: Option<String>,
    /// Principal variation starting with this move.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub pv: Vec<String>,
    /// Visits at each step of the principal variation.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub pv_visits: Option<Vec<u64>>,
    /// Edge visits at each step of the principal variation.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub pv_edge_visits: Option<Vec<u64>>,
    /// Ownership after this move (requires `includeMovesOwnership`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub ownership: Option<Vec<f64>>,
}

/// Evaluation of the position itself.
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct RootInfo {
    /// Win probability for the side to move.
    pub winrate: f64,
    /// Expected score lead for the side to move.
    pub score_lead: f64,
    /// Expected self-play score.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub score_selfplay: Option<f64>,
    /// Standard deviation of the expected score.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub score_stdev: Option<f64>,
    /// Combined utility.
    #[serde(default)]
    pub utility: f64,
    /// Total visits in the search.
    pub visits: u64,
    /// Side to move: `"B"` or `"W"`.
    pub current_player: String,
    /// Total weight in the search tree.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub weight: Option<f64>,
    /// Raw neural network winrate (no search).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub raw_winrate: Option<f64>,
    /// Raw neural network score lead.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub raw_lead: Option<f64>,
    /// Raw neural network score (older KataGo versions).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub raw_score_mean: Option<f64>,
    /// Raw neural network self-play score.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub raw_score_selfplay: Option<f64>,
    /// Raw self-play score standard deviation.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub raw_score_selfplay_stdev: Option<f64>,
    /// Raw short-term score error estimate.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub raw_st_score_error: Option<f64>,
    /// Raw short-term winrate error estimate.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub raw_st_wr_error: Option<f64>,
    /// Raw probability of a no-result game.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub raw_no_result_prob: Option<f64>,
    /// Raw variance of remaining game length.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub raw_var_time_left: Option<f64>,
    /// Hash of the position up to symmetry.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub sym_hash: Option<String>,
    /// Hash of the exact position.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub this_hash: Option<String>,
    /// Human SL network winrate (requires a human model).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub human_winrate: Option<f64>,
    /// Human SL network score mean.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub human_score_mean: Option<f64>,
    /// Human SL network score standard deviation.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub human_score_stdev: Option<f64>,
    /// Human SL network short-term winrate error.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub human_st_wr_error: Option<f64>,
    /// Human SL network short-term score error.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub human_st_score_error: Option<f64>,
}

/// Analysis of a single position.
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct AnalysisResponse {
    /// Request identifier (the client's `requestId`, or a generated UUID).
    pub id: String,
    /// Number of moves played before the analysed position.
    #[serde(default)]
    pub turn_number: u32,
    /// Always `false`; partial results are never returned.
    #[serde(default)]
    pub is_during_search: bool,
    /// `true` when the search was terminated before producing results.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub no_results: bool,
    /// Candidate moves, best first.
    #[serde(default)]
    pub move_infos: Vec<MoveInfo>,
    /// Evaluation of the position.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub root_info: Option<RootInfo>,
    /// Ownership per intersection, row by row from the top, -1 (White) to 1 (Black)
    /// from the perspective of the side to move.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub ownership: Option<Vec<f64>>,
    /// Standard deviation of ownership per intersection.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub ownership_stdev: Option<Vec<f64>>,
    /// Raw policy per intersection plus one trailing entry for pass.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub policy: Option<Vec<f64>>,
    /// Human SL network policy (requires a human model).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub human_policy: Option<Vec<f64>>,
}

/// Analysis of every requested turn of a game.
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct GameAnalysisResponse {
    /// Request identifier (the client's `requestId`, or a generated UUID).
    pub id: String,
    /// Board width.
    pub board_x_size: u8,
    /// Board height.
    pub board_y_size: u8,
    /// One entry per analysed turn, ordered by `turnNumber`.
    pub turns: Vec<AnalysisResponse>,
}

/// Server build information.
#[derive(Debug, Clone, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct ServerVersion {
    /// Always `"katago-server"`.
    pub name: String,
    /// Semantic version.
    pub version: String,
    /// Git commit the binary was built from, when known.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub git_sha: Option<String>,
}

/// KataGo build information.
#[derive(Debug, Clone, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct KatagoVersionInfo {
    /// KataGo version, e.g. `"1.18.2"`.
    pub version: String,
    /// KataGo git hash, when the build recorded it.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub git_hash: Option<String>,
}

/// Loaded neural network files.
#[derive(Debug, Clone, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct ModelInfo {
    /// File name of the main network.
    pub name: String,
    /// File name of the Human SL network, when loaded.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub human_model: Option<String>,
}

/// Response of `GET /api/v1/version`.
#[derive(Debug, Clone, Serialize, ToSchema)]
pub struct VersionResponse {
    /// Server build information.
    pub server: ServerVersion,
    /// KataGo build information; absent until KataGo has answered its first query.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub katago: Option<KatagoVersionInfo>,
    /// Loaded models.
    pub model: ModelInfo,
}

/// State of the KataGo subprocess.
#[derive(Debug, Clone, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct EngineHealth {
    /// KataGo process is running.
    pub alive: bool,
    /// KataGo has loaded its network and answered a query.
    pub ready: bool,
    /// Times KataGo was restarted since the server started.
    pub restarts: u32,
    /// KataGo version once known.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub version: Option<String>,
}

/// Response of `GET /api/v1/health`.
#[derive(Debug, Clone, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct HealthResponse {
    /// `"healthy"`, `"starting"` or `"unhealthy"`.
    #[schema(example = "healthy")]
    pub status: String,
    /// Current time, RFC 3339.
    pub timestamp: String,
    /// Seconds since the server started.
    pub uptime: u64,
    /// KataGo subprocess state.
    pub katago: EngineHealth,
}

/// Response of `POST /api/v1/cache/clear`.
#[derive(Debug, Clone, Serialize, ToSchema)]
pub struct CacheClearResponse {
    /// Always `"cleared"`.
    pub status: String,
    /// Current time, RFC 3339.
    pub timestamp: String,
}

/// Response of `GET /`.
#[derive(Debug, Clone, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct IndexResponse {
    /// Always `"katago-server"`.
    pub name: String,
    /// Semantic version.
    pub version: String,
    /// Where to find the interactive API documentation.
    pub docs: String,
    /// Where to find the OpenAPI document.
    pub openapi: String,
    /// Where to find the health endpoint.
    pub health: String,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn request_accepts_simple_and_explicit_moves() {
        let req: AnalysisRequest = serde_json::from_str(
            r#"{"moves": ["D4", ["W", "Q16"]], "komi": 7.5, "rules": "chinese", "includeOwnership": true}"#,
        )
        .unwrap();
        assert_eq!(req.moves.len(), 2);
        assert_eq!(req.moves[0].coord(), "D4");
        assert_eq!(req.moves[0].color(), None);
        assert_eq!(req.moves[1].coord(), "Q16");
        assert_eq!(req.moves[1].color(), Some("W"));
        assert_eq!(req.komi, Some(7.5));
        assert!(matches!(req.rules, Some(Rules::Named(ref r)) if r == "chinese"));
        assert_eq!(req.include_ownership, Some(true));
        assert_eq!(req.board_x_size, 19);
    }

    #[test]
    fn request_accepts_rules_object_and_empty_body() {
        let req: AnalysisRequest =
            serde_json::from_str(r#"{"rules": {"ko": "SIMPLE", "scoring": "AREA"}}"#).unwrap();
        assert!(req.moves.is_empty());
        assert!(matches!(req.rules, Some(Rules::Custom(_))));
        let req: AnalysisRequest = serde_json::from_str("{}").unwrap();
        assert!(req.moves.is_empty());
    }

    #[test]
    fn parses_real_katago_output() {
        let raw = r#"{"id":"a1","isDuringSearch":false,"moveInfos":[{"edgeVisits":2,"edgeWeight":4.37,"lcb":-0.157,"move":"R17","order":0,"ownership":[-0.06],"playSelectionValue":4.37,"prior":0.236,"pv":["R17","D4"],"pvEdgeVisits":[2,1],"pvVisits":[2,1],"scoreLead":-0.89,"scoreMean":-0.89,"scoreSelfplay":-1.27,"scoreStdev":14.42,"utility":-0.25,"utilityLcb":-1.75,"visits":2,"weight":4.37,"winrate":0.377}],"ownership":[-0.04],"ownershipStdev":[0.03],"policy":[1.4e-05],"rootInfo":{"currentPlayer":"B","rawLead":-0.89,"rawNoResultProb":0.0007,"rawScoreSelfplay":-1.56,"rawScoreSelfplayStdev":14.28,"rawStScoreError":0.51,"rawStWrError":0.03,"rawVarTimeLeft":50.1,"rawWinrate":0.385,"scoreLead":-0.93,"scoreSelfplay":-1.39,"scoreStdev":14.19,"symHash":"3862","thisHash":"6171","utility":-0.24,"visits":4,"weight":9.29,"winrate":0.379},"turnNumber":2}"#;
        let parsed: AnalysisResponse = serde_json::from_str(raw).unwrap();
        assert_eq!(parsed.id, "a1");
        assert_eq!(parsed.turn_number, 2);
        assert_eq!(parsed.move_infos[0].move_coord, "R17");
        assert_eq!(parsed.move_infos[0].pv, vec!["R17", "D4"]);
        assert_eq!(parsed.root_info.as_ref().unwrap().current_player, "B");
        assert_eq!(
            parsed.root_info.as_ref().unwrap().sym_hash.as_deref(),
            Some("3862")
        );
        assert!(!parsed.no_results);

        let out = serde_json::to_value(&parsed).unwrap();
        assert_eq!(out["moveInfos"][0]["moveCoord"], "R17");
        assert!(out["moveInfos"][0].get("move").is_none());
        assert!(
            out.get("noResults").is_none(),
            "noResults omitted when false"
        );
    }

    #[test]
    fn parses_terminated_result() {
        let parsed: AnalysisResponse = serde_json::from_str(
            r#"{"id":"a1","isDuringSearch":false,"noResults":true,"turnNumber":2}"#,
        )
        .unwrap();
        assert!(parsed.no_results);
        assert!(parsed.move_infos.is_empty());
        assert!(parsed.root_info.is_none());
    }
}
