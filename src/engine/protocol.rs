//! Wire format of queries sent to the KataGo analysis engine.
//!
//! See <https://github.com/lightvector/KataGo/blob/master/docs/Analysis_Engine.md>.

use serde::Serialize;

use crate::api::types::{MoveFilter, Rules};

/// An analysis query in KataGo's JSON format.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Query {
    /// Unique per query; every response line echoes it.
    pub id: String,
    /// Stones placed before the first move as `[colour, coordinate]`.
    pub initial_stones: Vec<[String; 2]>,
    /// Moves as `[colour, coordinate]`.
    pub moves: Vec<[String; 2]>,
    /// Rule set name or object.
    pub rules: Rules,
    /// Komi.
    pub komi: f64,
    /// Board width.
    pub board_x_size: u8,
    /// Board height.
    pub board_y_size: u8,
    /// Player to move at turn 0.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub initial_player: Option<String>,
    /// Turns to analyse; `None` means the final position only.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub analyze_turns: Option<Vec<u32>>,
    /// Visit limit.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub max_visits: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub root_policy_temperature: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub root_fpu_reduction_max: Option<f64>,
    #[serde(rename = "analysisPVLen", skip_serializing_if = "Option::is_none")]
    pub analysis_pv_len: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub include_ownership: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub include_ownership_stdev: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub include_moves_ownership: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub include_policy: Option<bool>,
    #[serde(rename = "includePVVisits", skip_serializing_if = "Option::is_none")]
    pub include_pv_visits: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub avoid_moves: Option<Vec<MoveFilter>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub allow_moves: Option<Vec<MoveFilter>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub override_settings: Option<serde_json::Map<String, serde_json::Value>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub priority: Option<i64>,
}

impl Query {
    /// Number of result lines KataGo will emit for this query.
    pub fn expected_results(&self) -> usize {
        self.analyze_turns
            .as_ref()
            .map_or(1, |turns| turns.len().max(1))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn serialises_in_katago_shape_and_omits_unset_fields() {
        let query = Query {
            id: "q".into(),
            initial_stones: vec![],
            moves: vec![["B".into(), "D4".into()]],
            rules: Rules::Named("chinese".into()),
            komi: 7.5,
            board_x_size: 19,
            board_y_size: 19,
            initial_player: None,
            analyze_turns: None,
            max_visits: Some(10),
            root_policy_temperature: None,
            root_fpu_reduction_max: None,
            analysis_pv_len: Some(3),
            include_ownership: Some(true),
            include_ownership_stdev: None,
            include_moves_ownership: None,
            include_policy: None,
            include_pv_visits: Some(true),
            avoid_moves: None,
            allow_moves: None,
            override_settings: None,
            priority: None,
        };
        let json = serde_json::to_value(&query).unwrap();
        assert_eq!(json["moves"][0][1], "D4");
        assert_eq!(json["boardXSize"], 19);
        assert_eq!(json["includeOwnership"], true);
        assert!(json.get("analyzeTurns").is_none());
        assert_eq!(json["analysisPVLen"], 3);
        assert_eq!(json["includePVVisits"], true);
        assert!(json.get("analysisPvLen").is_none());
        assert!(json.get("rootPolicyTemperature").is_none());
        assert_eq!(query.expected_results(), 1);
    }

    #[test]
    fn expected_results_counts_turns() {
        let mut query = Query {
            id: "q".into(),
            initial_stones: vec![],
            moves: vec![],
            rules: Rules::Named("chinese".into()),
            komi: 7.5,
            board_x_size: 9,
            board_y_size: 9,
            initial_player: None,
            analyze_turns: Some(vec![0, 1, 2]),
            max_visits: None,
            root_policy_temperature: None,
            root_fpu_reduction_max: None,
            analysis_pv_len: None,
            include_ownership: None,
            include_ownership_stdev: None,
            include_moves_ownership: None,
            include_policy: None,
            include_pv_visits: None,
            avoid_moves: None,
            allow_moves: None,
            override_settings: None,
            priority: None,
        };
        assert_eq!(query.expected_results(), 3);
        query.analyze_turns = Some(vec![]);
        assert_eq!(query.expected_results(), 1);
    }
}
