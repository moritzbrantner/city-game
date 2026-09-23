use std::fmt;

#[cfg(test)]
use std::cell::Cell;

use serde::{Deserialize, Serialize};

use crate::{CityRuleset, CitySave, CitySaveError, CityScenario, CityWorld};

pub const MINUTES_PER_DAY: u16 = 24 * 60;
pub const DEFAULT_MINUTES_PER_TICK: u16 = 15;

#[cfg(test)]
std::thread_local! {
    static TICK_WRITES: Cell<u64> = const { Cell::new(0) };
}

#[cfg(test)]
fn reset_tick_writes() {
    TICK_WRITES.with(|writes| writes.set(0));
}

#[cfg(test)]
fn tick_writes() -> u64 {
    TICK_WRITES.with(Cell::get)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CityTimeConfig {
    minutes_per_tick: u16,
}

impl CityTimeConfig {
    pub fn new(minutes_per_tick: u16) -> Result<Self, CityTimeError> {
        let config = Self { minutes_per_tick };
        config.validate()?;
        Ok(config)
    }

    #[must_use]
    pub const fn minutes_per_tick(self) -> u16 {
        self.minutes_per_tick
    }

    pub fn validate(self) -> Result<(), CityTimeError> {
        if self.minutes_per_tick == 0 || !MINUTES_PER_DAY.is_multiple_of(self.minutes_per_tick) {
            return Err(CityTimeError::InvalidMinutesPerTick(self.minutes_per_tick));
        }
        Ok(())
    }

    pub fn position(self, tick: u64) -> Result<CityTimePosition, CityTimeError> {
        self.validate()?;
        Ok(self.position_validated(tick))
    }

    fn position_validated(self, tick: u64) -> CityTimePosition {
        let ticks_per_day = u64::from(MINUTES_PER_DAY / self.minutes_per_tick);
        let tick_in_day = tick % ticks_per_day;
        let minute_of_day =
            u16::try_from(tick_in_day).expect("tick within a day fits u16") * self.minutes_per_tick;

        CityTimePosition {
            tick,
            day: tick / ticks_per_day,
            minute_of_day,
        }
    }
}

impl Default for CityTimeConfig {
    fn default() -> Self {
        Self {
            minutes_per_tick: DEFAULT_MINUTES_PER_TICK,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CityTimePosition {
    pub tick: u64,
    pub day: u64,
    pub minute_of_day: u16,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CityTimeError {
    InvalidMinutesPerTick(u16),
    TickOverflow,
}

impl fmt::Display for CityTimeError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidMinutesPerTick(value) => write!(
                formatter,
                "city minutes per tick must be a non-zero divisor of {MINUTES_PER_DAY}, got {value}"
            ),
            Self::TickOverflow => formatter.write_str("city simulation tick overflow"),
        }
    }
}

impl std::error::Error for CityTimeError {}

impl CityWorld {
    fn commit_tick(&mut self, tick: u64) {
        #[cfg(test)]
        TICK_WRITES.with(|writes| writes.set(writes.get() + 1));

        self.tick = tick;
    }

    fn advance_fixed_steps(
        &mut self,
        time: CityTimeConfig,
        steps: u64,
    ) -> Result<CityTimePosition, CityTimeError> {
        time.validate()?;
        let final_tick = self
            .tick
            .checked_add(steps)
            .ok_or(CityTimeError::TickOverflow)?;
        let final_position = time.position_validated(final_tick);

        if steps != 0 {
            self.commit_tick(final_tick);
        }

        Ok(final_position)
    }
}

impl CitySave {
    pub fn new_with_time_config(
        scenario: CityScenario,
        time: CityTimeConfig,
    ) -> Result<Self, CitySaveError> {
        Self::new_with_config(scenario, time, CityRuleset::default())
    }

    pub fn time_position(&self) -> Result<CityTimePosition, CityTimeError> {
        self.time.position(self.world.tick)
    }

    pub fn advance_tick(&mut self) -> Result<CityTimePosition, CitySaveError> {
        self.advance_fixed_steps(1)
    }

    pub fn advance_fixed_steps(&mut self, steps: u64) -> Result<CityTimePosition, CitySaveError> {
        // Scenario, time, rules, and initial population capacity are validated when a save is
        // constructed or deserialized. Current planning commands can only suppress/restore that
        // already-validated building stock, so rescanning immutable buildings on every clock
        // advance adds work without establishing a new invariant. Future systems that can create
        // developed capacity must validate that mutation at their own authoritative boundary.
        Ok(self.world.advance_fixed_steps(self.time, steps)?)
    }
}

#[cfg(test)]
mod tests {
    use geo_core::Geometry;

    use crate::{
        BuildingUse, ExternalRevision, PopulationError, PopulationRules, RuleSystem, RuleStatus,
        SCENARIO_SCHEMA_VERSION, ScenarioBuilding, ScenarioProvenance,
        population::{capacity_building_visits, reset_capacity_building_visits},
    };

    use super::*;

    fn scenario() -> CityScenario {
        CityScenario {
            schema_version: SCENARIO_SCHEMA_VERSION,
            provenance: ScenarioProvenance {
                source_format: "game-fixture".to_owned(),
                source_name: "clock".to_owned(),
                source_sha256: "clock".to_owned(),
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

    fn residential_building(id: &str, gross_floor_area_m2: u64) -> ScenarioBuilding {
        ScenarioBuilding {
            id: id.to_owned(),
            source_id: id.to_owned(),
            footprint: Geometry::Polygon {
                coordinates: vec![vec![
                    [8.0, 48.0],
                    [8.001, 48.0],
                    [8.001, 48.001],
                    [8.0, 48.0],
                ]],
            },
            use_kind: BuildingUse::Residential,
            name: None,
            levels: Some(1),
            height_m: Some(3.0),
            gross_floor_area_m2,
        }
    }

    #[test]
    fn calendar_position_is_derived_from_tick_without_elapsed_time_state() {
        let config = CityTimeConfig::new(30).unwrap();
        let position = config.position(50).unwrap();

        assert_eq!(position.tick, 50);
        assert_eq!(position.day, 1);
        assert_eq!(position.minute_of_day, 60);
    }

    #[test]
    fn batch_steps_equal_repeated_single_steps() {
        let config = CityTimeConfig::new(15).unwrap();
        let mut batch = CitySave::new_with_time_config(scenario(), config).unwrap();
        let mut repeated = batch.clone();

        let batch_position = batch.advance_fixed_steps(100).unwrap();
        let mut repeated_position = repeated.time_position().unwrap();
        for _ in 0..100 {
            repeated_position = repeated.advance_fixed_steps(1).unwrap();
        }

        assert_eq!(batch, repeated);
        assert_eq!(batch_position, repeated_position);
    }

    #[test]
    fn fixed_step_batch_commits_once_without_rescanning_buildings() {
        let mut large = scenario();
        large.buildings = (0..4_096)
            .map(|index| residential_building(&format!("building/{index}"), 9_000))
            .collect();
        let mut save = CitySave::new(large).unwrap();

        reset_tick_writes();
        reset_capacity_building_visits();

        let position = save.advance_fixed_steps(1_000_000).unwrap();

        assert_eq!(position.tick, 1_000_000);
        assert_eq!(save.time_position().unwrap(), position);
        assert_eq!(tick_writes(), 1, "a fixed-step batch must commit the clock once");
        assert_eq!(
            capacity_building_visits(),
            0,
            "clock advancement must not rescan immutable building capacity"
        );

        reset_tick_writes();
        let unchanged = save.advance_fixed_steps(0).unwrap();
        assert_eq!(unchanged, position);
        assert_eq!(tick_writes(), 0, "zero-step advancement must not write the clock");
    }

    #[test]
    fn save_resume_continues_from_identical_clock_position() {
        let config = CityTimeConfig::new(60).unwrap();
        let mut original = CitySave::new_with_time_config(scenario(), config).unwrap();
        original.advance_fixed_steps(37).unwrap();

        let encoded = serde_json::to_string(&original).unwrap();
        let mut resumed: CitySave = serde_json::from_str(&encoded).unwrap();

        let original_position = original.advance_fixed_steps(11).unwrap();
        let resumed_position = resumed.advance_fixed_steps(11).unwrap();

        assert_eq!(original, resumed);
        assert_eq!(original_position, resumed_position);
    }

    #[test]
    fn invalid_configuration_and_overflow_fail_without_mutation() {
        assert_eq!(
            CityTimeConfig::new(7),
            Err(CityTimeError::InvalidMinutesPerTick(7))
        );

        let invalid: CityTimeConfig = serde_json::from_str(r#"{"minutesPerTick":7}"#).unwrap();
        let mut invalid_save = CitySave::new(scenario()).unwrap();
        invalid_save.time = invalid;
        let before_invalid = invalid_save.clone();
        assert_eq!(
            invalid_save.advance_fixed_steps(1),
            Err(CitySaveError::Time(CityTimeError::InvalidMinutesPerTick(7)))
        );
        assert_eq!(invalid_save, before_invalid);

        let mut overflow_save = CitySave::new(scenario()).unwrap();
        overflow_save.world.tick = u64::MAX - 1;
        let before_overflow = overflow_save.clone();
        assert_eq!(
            overflow_save.advance_fixed_steps(2),
            Err(CitySaveError::Time(CityTimeError::TickOverflow))
        );
        assert_eq!(overflow_save, before_overflow);
    }

    #[test]
    fn invalid_population_capacity_is_rejected_at_save_boundary() {
        let mut invalid = scenario();
        invalid.buildings = vec![
            residential_building("residential/1", u64::MAX),
            residential_building("residential/2", u64::MAX),
        ];
        let mut ruleset = CityRuleset::default();
        ruleset.population.config = PopulationRules {
            residential_floor_area_m2_per_household: 1,
            ..PopulationRules::default()
        };

        assert_eq!(
            CitySave::new_with_config(invalid, CityTimeConfig::default(), ruleset),
            Err(CitySaveError::Population(PopulationError::CapacityOverflow))
        );
    }

    #[test]
    fn disabled_population_rules_remain_valid_without_tick_revalidation() {
        let mut ruleset = CityRuleset::default();
        ruleset.set_status(RuleSystem::Population, RuleStatus::Disabled);
        ruleset
            .population
            .config
            .residential_floor_area_m2_per_household = 0;
        let mut save =
            CitySave::new_with_config(scenario(), CityTimeConfig::default(), ruleset).unwrap();

        assert_eq!(save.advance_fixed_steps(10).unwrap().tick, 10);
    }
}
