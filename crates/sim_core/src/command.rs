//! Commands mutating [`crate::world::SimWorld`] (place, remove, player inventory, behaviors).

use behavior_api::{BehaviorCommand, BehaviorHostError};

use crate::catalog::{CoreInventoryRole, CoreItemStack};
use crate::ids::{BuildingId, ItemKindId, SurfaceZ, TilePos};
use crate::tick::{BehaviorEffectRejectionReason, BehaviorHostFailurePhase};
use crate::topology::graph::Direction;
use crate::units::UnitsPerTick;

#[derive(Clone, Debug, PartialEq, Eq)]
/// One deterministic world mutation applied before or during a sim tick.
pub enum SimCommand {
    PlaceBuilding {
        def_id: String,
        origin: TilePos,
        direction: Direction,
        inserter_drop_direction: Option<Direction>,
    },
    PlaceUndergroundBelt {
        def_id: String,
        entrance: TilePos,
        exit: TilePos,
        direction: Direction,
    },
    PlaceUnderground {
        def_id: String,
        pos: TilePos,
        direction: Direction,
    },
    RotateUnderground {
        pos: TilePos,
    },
    PlaceBelt {
        pos: TilePos,
        direction: Direction,
        input_direction: Direction,
        speed: UnitsPerTick,
    },
    SeedResource {
        pos: TilePos,
        kind: ItemKindId,
        amount: u32,
    },
    RemoveBuilding {
        pos: TilePos,
    },
    ApplyBehaviorCommand {
        building: BuildingId,
        command: BehaviorCommand,
    },
    InsertIntoInventory {
        building: BuildingId,
        role: CoreInventoryRole,
        stack: CoreItemStack,
    },
    TakeFromInventory {
        building: BuildingId,
        role: CoreInventoryRole,
        slot: usize,
        amount: u32,
    },
    InsertItemAtLineStart {
        line_index: usize,
        lane: usize,
        item: ItemKindId,
    },
    DropItemOnBeltTile {
        pos: TilePos,
        lane: usize,
        distance_numerator: u16,
        distance_denominator: u16,
        item: ItemKindId,
    },
    CreateSource {
        pos: TilePos,
        item: ItemKindId,
        interval_ticks: u32,
    },
    CreateSink {
        pos: TilePos,
    },
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum SimCommandError {
    InvalidPosition {
        pos: TilePos,
    },
    OccupiedTile {
        pos: TilePos,
    },
    UnbuildableTile {
        pos: TilePos,
    },
    UnevenTerrain {
        origin: TilePos,
        pos: TilePos,
        expected_z: SurfaceZ,
        found_z: SurfaceZ,
    },
    MissingBuilding {
        pos: TilePos,
    },
    MissingBuildingId {
        building: BuildingId,
    },
    UnknownBuildingKind,
    InvalidRecipe,
    InvalidBehaviorCommand,
    BehaviorEffectRejected {
        building: BuildingId,
        reason: BehaviorEffectRejectionReason,
    },
    BehaviorHostFailed {
        building: Option<BuildingId>,
        phase: BehaviorHostFailurePhase,
        error: BehaviorHostError,
    },
    InventoryRejected,
    InvalidPort,
    TopologyConflict {
        pos: TilePos,
    },
    CapacityExceeded,
}
