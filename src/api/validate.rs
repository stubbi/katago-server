//! Turns an [`AnalysisRequest`] into a KataGo [`Query`], rejecting anything
//! KataGo would choke on with a precise 400 instead of a vague engine error.

use std::collections::HashSet;

use crate::api::problem::ApiError;
use crate::api::types::{AnalysisRequest, MoveFilter, MoveInput, Rules};
use crate::config::KatagoConfig;
use crate::coords::{self, MAX_BOARD_SIZE, MIN_BOARD_SIZE, Vertex};
use crate::engine::Query;

/// Longest accepted client `requestId`.
pub const MAX_REQUEST_ID_LEN: usize = 128;
/// Komi magnitudes beyond this are almost certainly mistakes.
const MAX_ABS_KOMI: f64 = 150.0;

/// A validated query plus the id to echo back to the client.
#[derive(Debug, Clone)]
pub struct PreparedQuery {
    /// Query to send to KataGo. Its `id` is always a fresh UUID so that two
    /// clients reusing the same `requestId` can never collide.
    pub query: Query,
    /// Identifier to put in the response: the client's `requestId` or the UUID.
    pub client_id: String,
}

/// Validates `request` and builds the KataGo query.
///
/// With `whole_game` set, `analyzeTurns` defaults to every turn of the game;
/// otherwise it defaults to the final position only.
pub fn build_query(
    request: &AnalysisRequest,
    katago: &KatagoConfig,
    whole_game: bool,
) -> Result<PreparedQuery, ApiError> {
    let width = request.board_x_size;
    let height = request.board_y_size;
    for (name, size) in [("boardXSize", width), ("boardYSize", height)] {
        if !(MIN_BOARD_SIZE..=MAX_BOARD_SIZE).contains(&size) {
            return Err(ApiError::invalid_request(format!(
                "{name} must be between {MIN_BOARD_SIZE} and {MAX_BOARD_SIZE}, got {size}"
            ))
            .with_field(name));
        }
    }

    let komi = request.komi.unwrap_or(7.5);
    if !komi.is_finite() || komi.abs() > MAX_ABS_KOMI || (komi * 2.0).fract() != 0.0 {
        return Err(ApiError::invalid_request(format!(
            "komi must be a multiple of 0.5 between -{MAX_ABS_KOMI} and {MAX_ABS_KOMI}, got {komi}"
        ))
        .with_field("komi"));
    }

    let rules = match &request.rules {
        None => Rules::Named(default_rules(komi).to_owned()),
        Some(Rules::Named(name)) => {
            let trimmed = name.trim();
            if trimmed.is_empty() {
                return Err(
                    ApiError::invalid_request("rules must not be empty").with_field("rules")
                );
            }
            Rules::Named(trimmed.to_ascii_lowercase())
        }
        Some(Rules::Custom(map)) => {
            if map.is_empty() {
                return Err(
                    ApiError::invalid_request("rules object must not be empty").with_field("rules")
                );
            }
            Rules::Custom(map.clone())
        }
    };

    let initial_stones = validate_initial_stones(request, width, height)?;
    let initial_player = request
        .initial_player
        .as_deref()
        .map(|p| parse_color(p).ok_or_else(|| bad_color("initialPlayer", p)))
        .transpose()?;
    let moves = validate_moves(
        request,
        width,
        height,
        initial_player,
        !initial_stones.is_empty(),
    )?;

    let analyze_turns = validate_turns(request, moves.len(), whole_game)?;

    let max_visits = validate_visits(request, katago)?;
    validate_tuning(request)?;
    let avoid_moves =
        validate_filters(request.avoid_moves.as_deref(), "avoidMoves", width, height)?;
    let allow_moves =
        validate_filters(request.allow_moves.as_deref(), "allowMoves", width, height)?;

    let internal_id = uuid::Uuid::new_v4().to_string();
    let client_id = match request.request_id.as_deref().map(str::trim) {
        None | Some("") => internal_id.clone(),
        Some(id) if id.len() > MAX_REQUEST_ID_LEN => {
            return Err(ApiError::invalid_request(format!(
                "requestId must be at most {MAX_REQUEST_ID_LEN} characters"
            ))
            .with_field("requestId"));
        }
        Some(id) => id.to_owned(),
    };

    Ok(PreparedQuery {
        query: Query {
            id: internal_id,
            initial_stones,
            moves,
            rules,
            komi,
            board_x_size: width,
            board_y_size: height,
            initial_player: initial_player.map(ToOwned::to_owned),
            analyze_turns,
            max_visits,
            root_policy_temperature: request.root_policy_temperature,
            root_fpu_reduction_max: request.root_fpu_reduction_max,
            analysis_pv_len: request.analysis_pv_len,
            include_ownership: request.include_ownership,
            include_ownership_stdev: request.include_ownership_stdev,
            include_moves_ownership: request.include_moves_ownership,
            include_policy: request.include_policy,
            include_pv_visits: request.include_pv_visits,
            avoid_moves,
            allow_moves,
            override_settings: request.override_settings.clone(),
            priority: request.priority,
        },
        client_id,
    })
}

fn validate_turns(
    request: &AnalysisRequest,
    move_count: usize,
    whole_game: bool,
) -> Result<Option<Vec<u32>>, ApiError> {
    let turn_count = u32::try_from(move_count)
        .map_err(|_| ApiError::invalid_request("too many moves").with_field("moves"))?;
    let all_turns = || (0..=turn_count).collect::<Vec<u32>>();
    let Some(turns) = &request.analyze_turns else {
        return Ok(whole_game.then(all_turns));
    };
    let mut unique: Vec<u32> = Vec::with_capacity(turns.len());
    for &turn in turns {
        if turn > turn_count {
            return Err(ApiError::invalid_request(format!(
                "analyzeTurns contains {turn} but the game has only {turn_count} move(s)"
            ))
            .with_field("analyzeTurns"));
        }
        if !unique.contains(&turn) {
            unique.push(turn);
        }
    }
    if unique.is_empty() {
        return Ok(whole_game.then(all_turns));
    }
    unique.sort_unstable();
    Ok(Some(unique))
}

fn validate_visits(
    request: &AnalysisRequest,
    katago: &KatagoConfig,
) -> Result<Option<u32>, ApiError> {
    let max_visits = match request.max_visits {
        Some(0) => {
            return Err(
                ApiError::invalid_request("maxVisits must be at least 1").with_field("maxVisits")
            );
        }
        Some(v) => Some(v),
        None => katago.default_max_visits,
    };
    let Some(limit) = katago.max_visits_limit else {
        return Ok(max_visits);
    };
    if let Some(v) = max_visits
        && v > limit
    {
        return Err(ApiError::invalid_request(format!(
            "maxVisits {v} exceeds this server's limit of {limit}"
        ))
        .with_field("maxVisits"));
    }
    if let Some(v) = request
        .override_settings
        .as_ref()
        .and_then(|o| o.get("maxVisits"))
        .and_then(serde_json::Value::as_u64)
        && v > u64::from(limit)
    {
        return Err(ApiError::invalid_request(format!(
            "overrideSettings.maxVisits {v} exceeds this server's limit of {limit}"
        ))
        .with_field("overrideSettings"));
    }
    Ok(max_visits)
}

fn validate_tuning(request: &AnalysisRequest) -> Result<(), ApiError> {
    if request.analysis_pv_len == Some(0) {
        return Err(
            ApiError::invalid_request("analysisPVLen must be at least 1")
                .with_field("analysisPVLen"),
        );
    }
    if let Some(t) = request.root_policy_temperature
        && !(t.is_finite() && t > 0.0)
    {
        return Err(
            ApiError::invalid_request("rootPolicyTemperature must be positive")
                .with_field("rootPolicyTemperature"),
        );
    }
    if let Some(r) = request.root_fpu_reduction_max
        && !r.is_finite()
    {
        return Err(
            ApiError::invalid_request("rootFpuReductionMax must be a finite number")
                .with_field("rootFpuReductionMax"),
        );
    }
    Ok(())
}

/// Japanese rules for integer or 6.5 komi, Chinese otherwise.
fn default_rules(komi: f64) -> &'static str {
    if komi.fract() == 0.0 || (komi - 6.5).abs() < f64::EPSILON {
        "japanese"
    } else {
        "chinese"
    }
}

/// Normalises a colour to `"B"` or `"W"`.
pub fn parse_color(text: &str) -> Option<&'static str> {
    match text.trim().to_ascii_lowercase().as_str() {
        "b" | "black" => Some("B"),
        "w" | "white" => Some("W"),
        _ => None,
    }
}

fn bad_color(field: &str, text: &str) -> ApiError {
    ApiError::invalid_request(format!(
        "{field} colour must be \"B\" or \"W\", got {text:?}"
    ))
    .with_field(field)
}

fn other_color(color: &str) -> &'static str {
    if color == "B" { "W" } else { "B" }
}

fn validate_initial_stones(
    request: &AnalysisRequest,
    width: u8,
    height: u8,
) -> Result<Vec<[String; 2]>, ApiError> {
    let Some(stones) = &request.initial_stones else {
        return Ok(Vec::new());
    };
    let mut seen = HashSet::with_capacity(stones.len());
    let mut out = Vec::with_capacity(stones.len());
    for (i, [color, coord]) in stones.iter().enumerate() {
        let color = parse_color(color).ok_or_else(|| bad_color("initialStones", color))?;
        let vertex = coords::parse_vertex(coord, width, height).ok_or_else(|| {
            ApiError::invalid_request(format!(
                "initialStones[{i}] ({coord:?}) is not on a {width}x{height} board"
            ))
            .with_field("initialStones")
        })?;
        if vertex == Vertex::Pass {
            return Err(
                ApiError::invalid_request(format!("initialStones[{i}] cannot be a pass"))
                    .with_field("initialStones"),
            );
        }
        let canonical = coords::format_vertex(vertex);
        if !seen.insert(canonical.clone()) {
            return Err(ApiError::invalid_request(format!(
                "initialStones places two stones on {canonical}"
            ))
            .with_field("initialStones"));
        }
        out.push([color.to_owned(), canonical]);
    }
    Ok(out)
}

fn validate_moves(
    request: &AnalysisRequest,
    width: u8,
    height: u8,
    initial_player: Option<&'static str>,
    has_initial_stones: bool,
) -> Result<Vec<[String; 2]>, ApiError> {
    let explicit = request.moves.iter().filter(|m| m.color().is_some()).count();
    if explicit != 0 && explicit != request.moves.len() {
        return Err(ApiError::invalid_request(
            "moves must be either all bare coordinates or all [colour, coordinate] pairs",
        )
        .with_field("moves"));
    }

    let mut color = initial_player.unwrap_or(if has_initial_stones { "W" } else { "B" });
    let mut out = Vec::with_capacity(request.moves.len());
    for (i, mv) in request.moves.iter().enumerate() {
        let this_color = match mv {
            MoveInput::WithColor([c, _]) => parse_color(c).ok_or_else(|| {
                ApiError::invalid_request(format!(
                    "moves[{i}] colour must be \"B\" or \"W\", got {c:?}"
                ))
                .with_field("moves")
            })?,
            MoveInput::Simple(_) => color,
        };
        let canonical = coords::normalize(mv.coord(), width, height).ok_or_else(|| {
            ApiError::invalid_request(format!(
                "moves[{i}] ({:?}) is not on a {width}x{height} board (columns A-{}, skipping I, rows 1-{height}, or \"pass\")",
                mv.coord(),
                coords::last_column_letter(width).unwrap_or('?'),
            ))
            .with_field("moves")
        })?;
        out.push([this_color.to_owned(), canonical]);
        color = other_color(this_color);
    }
    Ok(out)
}

fn validate_filters(
    filters: Option<&[MoveFilter]>,
    field: &'static str,
    width: u8,
    height: u8,
) -> Result<Option<Vec<MoveFilter>>, ApiError> {
    let Some(filters) = filters else {
        return Ok(None);
    };
    let mut out = Vec::with_capacity(filters.len());
    for (i, filter) in filters.iter().enumerate() {
        let player = parse_color(&filter.player).ok_or_else(|| {
            ApiError::invalid_request(format!(
                "{field}[{i}].player must be \"B\" or \"W\", got {:?}",
                filter.player
            ))
            .with_field(field)
        })?;
        if filter.until_depth == 0 {
            return Err(ApiError::invalid_request(format!(
                "{field}[{i}].untilDepth must be at least 1"
            ))
            .with_field(field));
        }
        let mut moves = Vec::with_capacity(filter.moves.len());
        for coord in &filter.moves {
            let canonical = coords::normalize(coord, width, height).ok_or_else(|| {
                ApiError::invalid_request(format!(
                    "{field}[{i}] contains {coord:?}, which is not on a {width}x{height} board"
                ))
                .with_field(field)
            })?;
            moves.push(canonical);
        }
        out.push(MoveFilter {
            player: player.to_owned(),
            moves,
            until_depth: filter.until_depth,
        });
    }
    Ok(Some(out))
}

#[cfg(test)]
#[allow(clippy::float_cmp)]
mod tests {
    use super::*;

    fn request(json: serde_json::Value) -> AnalysisRequest {
        serde_json::from_value(json).unwrap()
    }

    fn cfg() -> KatagoConfig {
        KatagoConfig::default()
    }

    fn detail(err: &ApiError) -> String {
        err.to_problem().detail
    }

    #[test]
    fn simple_moves_alternate_from_black() {
        let prepared = build_query(
            &request(serde_json::json!({"moves": ["d4", "Q16", "pass"]})),
            &cfg(),
            false,
        )
        .unwrap();
        assert_eq!(
            prepared.query.moves,
            vec![
                ["B".to_owned(), "D4".to_owned()],
                ["W".to_owned(), "Q16".to_owned()],
                ["B".to_owned(), "pass".to_owned()],
            ]
        );
        assert_eq!(prepared.query.komi, 7.5);
        assert!(matches!(prepared.query.rules, Rules::Named(ref r) if r == "chinese"));
        assert_eq!(prepared.query.max_visits, Some(10));
        assert_eq!(prepared.query.analyze_turns, None);
        assert_eq!(prepared.client_id, prepared.query.id);
    }

    #[test]
    fn handicap_defaults_to_white_first() {
        let prepared = build_query(
            &request(serde_json::json!({
                "initialStones": [["b", "D4"], ["B", "Q16"]],
                "moves": ["Q4"],
                "komi": 0.5
            })),
            &cfg(),
            false,
        )
        .unwrap();
        assert_eq!(
            prepared.query.initial_stones[0],
            ["B".to_owned(), "D4".to_owned()]
        );
        assert_eq!(prepared.query.moves[0], ["W".to_owned(), "Q4".to_owned()]);
        assert_eq!(prepared.query.initial_player, None);
    }

    #[test]
    fn initial_player_is_forwarded_and_used() {
        let prepared = build_query(
            &request(serde_json::json!({"initialPlayer": "white", "moves": ["D4", "Q16"]})),
            &cfg(),
            false,
        )
        .unwrap();
        assert_eq!(prepared.query.initial_player.as_deref(), Some("W"));
        assert_eq!(prepared.query.moves[0][0], "W");
        assert_eq!(prepared.query.moves[1][0], "B");
    }

    #[test]
    fn explicit_colors_are_normalised() {
        let prepared = build_query(
            &request(serde_json::json!({"moves": [["w", "D4"], ["Black", "q16"]]})),
            &cfg(),
            false,
        )
        .unwrap();
        assert_eq!(prepared.query.moves[0], ["W".to_owned(), "D4".to_owned()]);
        assert_eq!(prepared.query.moves[1], ["B".to_owned(), "Q16".to_owned()]);
    }

    #[test]
    fn mixed_move_formats_are_rejected_not_panicked() {
        let err = build_query(
            &request(serde_json::json!({"moves": ["D4", ["W", "Q16"]]})),
            &cfg(),
            false,
        )
        .unwrap_err();
        assert_eq!(err.status(), axum::http::StatusCode::BAD_REQUEST);
        assert!(detail(&err).contains("all bare coordinates or all"));
    }

    #[test]
    fn off_board_moves_are_rejected_with_index() {
        let err = build_query(
            &request(serde_json::json!({"moves": ["D4", "Z99"], "boardXSize": 9, "boardYSize": 9})),
            &cfg(),
            false,
        )
        .unwrap_err();
        let d = detail(&err);
        assert!(d.contains("moves[1]"), "{d}");
        assert!(d.contains("A-J"), "{d}");
        assert_eq!(err.to_problem().field.as_deref(), Some("moves"));
    }

    #[test]
    fn bad_colours_and_board_sizes_and_komi() {
        let err = build_query(
            &request(serde_json::json!({"moves": [["X", "D4"]]})),
            &cfg(),
            false,
        )
        .unwrap_err();
        assert!(detail(&err).contains("colour"));

        let err = build_query(
            &request(serde_json::json!({"boardXSize": 1})),
            &cfg(),
            false,
        )
        .unwrap_err();
        assert_eq!(err.to_problem().field.as_deref(), Some("boardXSize"));
        let err = build_query(
            &request(serde_json::json!({"boardYSize": 26})),
            &cfg(),
            false,
        )
        .unwrap_err();
        assert_eq!(err.to_problem().field.as_deref(), Some("boardYSize"));

        for komi in [7.25, 1000.0, -151.0] {
            let err = build_query(&request(serde_json::json!({"komi": komi})), &cfg(), false)
                .unwrap_err();
            assert_eq!(
                err.to_problem().field.as_deref(),
                Some("komi"),
                "komi {komi}"
            );
        }
        assert!(build_query(&request(serde_json::json!({"komi": -3.5})), &cfg(), false).is_ok());
    }

    #[test]
    fn initial_stones_reject_pass_and_duplicates() {
        let err = build_query(
            &request(serde_json::json!({"initialStones": [["B", "pass"]]})),
            &cfg(),
            false,
        )
        .unwrap_err();
        assert!(detail(&err).contains("pass"));
        let err = build_query(
            &request(serde_json::json!({"initialStones": [["B", "D4"], ["W", "d4"]]})),
            &cfg(),
            false,
        )
        .unwrap_err();
        assert!(detail(&err).contains("two stones on D4"));
    }

    #[test]
    fn default_rules_follow_komi() {
        assert_eq!(default_rules(7.5), "chinese");
        assert_eq!(default_rules(6.5), "japanese");
        assert_eq!(default_rules(7.0), "japanese");
        assert_eq!(default_rules(0.5), "chinese");
        let prepared = build_query(
            &request(serde_json::json!({"rules": " Japanese "})),
            &cfg(),
            false,
        )
        .unwrap();
        assert!(matches!(prepared.query.rules, Rules::Named(ref r) if r == "japanese"));
        let prepared = build_query(
            &request(serde_json::json!({"rules": {"ko": "SIMPLE", "scoring": "AREA"}})),
            &cfg(),
            false,
        )
        .unwrap();
        assert!(matches!(prepared.query.rules, Rules::Custom(_)));
        assert!(build_query(&request(serde_json::json!({"rules": ""})), &cfg(), false).is_err());
        assert!(build_query(&request(serde_json::json!({"rules": {}})), &cfg(), false).is_err());
    }

    #[test]
    fn analyze_turns_are_validated_and_deduplicated() {
        let prepared = build_query(
            &request(serde_json::json!({"moves": ["D4", "Q16"], "analyzeTurns": [2, 0, 2]})),
            &cfg(),
            false,
        )
        .unwrap();
        assert_eq!(prepared.query.analyze_turns, Some(vec![0, 2]));
        assert_eq!(prepared.query.expected_results(), 2);

        let err = build_query(
            &request(serde_json::json!({"moves": ["D4"], "analyzeTurns": [5]})),
            &cfg(),
            false,
        )
        .unwrap_err();
        assert_eq!(err.to_problem().field.as_deref(), Some("analyzeTurns"));
    }

    #[test]
    fn whole_game_defaults_to_every_turn() {
        let prepared = build_query(
            &request(serde_json::json!({"moves": ["D4", "Q16", "R4"]})),
            &cfg(),
            true,
        )
        .unwrap();
        assert_eq!(prepared.query.analyze_turns, Some(vec![0, 1, 2, 3]));
        let prepared = build_query(
            &request(serde_json::json!({"moves": ["D4"], "analyzeTurns": []})),
            &cfg(),
            true,
        )
        .unwrap();
        assert_eq!(prepared.query.analyze_turns, Some(vec![0, 1]));
        let prepared = build_query(
            &request(serde_json::json!({"moves": ["D4"], "analyzeTurns": []})),
            &cfg(),
            false,
        )
        .unwrap();
        assert_eq!(prepared.query.analyze_turns, None);
    }

    #[test]
    fn visit_limits_are_enforced() {
        let mut katago = cfg();
        katago.max_visits_limit = Some(100);
        assert!(
            build_query(
                &request(serde_json::json!({"maxVisits": 100})),
                &katago,
                false
            )
            .is_ok()
        );
        let err = build_query(
            &request(serde_json::json!({"maxVisits": 101})),
            &katago,
            false,
        )
        .unwrap_err();
        assert!(detail(&err).contains("limit of 100"));
        let err = build_query(
            &request(serde_json::json!({"overrideSettings": {"maxVisits": 5000}})),
            &katago,
            false,
        )
        .unwrap_err();
        assert_eq!(err.to_problem().field.as_deref(), Some("overrideSettings"));
        let err = build_query(
            &request(serde_json::json!({"maxVisits": 0})),
            &katago,
            false,
        )
        .unwrap_err();
        assert_eq!(err.to_problem().field.as_deref(), Some("maxVisits"));

        katago.default_max_visits = None;
        let prepared = build_query(&request(serde_json::json!({})), &katago, false).unwrap();
        assert_eq!(prepared.query.max_visits, None);
    }

    #[test]
    fn filters_are_validated_and_normalised() {
        let prepared = build_query(
            &request(serde_json::json!({
                "avoidMoves": [{"player": "b", "moves": ["c3", "pass"], "untilDepth": 2}],
                "allowMoves": [{"player": "W", "moves": ["q16"], "untilDepth": 1}]
            })),
            &cfg(),
            false,
        )
        .unwrap();
        let avoid = prepared.query.avoid_moves.unwrap();
        assert_eq!(avoid[0].player, "B");
        assert_eq!(avoid[0].moves, vec!["C3", "pass"]);
        assert_eq!(prepared.query.allow_moves.unwrap()[0].moves, vec!["Q16"]);

        let err = build_query(
            &request(serde_json::json!({"avoidMoves": [{"player": "B", "moves": ["Z1"], "untilDepth": 1}]})),
            &cfg(),
            false,
        )
        .unwrap_err();
        assert_eq!(err.to_problem().field.as_deref(), Some("avoidMoves"));
        let err = build_query(
            &request(serde_json::json!({"allowMoves": [{"player": "B", "moves": ["D4"], "untilDepth": 0}]})),
            &cfg(),
            false,
        )
        .unwrap_err();
        assert!(detail(&err).contains("untilDepth"));
    }

    #[test]
    fn request_id_is_echoed_but_never_used_internally() {
        let prepared = build_query(
            &request(serde_json::json!({"requestId": " game-7 "})),
            &cfg(),
            false,
        )
        .unwrap();
        assert_eq!(prepared.client_id, "game-7");
        assert_ne!(prepared.query.id, "game-7");
        let long = "x".repeat(MAX_REQUEST_ID_LEN + 1);
        let err = build_query(
            &request(serde_json::json!({"requestId": long})),
            &cfg(),
            false,
        )
        .unwrap_err();
        assert_eq!(err.to_problem().field.as_deref(), Some("requestId"));
    }

    #[test]
    fn numeric_tuning_parameters_are_checked() {
        assert!(
            build_query(
                &request(serde_json::json!({"analysisPVLen": 0})),
                &cfg(),
                false
            )
            .is_err()
        );
        assert!(
            build_query(
                &request(serde_json::json!({"rootPolicyTemperature": 0})),
                &cfg(),
                false
            )
            .is_err()
        );
        assert!(build_query(&request(serde_json::json!({"rootPolicyTemperature": 1.2, "rootFpuReductionMax": 0.1, "analysisPVLen": 5, "priority": -3})), &cfg(), false).is_ok());
    }
}
