use serde::Serialize;
use serde_json::{Value, json};

use crate::{CityCommand, CityQuery, CitySave, CityScenario, build_save_render_frame};

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct ErrorEnvelope<'a> {
    ok: bool,
    error: &'a str,
}

pub(crate) fn new_save_json(scenario_json: &str) -> String {
    encode_result((|| {
        let scenario: CityScenario =
            serde_json::from_str(scenario_json).map_err(|error| error.to_string())?;
        let save = CitySave::new(scenario).map_err(|error| error.to_string())?;
        Ok(json!({ "ok": true, "save": save }))
    })())
}

pub(crate) fn execute_json(save_json: &str, command_json: &str) -> String {
    encode_result((|| {
        let mut save: CitySave =
            serde_json::from_str(save_json).map_err(|error| error.to_string())?;
        let command: CityCommand =
            serde_json::from_str(command_json).map_err(|error| error.to_string())?;
        let outcome = save.execute(command).map_err(|error| error.to_string())?;
        Ok(json!({ "ok": true, "save": save, "outcome": outcome }))
    })())
}

pub(crate) fn query_json(save_json: &str, query_json: &str) -> String {
    encode_result((|| {
        let save: CitySave = serde_json::from_str(save_json).map_err(|error| error.to_string())?;
        let query: CityQuery =
            serde_json::from_str(query_json).map_err(|error| error.to_string())?;
        let result = save.query(query).map_err(|error| error.to_string())?;
        Ok(json!({ "ok": true, "result": result }))
    })())
}

pub(crate) fn render_frame_json(save_json: &str, aspect: f32) -> String {
    encode_result((|| {
        let save: CitySave = serde_json::from_str(save_json).map_err(|error| error.to_string())?;
        let frame = build_save_render_frame(&save, aspect).map_err(|error| error.to_string())?;
        Ok(json!({ "ok": true, "frame": frame }))
    })())
}

fn encode_result(result: Result<Value, String>) -> String {
    match result {
        Ok(value) => serde_json::to_string(&value)
            .expect("browser transport success envelopes are serializable"),
        Err(error) => serde_json::to_string(&ErrorEnvelope {
            ok: false,
            error: &error,
        })
        .expect("browser transport error envelopes are serializable"),
    }
}

#[cfg(test)]
mod tests {
    use crate::{
        CityCommand, CityQuery, CitySave, CityScenario, ExternalRevision, SCENARIO_SCHEMA_VERSION,
        ScenarioProvenance,
    };

    use super::*;

    fn scenario() -> CityScenario {
        CityScenario {
            schema_version: SCENARIO_SCHEMA_VERSION,
            provenance: ScenarioProvenance {
                source_format: "fixture".to_owned(),
                source_name: "browser".to_owned(),
                source_sha256: "browser".to_owned(),
                parser: ExternalRevision {
                    repository: "fixture".to_owned(),
                    revision: "fixture".to_owned(),
                },
            },
            roads: Vec::new(),
            buildings: Vec::new(),
            water: Vec::new(),
            land_use_areas: Vec::new(),
            transit_anchors: Vec::new(),
        }
    }

    #[test]
    fn browser_transport_executes_the_same_command_contract() {
        let save = CitySave::new(scenario()).unwrap();
        let save_json = serde_json::to_string(&save).unwrap();
        let command_json =
            serde_json::to_string(&CityCommand::AdvanceFixedSteps { steps: 4 }).unwrap();

        let response: Value =
            serde_json::from_str(&execute_json(&save_json, &command_json)).unwrap();

        assert_eq!(response["ok"], true);
        assert_eq!(response["save"]["world"]["tick"], 4);
        assert_eq!(response["outcome"]["kind"], "advanced");
    }

    #[test]
    fn browser_transport_queries_the_same_read_contract() {
        let save = CitySave::new(scenario()).unwrap();
        let save_json = serde_json::to_string(&save).unwrap();
        let query_request = serde_json::to_string(&CityQuery::TimePosition).unwrap();

        let response: Value =
            serde_json::from_str(&query_json(&save_json, &query_request)).unwrap();

        assert_eq!(response["ok"], true);
        assert_eq!(response["result"]["kind"], "timePosition");
        assert_eq!(response["result"]["position"]["tick"], 0);
    }

    #[test]
    fn browser_transport_fails_closed_on_invalid_current_save() {
        let response: Value = serde_json::from_str(&query_json(
            r#"{"schemaVersion":5}"#,
            r#"{"kind":"timePosition"}"#,
        ))
        .unwrap();

        assert_eq!(response["ok"], false);
        assert!(
            response["error"]
                .as_str()
                .unwrap()
                .contains("missing field")
        );
    }
}
