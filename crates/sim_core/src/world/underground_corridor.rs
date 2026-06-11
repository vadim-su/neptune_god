//! Shared underground corridor tiles between entrance/exit pairs.

use std::collections::{BTreeMap, BTreeSet};

use serde::{Deserialize, Serialize};

use crate::ids::TilePos;
use crate::topology::graph::Direction;
use crate::units::UnitsPerTick;

#[derive(Clone, Copy, Debug, Deserialize, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize)]
pub struct UndergroundCorridorRecord {
    pub speed: UnitsPerTick,
    pub direction: Direction,
}

pub type UndergroundCorridors = BTreeMap<TilePos, BTreeSet<UndergroundCorridorRecord>>;

pub fn underground_corridor_tiles(entrance: TilePos, exit: TilePos) -> Vec<TilePos> {
    let delta = (
        (exit.x - entrance.x).signum(),
        (exit.y - entrance.y).signum(),
    );
    let mut tiles = Vec::new();
    let mut cursor = TilePos::new(entrance.x + delta.0, entrance.y + delta.1);
    while cursor != exit {
        tiles.push(cursor);
        cursor = TilePos::new(cursor.x + delta.0, cursor.y + delta.1);
    }
    tiles
}
