//! Static belt topology indexed by tile (direction, underground endpoints).

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

use crate::ids::TilePos;
use crate::units::UnitsPerTick;

#[derive(Clone, Copy, Debug, Deserialize, Hash, PartialEq, Eq, PartialOrd, Ord, Serialize)]
pub enum Direction {
    North,
    East,
    South,
    West,
}

impl Direction {
    pub const fn delta(self) -> (i32, i32) {
        match self {
            Self::North => (0, 1),
            Self::East => (1, 0),
            Self::South => (0, -1),
            Self::West => (-1, 0),
        }
    }

    pub const fn left(self) -> Self {
        match self {
            Self::North => Self::West,
            Self::East => Self::North,
            Self::South => Self::East,
            Self::West => Self::South,
        }
    }

    pub const fn right(self) -> Self {
        match self {
            Self::North => Self::East,
            Self::East => Self::South,
            Self::South => Self::West,
            Self::West => Self::North,
        }
    }

    pub const fn opposite(self) -> Self {
        match self {
            Self::North => Self::South,
            Self::East => Self::West,
            Self::South => Self::North,
            Self::West => Self::East,
        }
    }

    pub const fn output_pos(self, pos: TilePos) -> TilePos {
        let (dx, dy) = self.delta();
        TilePos::new(pos.x + dx, pos.y + dy)
    }

    pub const fn near_lane_for_source_side(self, source_side: Direction) -> Option<usize> {
        match (self, source_side) {
            (Self::North, Self::West)
            | (Self::East, Self::North)
            | (Self::South, Self::East)
            | (Self::West, Self::South) => Some(0),
            (Self::North, Self::East)
            | (Self::East, Self::South)
            | (Self::South, Self::West)
            | (Self::West, Self::North) => Some(1),
            _ => None,
        }
    }
}

#[derive(Clone, Copy, Debug, Deserialize, PartialEq, Eq, Serialize)]
pub struct BeltTile {
    /// Tile output direction.
    pub direction: Direction,
    /// Only incoming direction that continues this packed transport line.
    pub input_direction: Direction,
}

impl BeltTile {
    pub const fn new(direction: Direction) -> Self {
        Self::straight(direction)
    }

    pub const fn straight(direction: Direction) -> Self {
        Self {
            direction,
            input_direction: direction,
        }
    }

    pub const fn turn(input_direction: Direction, direction: Direction) -> Self {
        Self {
            direction,
            input_direction,
        }
    }
}

#[derive(Clone, Copy, Debug, Deserialize, PartialEq, Eq, Serialize)]
pub enum UndergroundEndpointRole {
    Entrance,
    Exit,
}

#[derive(Clone, Copy, Debug, Deserialize, PartialEq, Eq, Serialize)]
pub struct UndergroundLink {
    pub partner: TilePos,
    pub role: UndergroundEndpointRole,
    pub speed: UnitsPerTick,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Eq, Serialize)]
pub struct TopologyGraphSnapshot {
    pub belts: BTreeMap<TilePos, BeltTile>,
    #[serde(default)]
    pub underground_links: BTreeMap<TilePos, UndergroundLink>,
    pub revision: u64,
}

#[derive(Debug, Default)]
pub struct TopologyGraph {
    belts: BTreeMap<TilePos, BeltTile>,
    underground_links: BTreeMap<TilePos, UndergroundLink>,
    revision: u64,
}

impl TopologyGraph {
    pub fn set_belt(&mut self, pos: TilePos, belt: BeltTile) {
        self.belts.insert(pos, belt);
        self.revision += 1;
    }

    pub fn remove_belt(&mut self, pos: TilePos) {
        if self.belts.remove(&pos).is_some() {
            self.revision += 1;
        }
    }

    pub fn belt(&self, pos: TilePos) -> Option<BeltTile> {
        self.belts.get(&pos).copied()
    }

    pub fn belts_sorted(&self) -> impl Iterator<Item = (TilePos, BeltTile)> + '_ {
        self.belts.iter().map(|(pos, belt)| (*pos, *belt))
    }

    pub fn revision(&self) -> u64 {
        self.revision
    }

    pub fn underground_link(&self, pos: TilePos) -> Option<UndergroundLink> {
        self.underground_links.get(&pos).copied()
    }

    pub fn set_underground_link(&mut self, pos: TilePos, link: UndergroundLink) {
        self.underground_links.insert(pos, link);
        self.revision += 1;
    }

    pub fn clear_underground_link(&mut self, pos: TilePos) {
        if self.underground_links.remove(&pos).is_some() {
            self.revision += 1;
        }
    }

    pub fn underground_links_sorted(
        &self,
    ) -> impl Iterator<Item = (TilePos, UndergroundLink)> + '_ {
        self.underground_links
            .iter()
            .map(|(pos, link)| (*pos, *link))
    }

    pub fn snapshot(&self) -> TopologyGraphSnapshot {
        TopologyGraphSnapshot {
            belts: self.belts.clone(),
            underground_links: self.underground_links.clone(),
            revision: self.revision,
        }
    }

    pub fn from_snapshot(snapshot: TopologyGraphSnapshot) -> Self {
        Self {
            belts: snapshot.belts,
            underground_links: snapshot.underground_links,
            revision: snapshot.revision,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn direction_turn_helpers_are_stable() {
        assert_eq!(Direction::North.left(), Direction::West);
        assert_eq!(Direction::North.right(), Direction::East);
        assert_eq!(Direction::East.left(), Direction::North);
        assert_eq!(Direction::East.right(), Direction::South);
        assert_eq!(Direction::South.left(), Direction::East);
        assert_eq!(Direction::South.right(), Direction::West);
        assert_eq!(Direction::West.left(), Direction::South);
        assert_eq!(Direction::West.right(), Direction::North);
    }

    #[test]
    fn belt_tile_new_is_straight_by_default() {
        let belt = BeltTile::new(Direction::North);

        assert_eq!(belt.direction, Direction::North);
        assert_eq!(belt.input_direction, Direction::North);
    }

    #[test]
    fn belt_tile_turn_stores_input_and_output_direction() {
        let belt = BeltTile::turn(Direction::East, Direction::North);

        assert_eq!(belt.input_direction, Direction::East);
        assert_eq!(belt.direction, Direction::North);
    }

    #[test]
    fn output_pos_uses_direction_delta() {
        let origin = TilePos::new(4, -3);

        assert_eq!(Direction::North.output_pos(origin), TilePos::new(4, -2));
        assert_eq!(Direction::East.output_pos(origin), TilePos::new(5, -3));
        assert_eq!(Direction::South.output_pos(origin), TilePos::new(4, -4));
        assert_eq!(Direction::West.output_pos(origin), TilePos::new(3, -3));
    }

    #[test]
    fn underground_link_set_get_clear_and_revision() {
        let mut graph = TopologyGraph::default();
        let entrance = TilePos::new(0, 0);
        let exit = TilePos::new(4, 0);
        let speed = crate::units::UnitsPerTick::new(4);
        let revision_before = graph.revision();

        graph.set_underground_link(
            entrance,
            UndergroundLink {
                partner: exit,
                role: UndergroundEndpointRole::Entrance,
                speed,
            },
        );
        graph.set_underground_link(
            exit,
            UndergroundLink {
                partner: entrance,
                role: UndergroundEndpointRole::Exit,
                speed,
            },
        );

        assert!(graph.revision() > revision_before);
        let entrance_link = graph.underground_link(entrance).unwrap();
        assert_eq!(entrance_link.partner, exit);
        assert_eq!(entrance_link.role, UndergroundEndpointRole::Entrance);
        assert_eq!(entrance_link.speed, speed);

        let exit_link = graph.underground_link(exit).unwrap();
        assert_eq!(exit_link.partner, entrance);
        assert_eq!(exit_link.role, UndergroundEndpointRole::Exit);

        graph.clear_underground_link(entrance);
        assert!(graph.underground_link(entrance).is_none());
        assert!(graph.underground_link(exit).is_some());

        graph.clear_underground_link(exit);
        assert!(graph.underground_link(exit).is_none());
    }

    #[test]
    fn near_lane_uses_source_side_relative_to_target_direction() {
        assert_eq!(
            Direction::North.near_lane_for_source_side(Direction::West),
            Some(0)
        );
        assert_eq!(
            Direction::North.near_lane_for_source_side(Direction::East),
            Some(1)
        );
        assert_eq!(
            Direction::East.near_lane_for_source_side(Direction::North),
            Some(0)
        );
        assert_eq!(
            Direction::East.near_lane_for_source_side(Direction::South),
            Some(1)
        );
        assert_eq!(
            Direction::North.near_lane_for_source_side(Direction::South),
            None
        );
        assert_eq!(
            Direction::North.near_lane_for_source_side(Direction::North),
            None
        );
    }
}
