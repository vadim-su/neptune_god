//! Fixed-point power and energy amounts for deterministic simulation.

use serde::{
    Deserialize, Deserializer, Serialize,
    de::{self, SeqAccess, Visitor},
};
use std::fmt;

#[derive(Clone, Copy, Debug, Default, Deserialize, PartialEq, Eq, PartialOrd, Ord, Serialize)]
pub struct PowerUnits(i64);

impl PowerUnits {
    pub const ZERO: Self = Self(0);

    pub const fn new(raw: i64) -> Self {
        Self(raw)
    }

    pub const fn raw(self) -> i64 {
        self.0
    }

    pub fn min(self, other: Self) -> Self {
        Self(self.0.min(other.0))
    }
}

#[derive(Clone, Copy, Debug, Default, Deserialize, PartialEq, Eq, PartialOrd, Ord, Serialize)]
pub struct EnergyAmount(i64);

impl EnergyAmount {
    pub const ZERO: Self = Self(0);

    pub const fn new(raw: i64) -> Self {
        Self(raw)
    }

    pub const fn raw(self) -> i64 {
        self.0
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize)]
pub struct SuppliedRatio(u32);

impl SuppliedRatio {
    pub const ZERO: Self = Self(0);
    pub const FULL: Self = Self(1_000_000);

    pub const fn from_ppm(ppm: u32) -> Self {
        Self(if ppm > 1_000_000 { 1_000_000 } else { ppm })
    }

    pub fn from_parts(supplied: PowerUnits, demand: PowerUnits) -> Self {
        if demand.raw() <= 0 {
            return Self::FULL;
        }

        let ppm = ((supplied.raw().max(0) as i128) * 1_000_000 / demand.raw() as i128)
            .clamp(0, 1_000_000) as u32;
        Self(ppm)
    }

    pub const fn ppm(self) -> u32 {
        self.0
    }

    pub const fn is_zero(self) -> bool {
        self.0 == 0
    }
}

impl Default for SuppliedRatio {
    fn default() -> Self {
        Self::FULL
    }
}

impl<'de> Deserialize<'de> for SuppliedRatio {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        struct SuppliedRatioVisitor;

        impl<'de> Visitor<'de> for SuppliedRatioVisitor {
            type Value = SuppliedRatio;

            fn expecting(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
                formatter.write_str("a supplied ratio in parts per million")
            }

            fn visit_u64<E>(self, value: u64) -> Result<Self::Value, E>
            where
                E: de::Error,
            {
                Ok(SuppliedRatio::from_ppm(
                    value.min(u64::from(u32::MAX)) as u32
                ))
            }

            fn visit_i64<E>(self, value: i64) -> Result<Self::Value, E>
            where
                E: de::Error,
            {
                Ok(SuppliedRatio::from_ppm(
                    value.max(0).min(i64::from(u32::MAX)) as u32,
                ))
            }

            fn visit_newtype_struct<D>(self, deserializer: D) -> Result<Self::Value, D::Error>
            where
                D: Deserializer<'de>,
            {
                let ppm = u32::deserialize(deserializer)?;
                Ok(SuppliedRatio::from_ppm(ppm))
            }

            fn visit_seq<A>(self, mut seq: A) -> Result<Self::Value, A::Error>
            where
                A: SeqAccess<'de>,
            {
                let ppm = seq
                    .next_element::<u32>()?
                    .ok_or_else(|| de::Error::invalid_length(0, &self))?;
                Ok(SuppliedRatio::from_ppm(ppm))
            }
        }

        deserializer.deserialize_newtype_struct("SuppliedRatio", SuppliedRatioVisitor)
    }
}
