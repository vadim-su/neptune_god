//! Distance along belts and belt speed in fixed-point units per tick.

use std::ops::{Add, AddAssign, Sub, SubAssign};

use serde::{Deserialize, Serialize};

#[derive(
    Clone, Copy, Debug, Default, Deserialize, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize,
)]
pub struct DistanceUnits(i32);

impl DistanceUnits {
    pub const ZERO: Self = Self(0);
    /// Distance units per belt tile (256 sub-units per tile).
    pub const UNITS_PER_TILE: i32 = 256;

    pub const fn new(raw: i32) -> Self {
        Self(raw)
    }

    pub const fn from_tiles(tiles: i32) -> Self {
        Self(tiles * Self::UNITS_PER_TILE)
    }

    pub const fn raw(self) -> i32 {
        self.0
    }

    pub const fn as_tile_fraction(self) -> (i32, i32) {
        (self.0 / Self::UNITS_PER_TILE, self.0 % Self::UNITS_PER_TILE)
    }

    pub const fn saturating_sub(self, rhs: Self) -> Self {
        Self(if self.0 > rhs.0 { self.0 - rhs.0 } else { 0 })
    }
}

impl Add for DistanceUnits {
    type Output = Self;

    fn add(self, rhs: Self) -> Self::Output {
        Self(self.0 + rhs.0)
    }
}

impl AddAssign for DistanceUnits {
    fn add_assign(&mut self, rhs: Self) {
        self.0 += rhs.0;
    }
}

impl Sub for DistanceUnits {
    type Output = Self;

    fn sub(self, rhs: Self) -> Self::Output {
        Self(self.0 - rhs.0)
    }
}

impl SubAssign for DistanceUnits {
    fn sub_assign(&mut self, rhs: Self) {
        self.0 -= rhs.0;
    }
}

#[derive(
    Clone, Copy, Debug, Default, Deserialize, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize,
)]
pub struct UnitsPerTick(i32);

impl UnitsPerTick {
    pub const fn new(raw: i32) -> Self {
        Self(raw)
    }

    pub const fn raw(self) -> i32 {
        self.0
    }

    pub const fn distance_per_tick(self) -> DistanceUnits {
        DistanceUnits::new(self.0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn converts_tiles_to_fixed_units() {
        assert_eq!(DistanceUnits::UNITS_PER_TILE, 256);
        assert_eq!(DistanceUnits::from_tiles(3).raw(), 768);
        assert_eq!(DistanceUnits::new(64).as_tile_fraction(), (0, 64));
        assert_eq!(DistanceUnits::new(320).as_tile_fraction(), (1, 64));
    }

    #[test]
    fn clamps_subtraction_at_zero_for_gaps() {
        assert_eq!(
            DistanceUnits::new(10).saturating_sub(DistanceUnits::new(30)),
            DistanceUnits::ZERO
        );
    }

    #[test]
    fn speed_is_integer_units_per_tick() {
        let speed = UnitsPerTick::new(8);
        assert_eq!(speed.raw(), 8);
        assert_eq!(speed.distance_per_tick(), DistanceUnits::new(8));
    }
}
