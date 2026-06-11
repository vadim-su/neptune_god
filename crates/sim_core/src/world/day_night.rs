//! Day/night cycle driving solar curve and time-of-day for energy consumers.

use serde::{Deserialize, Serialize};

/// Default sim day length in ticks (Factorio-style 24k at 60 UPS).
pub const DAY_LENGTH_TICKS: u32 = 24_000;
const RATIO_FULL_PPM: u32 = 1_000_000;

use crate::energy::SuppliedRatio;

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
/// Normalized position within the current day (0 = midnight, 0.5 = noon by default).
pub struct TimeOfDay {
    tick: u32,
    #[serde(default = "default_day_length_ticks")]
    day_length_ticks: u32,
}

impl Default for TimeOfDay {
    fn default() -> Self {
        Self {
            tick: DAY_LENGTH_TICKS / 2,
            day_length_ticks: DAY_LENGTH_TICKS,
        }
    }
}

impl TimeOfDay {
    pub fn from_normalized(value: f32) -> Result<Self, String> {
        if !value.is_finite() || !(0.0..=1.0).contains(&value) {
            return Err("time of day must be a finite value between 0.0 and 1.0".to_string());
        }
        Self::from_normalized_with_day_length(value, DAY_LENGTH_TICKS)
    }

    pub fn from_normalized_with_day_length(
        value: f32,
        day_length_ticks: u32,
    ) -> Result<Self, String> {
        if !value.is_finite() || !(0.0..=1.0).contains(&value) {
            return Err("time of day must be a finite value between 0.0 and 1.0".to_string());
        }
        let day_length_ticks = day_length_ticks.max(1);
        let tick = if value >= 1.0 {
            0
        } else {
            ((value * day_length_ticks as f32).round() as u32) % day_length_ticks
        };
        Ok(Self {
            tick,
            day_length_ticks,
        })
    }

    pub fn normalized(self) -> f32 {
        let day_length_ticks = self.day_length_ticks.max(1);
        self.tick.min(day_length_ticks - 1) as f32 / day_length_ticks as f32
    }

    pub(crate) fn raw_tick(self) -> u32 {
        self.tick
    }

    pub(crate) fn day_length_ticks(self) -> u32 {
        self.day_length_ticks.max(1)
    }

    pub(crate) fn with_day_length(self, day_length_ticks: u32) -> Self {
        Self::from_normalized_with_day_length(self.normalized(), day_length_ticks)
            .expect("existing normalized time is always valid")
    }

    pub(crate) fn advance_one_tick(&mut self) {
        let day_length_ticks = self.day_length_ticks();
        self.tick = (self.tick + 1) % day_length_ticks;
    }
}

fn default_day_length_ticks() -> u32 {
    DAY_LENGTH_TICKS
}

#[derive(Clone, Copy, Debug, Deserialize, PartialEq, Serialize)]
pub struct DayNightSettings {
    #[serde(default = "default_day_length_ticks")]
    pub day_length_ticks: u32,
    #[serde(default)]
    pub solar_curve: SolarCurveSettings,
}

impl Default for DayNightSettings {
    fn default() -> Self {
        Self {
            day_length_ticks: DAY_LENGTH_TICKS,
            solar_curve: SolarCurveSettings::default(),
        }
    }
}

impl DayNightSettings {
    pub fn normalized(self) -> Self {
        Self {
            day_length_ticks: self.day_length_ticks.max(1),
            solar_curve: self.solar_curve.normalized(),
        }
    }

    pub fn solar_factor(self, time_of_day: TimeOfDay) -> SuppliedRatio {
        self.solar_curve.solar_factor(time_of_day.normalized())
    }
}

#[derive(Clone, Copy, Debug, Deserialize, PartialEq, Serialize)]
pub enum SolarCurveSettings {
    GameLike {
        sunrise_start: f32,
        full_day_start: f32,
        full_day_end: f32,
        sunset_end: f32,
    },
}

impl Default for SolarCurveSettings {
    fn default() -> Self {
        Self::GameLike {
            sunrise_start: 0.20,
            full_day_start: 0.30,
            full_day_end: 0.70,
            sunset_end: 0.82,
        }
    }
}

impl SolarCurveSettings {
    fn normalized(self) -> Self {
        match self {
            Self::GameLike {
                sunrise_start,
                full_day_start,
                full_day_end,
                sunset_end,
            } if valid_curve(sunrise_start, full_day_start, full_day_end, sunset_end) => {
                Self::GameLike {
                    sunrise_start,
                    full_day_start,
                    full_day_end,
                    sunset_end,
                }
            }
            Self::GameLike { .. } => Self::default(),
        }
    }

    fn solar_factor(self, phase: f32) -> SuppliedRatio {
        match self.normalized() {
            Self::GameLike {
                sunrise_start,
                full_day_start,
                full_day_end,
                sunset_end,
            } => {
                let phase = phase.rem_euclid(1.0);
                let factor = if phase < sunrise_start || phase >= sunset_end {
                    0.0
                } else if phase < full_day_start {
                    (phase - sunrise_start) / (full_day_start - sunrise_start)
                } else if phase <= full_day_end {
                    1.0
                } else {
                    1.0 - (phase - full_day_end) / (sunset_end - full_day_end)
                };
                SuppliedRatio::from_ppm((factor.clamp(0.0, 1.0) * RATIO_FULL_PPM as f32) as u32)
            }
        }
    }
}

fn valid_curve(
    sunrise_start: f32,
    full_day_start: f32,
    full_day_end: f32,
    sunset_end: f32,
) -> bool {
    [sunrise_start, full_day_start, full_day_end, sunset_end]
        .into_iter()
        .all(|value| value.is_finite() && (0.0..=1.0).contains(&value))
        && sunrise_start < full_day_start
        && full_day_start <= full_day_end
        && full_day_end < sunset_end
}
