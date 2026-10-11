use serde::Serialize;
use serde_json::{Value, json};

use crate::render::{PreparedCamera, prepare_save_frame, prepared_camera_view};
use crate::{
    CityCommand, CityCommandOutcome, CityQuery, CityQueryResult, CitySave, CityScenario,
    PreparedCityRender, RenderPreparationWork, RenderView, build_save_render_frame,
    build_save_render_frame_with_view,
};

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct ErrorEnvelope<'a> {
    ok: bool,
    error: &'a str,
}

pub(crate) fn save_from_scenario_json(scenario_json: &str) -> Result<CitySave, String> {
    let scenario: CityScenario =
        serde_json::from_str(scenario_json).map_err(|error| error.to_string())?;
    CitySave::new(scenario).map_err(|error| error.to_string())
}

fn parse_command(command_json: &str) -> Result<CityCommand, String> {
    serde_json::from_str(command_json).map_err(|error| error.to_string())
}

fn execute_outcome(save: &mut CitySave, command_json: &str) -> Result<CityCommandOutcome, String> {
    save.execute(parse_command(command_json)?)
        .map_err(|error| error.to_string())
}

/// One retained browser session: the authoritative live save plus disposable render state.
///
/// Commands go through the same validated gateway as [`CitySave::execute`]; their explicit render
/// impact incrementally updates the retained [`PreparedCityRender`]. The render state is derived
/// only and is dropped (then fully re-prepared) whenever it cannot be updated.
pub(crate) struct CitySession {
    save: CitySave,
    render: Option<PreparedCityRender>,
    /// Render work performed since the last preparation response, reported with that response.
    pending_work: RenderPreparationWork,
}

impl CitySession {
    pub(crate) fn from_scenario_json(scenario_json: &str) -> Result<Self, String> {
        Ok(Self {
            save: save_from_scenario_json(scenario_json)?,
            render: None,
            pending_work: RenderPreparationWork::default(),
        })
    }

    pub(crate) fn execute_json(&mut self, command_json: &str) -> String {
        encode_result((|| {
            let command = parse_command(command_json)?;
            let (outcome, impact) = self
                .save
                .execute_with_render_impact(command)
                .map_err(|error| error.to_string())?;
            if let Some(render) = &mut self.render {
                match render.apply(&self.save, &impact) {
                    Ok(()) => self.pending_work.accumulate(render.last_work()),
                    // Derived state only: drop it and fully re-prepare on the next request.
                    Err(_) => self.render = None,
                }
            }
            Ok(json!({ "ok": true, "outcome": outcome }))
        })())
    }

    pub(crate) fn query_json(&self, query_json: &str) -> String {
        encode_result(
            query_result(&self.save, query_json)
                .map(|result| json!({ "ok": true, "result": result })),
        )
    }

    pub(crate) fn prepare_render_json(&mut self, aspect: f32) -> String {
        let prepared = match self.render.take() {
            Some(mut render) => match render.set_aspect(aspect) {
                Ok(()) => Ok(render),
                Err(error) => {
                    // An invalid aspect leaves the retained state untouched.
                    self.render = Some(render);
                    Err(error)
                }
            },
            None => PreparedCityRender::prepare(&self.save, aspect),
        };
        let render = match prepared {
            Ok(render) => self.render.insert(render),
            Err(error) => return encode_result(Err(error.to_string())),
        };
        self.pending_work.accumulate(render.last_work());
        // Same `serde_json::Value` encoding as the stateless reference ABI, so numbers are
        // byte-identical to the full-frame oracle on the browser side.
        let work = std::mem::take(&mut self.pending_work);
        encode_result(Ok(json!({
            "ok": true,
            "frame": render.frame(),
            "overview": PreparedCamera::from_camera(render.overview_camera(), render.aspect()),
            "work": work,
        })))
    }
}

fn query_result(save: &CitySave, query_json: &str) -> Result<CityQueryResult, String> {
    let query: CityQuery = serde_json::from_str(query_json).map_err(|error| error.to_string())?;
    save.query(query).map_err(|error| error.to_string())
}

pub(crate) fn new_save_json(scenario_json: &str) -> String {
    encode_result(
        save_from_scenario_json(scenario_json).map(|save| json!({ "ok": true, "save": save })),
    )
}

pub(crate) fn execute_json(save_json: &str, command_json: &str) -> String {
    encode_result((|| {
        let mut save: CitySave =
            serde_json::from_str(save_json).map_err(|error| error.to_string())?;
        let outcome = execute_outcome(&mut save, command_json)?;
        Ok(json!({ "ok": true, "save": save, "outcome": outcome }))
    })())
}

pub(crate) fn query_json(save_json: &str, query_json: &str) -> String {
    encode_result((|| {
        let save: CitySave = serde_json::from_str(save_json).map_err(|error| error.to_string())?;
        let result = query_result(&save, query_json)?;
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

pub(crate) fn render_frame_with_view_json(save_json: &str, view_json: &str, aspect: f32) -> String {
    encode_result((|| {
        let save: CitySave = serde_json::from_str(save_json).map_err(|error| error.to_string())?;
        let view: RenderView =
            serde_json::from_str(view_json).map_err(|error| error.to_string())?;
        let frame = build_save_render_frame_with_view(&save, aspect, view)
            .map_err(|error| error.to_string())?;
        Ok(json!({ "ok": true, "frame": frame }))
    })())
}

pub(crate) fn prepare_render_json(save_json: &str, aspect: f32) -> String {
    encode_result((|| {
        let save: CitySave = serde_json::from_str(save_json).map_err(|error| error.to_string())?;
        prepare_live_render_value(&save, aspect)
    })())
}

fn prepare_live_render_value(save: &CitySave, aspect: f32) -> Result<Value, String> {
    let prepared = prepare_save_frame(save, aspect).map_err(|error| error.to_string())?;
    Ok(json!({ "ok": true, "frame": prepared.frame, "overview": prepared.overview }))
}

pub(crate) fn render_camera_json(overview_json: &str, view_json: &str) -> String {
    encode_result((|| {
        let overview: PreparedCamera =
            serde_json::from_str(overview_json).map_err(|error| error.to_string())?;
        let view: RenderView =
            serde_json::from_str(view_json).map_err(|error| error.to_string())?;
        let camera = prepared_camera_view(&overview, view).map_err(|error| error.to_string())?;
        Ok(json!({ "ok": true, "camera": camera }))
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
        CityCommand, CityQuery, CitySave, CityScenario, ExternalRevision, SAVE_SCHEMA_VERSION,
        SCENARIO_SCHEMA_VERSION, ScenarioProvenance,
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
    fn browser_transport_creates_current_save_from_canonical_scenario() {
        let scenario_json = serde_json::to_string(&scenario()).unwrap();

        let response: Value = serde_json::from_str(&new_save_json(&scenario_json)).unwrap();

        assert_eq!(response["ok"], true);
        assert_eq!(response["save"]["schemaVersion"], SAVE_SCHEMA_VERSION);
        assert_eq!(
            response["save"]["scenario"]["schemaVersion"],
            SCENARIO_SCHEMA_VERSION
        );
    }

    #[test]
    fn browser_transport_executes_the_same_application_command_contract() {
        let mut save = CitySave::new(scenario()).unwrap();
        save.world.tick = 4;
        let save_json = serde_json::to_string(&save).unwrap();
        let command_json = serde_json::to_string(&CityCommand::Restart).unwrap();

        let response: Value =
            serde_json::from_str(&execute_json(&save_json, &command_json)).unwrap();

        assert_eq!(response["ok"], true);
        assert_eq!(response["save"]["world"]["tick"], 0);
        assert_eq!(response["outcome"]["kind"], "restarted");
    }

    #[test]
    fn live_transport_keeps_save_owned_and_returns_only_outcomes_or_projections() {
        let scenario_json = serde_json::to_string(&scenario()).unwrap();
        let mut session = CitySession::from_scenario_json(&scenario_json).unwrap();
        session.save.world.tick = 4;

        let command_json = serde_json::to_string(&CityCommand::Restart).unwrap();
        let command: Value = serde_json::from_str(&session.execute_json(&command_json)).unwrap();
        assert_eq!(command["ok"], true);
        assert_eq!(command["outcome"]["kind"], "restarted");
        assert!(command.get("save").is_none());
        assert_eq!(session.save.world.tick, 0);

        let query_json = serde_json::to_string(&CityQuery::TimePosition).unwrap();
        let query: Value = serde_json::from_str(&session.query_json(&query_json)).unwrap();
        assert_eq!(query["ok"], true);
        assert_eq!(query["result"]["kind"], "timePosition");
        assert!(query.get("save").is_none());

        let prepared: Value = serde_json::from_str(&session.prepare_render_json(1.0)).unwrap();
        assert_eq!(prepared["ok"], true);
        assert!(prepared["frame"].is_object());
        assert!(prepared.get("save").is_none());
    }

    fn grid_scenario() -> CityScenario {
        let mut scenario = scenario();
        let square = |lon: f64, lat: f64| geo_core::Geometry::Polygon {
            coordinates: vec![vec![
                [lon, lat],
                [lon + 0.0002, lat],
                [lon + 0.0002, lat + 0.0002],
                [lon, lat + 0.0002],
                [lon, lat],
            ]],
        };
        for column in 0..6 {
            for row in 0..6 {
                scenario.buildings.push(crate::ScenarioBuilding {
                    id: format!("imported/building/{column}-{row}"),
                    source_id: format!("way/{column}-{row}"),
                    footprint: square(8.4 + column as f64 * 0.0005, 49.0 + row as f64 * 0.0005),
                    use_kind: crate::BuildingUse::Residential,
                    name: None,
                    levels: Some(3),
                    height_m: None,
                    gross_floor_area_m2: 1_000,
                });
            }
        }
        scenario
    }

    fn planning(command: Value) -> String {
        json!({ "kind": "planning", "command": command }).to_string()
    }

    fn assert_prepared_matches_reference(session: &mut CitySession, aspect: f32) -> Value {
        let prepared: Value = serde_json::from_str(&session.prepare_render_json(aspect)).unwrap();
        assert_eq!(prepared["ok"], true, "{prepared}");
        // Both sides use the transport's `serde_json::Value` number encoding.
        // (then the same text parser, which is not round-trip exact for every f64).
        let reference = json!(build_save_render_frame(&session.save, aspect).unwrap());
        let reference: Value = serde_json::from_str(&reference.to_string()).unwrap();
        assert_eq!(prepared["frame"], reference);
        let view = r#"{"panX":0.25,"panY":-0.125,"zoom":2.0}"#;
        let camera: Value =
            serde_json::from_str(&render_camera_json(&prepared["overview"].to_string(), view))
                .unwrap();
        let save_json = serde_json::to_string(&session.save).unwrap();
        let full: Value =
            serde_json::from_str(&render_frame_with_view_json(&save_json, view, aspect)).unwrap();
        assert_eq!(camera["camera"], full["frame"]["camera"]);
        prepared["work"].clone()
    }

    #[test]
    fn session_routes_commands_through_incremental_render_preparation() {
        let scenario_json = serde_json::to_string(&grid_scenario()).unwrap();
        let mut session = CitySession::from_scenario_json(&scenario_json).unwrap();

        let work = assert_prepared_matches_reference(&mut session, 1.0);
        assert_eq!(work["fullRebuilds"], 1);
        assert_eq!(work["entitiesVisited"], 36);

        // Interior suppression: the retained scene is updated, not rebuilt.
        let suppress = planning(json!({
            "kind": "suppressScenarioEntity", "id": "imported/building/2-2"
        }));
        assert_eq!(
            serde_json::from_str::<Value>(&session.execute_json(&suppress)).unwrap()["outcome"]["outcome"],
            "applied"
        );
        let work = assert_prepared_matches_reference(&mut session, 1.0);
        assert_eq!(
            work,
            json!({
                "fullRebuilds": 0, "entitiesVisited": 1, "nodesBuilt": 0,
                "nodesRemoved": 1, "fullSorts": 0, "overviewRefits": 0
            })
        );

        // A zone inside the city is one derived node.
        let zone = planning(json!({
            "kind": "zoneArea",
            "zone": { "id": "player/zone/session", "kind": "residential", "geometry": {
                "type": "Polygon",
                "coordinates": [[[8.4011, 49.0011], [8.4013, 49.0011], [8.4013, 49.0013], [8.4011, 49.0011]]]
            } }
        }));
        session.execute_json(&zone);
        let work = assert_prepared_matches_reference(&mut session, 1.0);
        assert_eq!(work["fullRebuilds"], 0);
        assert_eq!(work["nodesBuilt"], 1);
        assert_eq!(work["fullSorts"], 0);

        // Authoritative no-op and rejection do no render work.
        session.execute_json(&suppress);
        let rejected: Value = serde_json::from_str(&session.execute_json(&planning(json!({
            "kind": "suppressScenarioEntity", "id": "imported/building/missing"
        }))))
        .unwrap();
        assert_eq!(rejected["ok"], false);
        let work = assert_prepared_matches_reference(&mut session, 1.0);
        assert_eq!(
            work,
            serde_json::to_value(RenderPreparationWork::default()).unwrap()
        );

        // An aspect change refits the overview only.
        let work = assert_prepared_matches_reference(&mut session, 16.0 / 9.0);
        assert_eq!(work["fullRebuilds"], 0);
        assert_eq!(work["overviewRefits"], 1);
        assert_eq!(work["nodesBuilt"], 0);

        // An invalid aspect fails closed and keeps the retained state.
        let invalid: Value = serde_json::from_str(&session.prepare_render_json(0.0)).unwrap();
        assert_eq!(invalid["ok"], false);
        assert_prepared_matches_reference(&mut session, 16.0 / 9.0);

        // Restart drops exactly the overlay: the suppressed building and the zone.
        session.execute_json(&serde_json::to_string(&CityCommand::Restart).unwrap());
        let work = assert_prepared_matches_reference(&mut session, 16.0 / 9.0);
        assert_eq!(work["fullRebuilds"], 0);
        assert_eq!(work["entitiesVisited"], 2);
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
    fn browser_transport_renders_from_authoritative_save_state() {
        let save = CitySave::new(scenario()).unwrap();
        let save_json = serde_json::to_string(&save).unwrap();

        let response: Value =
            serde_json::from_str(&render_frame_json(&save_json, 16.0 / 9.0)).unwrap();
        let aspect = response["frame"]["camera"]["aspect"].as_f64().unwrap();

        assert_eq!(response["ok"], true);
        assert!((aspect - f64::from(16.0_f32 / 9.0_f32)).abs() <= f64::EPSILON);
        assert!(response["frame"]["nodes"].as_array().unwrap().is_empty());
    }

    #[test]
    fn browser_transport_applies_read_only_inspection_view() {
        let save = CitySave::new(scenario()).unwrap();
        let save_json = serde_json::to_string(&save).unwrap();
        let view_json = r#"{"panX":0.25,"panY":-0.125,"zoom":2.0}"#;

        let response: Value = serde_json::from_str(&render_frame_with_view_json(
            &save_json,
            view_json,
            16.0 / 9.0,
        ))
        .unwrap();

        assert_eq!(response["ok"], true);
        assert!(response["frame"]["camera"]["projectionMatrix"].is_array());
    }

    #[test]
    fn browser_transport_rejects_invalid_inspection_view() {
        let save = CitySave::new(scenario()).unwrap();
        let save_json = serde_json::to_string(&save).unwrap();
        let response: Value = serde_json::from_str(&render_frame_with_view_json(
            &save_json,
            r#"{"panX":0.0,"panY":0.0,"zoom":0.0}"#,
            1.0,
        ))
        .unwrap();

        assert_eq!(response["ok"], false);
        assert!(response["error"].as_str().unwrap().contains("zoom"));
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

    #[test]
    fn prepared_transport_returns_camera_only_and_matches_full_frame() {
        let save_json = serde_json::to_string(&CitySave::new(scenario()).unwrap()).unwrap();
        let prepared: Value = serde_json::from_str(&prepare_render_json(&save_json, 1.0)).unwrap();
        assert_eq!(prepared["ok"], true);
        let view = r#"{"panX":0.25,"panY":-0.125,"zoom":2.0}"#;
        let overview = serde_json::to_string(&prepared["overview"]).unwrap();
        let camera: Value = serde_json::from_str(&render_camera_json(&overview, view)).unwrap();
        let full: Value =
            serde_json::from_str(&render_frame_with_view_json(&save_json, view, 1.0)).unwrap();
        assert_eq!(camera["ok"], true);
        assert_eq!(camera["camera"], full["frame"]["camera"]);
        assert_eq!(camera.as_object().unwrap().len(), 2);
        assert!(camera.get("save").is_none());
        assert!(camera.get("nodes").is_none());
        let invalid: Value = serde_json::from_str(&render_camera_json("{}", view)).unwrap();
        assert_eq!(invalid["ok"], false);
    }
}
