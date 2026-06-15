//! Strongly typed simulation IDs and tile/chunk coordinates.

#![allow(dead_code)]

use serde::{Deserialize, Serialize};

/// Tiles per chunk edge (matches render chunking in the app).
pub const CHUNK_SIZE: i32 = 32;
/// Default surface level for old saves and flat worlds.
pub const DEFAULT_SURFACE_Z: SurfaceZ = 0;

/// Discrete vertical surface level for z-level terrain.
pub type SurfaceZ = i16;

#[derive(Clone, Copy, Debug, Deserialize, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize)]
pub struct TilePos {
    pub x: i32,
    pub y: i32,
}

impl TilePos {
    pub const fn new(x: i32, y: i32) -> Self {
        Self { x, y }
    }

    pub const fn chunk_pos(self) -> ChunkPos {
        ChunkPos::new(self.x.div_euclid(CHUNK_SIZE), self.y.div_euclid(CHUNK_SIZE))
    }
}

#[derive(Clone, Copy, Debug, Deserialize, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize)]
pub struct TileCoord3 {
    pub x: i32,
    pub y: i32,
    pub z: SurfaceZ,
}

impl TileCoord3 {
    pub const fn new(x: i32, y: i32, z: SurfaceZ) -> Self {
        Self { x, y, z }
    }

    pub const fn from_tile_pos(pos: TilePos, z: SurfaceZ) -> Self {
        Self {
            x: pos.x,
            y: pos.y,
            z,
        }
    }

    pub const fn tile_pos(self) -> TilePos {
        TilePos::new(self.x, self.y)
    }

    pub const fn chunk_pos(self) -> ChunkPos {
        self.tile_pos().chunk_pos()
    }
}

#[derive(Clone, Copy, Debug, Deserialize, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize)]
pub struct ChunkPos {
    pub x: i32,
    pub y: i32,
}

impl ChunkPos {
    pub const fn new(x: i32, y: i32) -> Self {
        Self { x, y }
    }
}

#[derive(Clone, Copy, Debug, Deserialize, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize)]
pub struct BuildingId(pub u32);

#[derive(Clone, Copy, Debug, Deserialize, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize)]
pub struct EnergyNodeId(pub u32);

#[derive(Clone, Copy, Debug, Deserialize, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize)]
pub struct EnergyEdgeId(pub u32);

#[derive(Clone, Copy, Debug, Deserialize, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize)]
pub struct LineId(pub u32);

#[derive(Clone, Copy, Debug, Deserialize, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize)]
pub struct InventoryId(pub u32);

#[derive(Clone, Copy, Debug, Deserialize, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize)]
pub struct GroupId(pub u32);

#[derive(Clone, Copy, Debug, Deserialize, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize)]
pub struct ItemKindId(pub u16);

#[derive(Clone, Copy, Debug, Deserialize, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize)]
pub struct ItemInstanceId(pub u32);

#[derive(Clone, Debug, Deserialize, PartialEq, Eq, Serialize)]
pub struct IdAllocatorSnapshot {
    pub next_building: u32,
    pub next_line: u32,
    pub next_inventory: u32,
    pub next_group: u32,
    pub next_item_kind: u16,
    #[serde(default)]
    pub next_item_instance: u32,
}

#[derive(Debug, Default)]
pub struct IdAllocator {
    next_building: u32,
    next_line: u32,
    next_inventory: u32,
    next_group: u32,
    next_item_kind: u16,
    next_item_instance: u32,
}

impl IdAllocator {
    pub fn next_building(&mut self) -> BuildingId {
        let id = BuildingId(self.next_building);
        self.next_building += 1;
        id
    }

    pub fn next_line(&mut self) -> LineId {
        let id = LineId(self.next_line);
        self.next_line += 1;
        id
    }

    pub fn next_inventory(&mut self) -> InventoryId {
        let id = InventoryId(self.next_inventory);
        self.next_inventory += 1;
        id
    }

    pub fn next_group(&mut self) -> GroupId {
        let id = GroupId(self.next_group);
        self.next_group += 1;
        id
    }

    pub fn next_item_kind(&mut self) -> ItemKindId {
        let id = ItemKindId(self.next_item_kind);
        self.next_item_kind += 1;
        id
    }

    pub fn next_item_instance(&mut self) -> ItemInstanceId {
        let id = ItemInstanceId(self.next_item_instance);
        self.next_item_instance += 1;
        id
    }

    pub fn snapshot(&self) -> IdAllocatorSnapshot {
        IdAllocatorSnapshot {
            next_building: self.next_building,
            next_line: self.next_line,
            next_inventory: self.next_inventory,
            next_group: self.next_group,
            next_item_kind: self.next_item_kind,
            next_item_instance: self.next_item_instance,
        }
    }

    pub fn from_snapshot(snapshot: IdAllocatorSnapshot) -> Self {
        Self {
            next_building: snapshot.next_building,
            next_line: snapshot.next_line,
            next_inventory: snapshot.next_inventory,
            next_group: snapshot.next_group,
            next_item_kind: snapshot.next_item_kind,
            next_item_instance: snapshot.next_item_instance,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tile_coord3_keeps_z_separate_from_chunking() {
        let coord = TileCoord3::new(33, -1, 2);

        assert_eq!(coord.tile_pos(), TilePos::new(33, -1));
        assert_eq!(coord.chunk_pos(), ChunkPos::new(1, -1));
        assert_eq!(coord.z, 2);
    }

    #[test]
    fn allocator_returns_stable_monotonic_ids() {
        let mut ids = IdAllocator::default();
        assert_eq!(ids.next_line(), LineId(0));
        assert_eq!(ids.next_line(), LineId(1));
        assert_eq!(ids.next_inventory(), InventoryId(0));
        assert_eq!(ids.next_group(), GroupId(0));
    }

    #[test]
    fn tile_to_chunk_uses_floor_division_for_negative_tiles() {
        assert_eq!(TilePos::new(0, 0).chunk_pos(), ChunkPos::new(0, 0));
        assert_eq!(TilePos::new(31, 31).chunk_pos(), ChunkPos::new(0, 0));
        assert_eq!(TilePos::new(32, 0).chunk_pos(), ChunkPos::new(1, 0));
        assert_eq!(TilePos::new(-1, 0).chunk_pos(), ChunkPos::new(-1, 0));
        assert_eq!(TilePos::new(-32, 0).chunk_pos(), ChunkPos::new(-1, 0));
        assert_eq!(TilePos::new(-33, 0).chunk_pos(), ChunkPos::new(-2, 0));
    }
}
