use std::fmt;

use serde::{Deserialize, Serialize};

use crate::{CitySave, CitySaveError, CityScenario, CityWorld};

pub const MINUTES_PER_DAY: u16 = 24 * 60;
pub const DEFAULT_MINUTES_PER_TICK: u16 = 15;

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
        let ticks_per_day = u64::from(MINUTES_PER_DAY / self.minutes_per_tick);
        let tick_in_day = tick % ticks_per_day;
        let minute_of_day =
            u16::try_from(tick_in_day).expect("tick within a day fits u16") * self.minutes_per_tick;

        Ok(CityTimePosition {
            tick,
            day: tick / ticks_per_day,
            minute_of_day,
        })
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
        let final_position = time.position(final_tick)?;

        for _ in 0..steps {
            self.tick += 1;
        }
        debug_assert_eq!(self.tick, final_tick);

        Ok(final_position)
    }
}

impl CitySave {
    pub fn new_with_time_config(
        scenario: CityScenario,
        time: CityTimeConfig,
    ) -> Result<Self, CitySaveError> {
        time.validate()?;
        let mut save = Self::new(scenario)?;
        save.time = time;
        Ok(save)
    }

    pub(crate) fn time_position(&self) -> Result<CityTimePosition, CityTimeError> {
        self.time.position(self.world.tick)
    }

    pub(crate) fn advance_tick(&mut self) -> Result<CityTimePosition, CitySaveError> {
        self.advance_fixed_steps(1)
    }

    pub(crate) fn advance_fixed_steps(
        &mut self,
        steps: u64,
    ) -> Result<CityTimePosition, CitySaveError> {
        Ok(self.world.advance_fixed_steps(self.time, steps)?)
    }
}

#[cfg(test)]
mod tests {
    use crate::{ExternalRevision, SCENARIO_SCHEMA_VERSION, ScenarioProvenance};

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
            repeated_position = repeated.advance_tick().unwrap();
        }

        assert_eq!(batch, repeated);
        assert_eq!(batch_position, repeated_position);
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
}
