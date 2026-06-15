//! Building instances: footprint, ports, inventories, and driver-specific runtime state.

use crate::behavior_host::{BehaviorHost, BehaviorInitInput};
use crate::catalog::{
    CoreBuildingBehavior, CoreBuildingDriver, CoreBuildingKind, CoreItemStack, CorePortRole,
};

use crate::ids::{BuildingId, DEFAULT_SURFACE_Z, InventoryId, ItemKindId, SurfaceZ, TilePos};
use crate::inventory::{SimInventory, SimInventorySnapshot};
use crate::topology::graph::Direction;
use crate::transport::node::SplitterRuntime;
use behavior_api::{BehaviorHostError, BehaviorInstanceState};
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Deserialize, PartialEq, Eq, Serialize)]
/// One placed building with catalog def, rotation, ports, and [`SimBuildingState`].
pub struct SimBuilding {
    pub id: BuildingId,
    pub def_id: String,
    #[serde(with = "serde_core_building_kind")]
    pub kind: CoreBuildingKind,
    pub origin: TilePos,
    #[serde(with = "serde_direction")]
    pub direction: Direction,
    #[serde(default = "default_surface_z")]
    pub surface_z: SurfaceZ,
    pub footprint: Vec<TilePos>,
    pub ports: Vec<SimBuildingPort>,
    pub inventories: Vec<InventoryId>,
    pub state: SimBuildingState,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Eq, Serialize)]
pub struct SimBuildingPort {
    #[serde(with = "serde_core_port_role")]
    pub role: CorePortRole,
    pub tile: TilePos,
    #[serde(default = "default_surface_z")]
    pub surface_z: SurfaceZ,
    pub accepts: Vec<ItemKindId>,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Eq, Serialize)]
pub enum SimBuildingState {
    Passive,
    Transport,
    Behavior(BehaviorInstanceState),
    Inserter(InserterRuntime),
    Underground(UndergroundRuntime),
    Splitter(SplitterRuntime),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum UndergroundRole {
    Entrance,
    Exit,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Eq, Serialize)]
pub struct UndergroundRuntime {
    pub role: UndergroundRole,
    pub partner: BuildingId,
    #[serde(with = "serde_direction")]
    pub direction: Direction,
}

impl SimBuildingState {
    pub fn behavior_state(&self) -> Option<&BehaviorInstanceState> {
        match self {
            Self::Behavior(state) => Some(state),
            _ => None,
        }
    }
}

#[derive(Clone, Debug, Deserialize, PartialEq, Eq, Serialize)]
pub struct InserterRuntime {
    #[serde(with = "serde_direction")]
    pub pickup_direction: Direction,
    #[serde(with = "serde_direction")]
    pub drop_direction: Direction,
    pub cooldown_remaining_ticks: u32,
    #[serde(with = "serde_core_item_stack_option")]
    pub carried: Option<CoreItemStack>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SimInventoryRecord {
    pub id: InventoryId,
    pub owner: BuildingId,
    pub inventory: SimInventory,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SimBuildingSnapshot {
    pub id: BuildingId,
    pub def_id: String,
    pub kind: CoreBuildingKind,
    pub origin: TilePos,
    pub surface_z: SurfaceZ,
    pub direction: Direction,
    pub state: SimBuildingState,
    pub inventories: Vec<SimInventorySnapshot>,
}

fn default_surface_z() -> SurfaceZ {
    DEFAULT_SURFACE_Z
}

pub fn footprint_tiles(origin: TilePos, offsets: &[(i32, i32)]) -> Vec<TilePos> {
    let mut tiles = offsets
        .iter()
        .map(|(x, y)| TilePos::new(origin.x + *x, origin.y + *y))
        .collect::<Vec<_>>();
    tiles.sort();
    tiles
}

pub fn initial_state(
    _kind: CoreBuildingKind,
    behavior: &CoreBuildingBehavior,
    direction: Direction,
    inserter_drop_direction: Option<Direction>,
    behavior_host: &(impl BehaviorHost + ?Sized),
) -> Result<SimBuildingState, BehaviorHostError> {
    match &behavior.driver {
        CoreBuildingDriver::Noop => Ok(SimBuildingState::Passive),
        CoreBuildingDriver::Transport { .. } => Ok(SimBuildingState::Transport),
        CoreBuildingDriver::ConveyorLift { .. } => Ok(SimBuildingState::Passive),
        CoreBuildingDriver::Splitter { .. } => {
            Ok(SimBuildingState::Splitter(SplitterRuntime::default()))
        }
        CoreBuildingDriver::Underground { .. } => Err(BehaviorHostError::new(
            behavior_api::BehaviorHostErrorKind::InvalidManifest,
            "underground requires paired placement",
        )),
        CoreBuildingDriver::Inserter { .. } => Ok(SimBuildingState::Inserter(InserterRuntime {
            pickup_direction: direction,
            drop_direction: inserter_drop_direction.unwrap_or_else(|| direction.opposite()),
            cooldown_remaining_ticks: 0,
            carried: None,
        })),
        CoreBuildingDriver::BehaviorHost => behavior_host
            .initial_behavior_state(BehaviorInitInput {
                behavior_id: &behavior.behavior_id,
                config: &behavior.config,
            })
            .map(SimBuildingState::Behavior),
    }
}

mod serde_core_building_kind {
    use super::*;
    use serde::{Deserialize, Deserializer, Serializer};

    pub fn serialize<S>(kind: &CoreBuildingKind, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        let value = match kind {
            CoreBuildingKind::Machine => "machine",
            CoreBuildingKind::Transport => "transport",
            CoreBuildingKind::Passive => "passive",
            CoreBuildingKind::Inserter => "inserter",
        };
        serializer.serialize_str(value)
    }

    pub fn deserialize<'de, D>(deserializer: D) -> Result<CoreBuildingKind, D::Error>
    where
        D: Deserializer<'de>,
    {
        match String::deserialize(deserializer)?.as_str() {
            "machine" => Ok(CoreBuildingKind::Machine),
            "transport" => Ok(CoreBuildingKind::Transport),
            "passive" => Ok(CoreBuildingKind::Passive),
            "inserter" => Ok(CoreBuildingKind::Inserter),
            value => Err(serde::de::Error::unknown_variant(
                value,
                &["machine", "transport", "passive", "inserter"],
            )),
        }
    }
}

mod serde_core_port_role {
    use super::*;
    use serde::{Deserialize, Deserializer, Serializer};

    pub fn serialize<S>(role: &CorePortRole, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        let value = match role {
            CorePortRole::Input => "input",
            CorePortRole::Output => "output",
            CorePortRole::Fuel => "fuel",
            CorePortRole::Storage => "storage",
            CorePortRole::BeltLane => "belt_lane",
        };
        serializer.serialize_str(value)
    }

    pub fn deserialize<'de, D>(deserializer: D) -> Result<CorePortRole, D::Error>
    where
        D: Deserializer<'de>,
    {
        match String::deserialize(deserializer)?.as_str() {
            "input" => Ok(CorePortRole::Input),
            "output" => Ok(CorePortRole::Output),
            "fuel" => Ok(CorePortRole::Fuel),
            "storage" => Ok(CorePortRole::Storage),
            "belt_lane" => Ok(CorePortRole::BeltLane),
            value => Err(serde::de::Error::unknown_variant(
                value,
                &["input", "output", "fuel", "storage", "belt_lane"],
            )),
        }
    }
}

mod serde_direction {
    use super::*;
    use serde::{Deserialize, Deserializer, Serializer};

    pub fn serialize<S>(direction: &Direction, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        let value = match direction {
            Direction::North => "north",
            Direction::East => "east",
            Direction::South => "south",
            Direction::West => "west",
        };
        serializer.serialize_str(value)
    }

    pub fn deserialize<'de, D>(deserializer: D) -> Result<Direction, D::Error>
    where
        D: Deserializer<'de>,
    {
        match String::deserialize(deserializer)?.as_str() {
            "north" => Ok(Direction::North),
            "east" => Ok(Direction::East),
            "south" => Ok(Direction::South),
            "west" => Ok(Direction::West),
            value => Err(serde::de::Error::unknown_variant(
                value,
                &["north", "east", "south", "west"],
            )),
        }
    }
}

#[derive(Deserialize, Serialize)]
struct CoreItemStackSerde {
    kind: ItemKindId,
    amount: u32,
}

impl From<CoreItemStack> for CoreItemStackSerde {
    fn from(stack: CoreItemStack) -> Self {
        Self {
            kind: stack.kind,
            amount: stack.amount,
        }
    }
}

impl From<CoreItemStackSerde> for CoreItemStack {
    fn from(stack: CoreItemStackSerde) -> Self {
        Self {
            kind: stack.kind,
            amount: stack.amount,
        }
    }
}

mod serde_core_item_stack_option {
    use super::*;
    use serde::{Deserialize, Deserializer, Serializer};

    pub fn serialize<S>(stack: &Option<CoreItemStack>, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        stack.map(CoreItemStackSerde::from).serialize(serializer)
    }

    pub fn deserialize<'de, D>(deserializer: D) -> Result<Option<CoreItemStack>, D::Error>
    where
        D: Deserializer<'de>,
    {
        Ok(Option::<CoreItemStackSerde>::deserialize(deserializer)?.map(CoreItemStack::from))
    }
}

mod serde_core_item_stack_vec {
    use super::*;
    use serde::{Deserialize, Deserializer, Serializer};

    #[allow(dead_code, reason = "used via serde(with = ...) on snapshot fields")]
    pub fn serialize<S>(stacks: &[CoreItemStack], serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        stacks
            .iter()
            .copied()
            .map(CoreItemStackSerde::from)
            .collect::<Vec<_>>()
            .serialize(serializer)
    }

    #[allow(dead_code, reason = "used via serde(with = ...) on snapshot fields")]
    pub fn deserialize<'de, D>(deserializer: D) -> Result<Vec<CoreItemStack>, D::Error>
    where
        D: Deserializer<'de>,
    {
        Ok(Vec::<CoreItemStackSerde>::deserialize(deserializer)?
            .into_iter()
            .map(CoreItemStack::from)
            .collect())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::behavior_host::NOOP_BEHAVIOR_HOST;
    use crate::catalog::{CoreBuildingBehavior, CoreBuildingKind};
    use crate::ids::TilePos;
    use crate::topology::graph::Direction;

    #[test]
    fn footprint_tiles_are_absolute_and_sorted() {
        assert_eq!(
            footprint_tiles(TilePos::new(10, 20), &[(1, 1), (0, 0), (1, 0)]),
            vec![
                TilePos::new(10, 20),
                TilePos::new(11, 20),
                TilePos::new(11, 21)
            ]
        );
    }

    #[test]
    fn inserter_initial_state_uses_direction_as_pickup_side_and_explicit_drop_side() {
        let state = initial_state(
            CoreBuildingKind::Inserter,
            &CoreBuildingBehavior::inserter(27),
            Direction::East,
            Some(Direction::West),
            &NOOP_BEHAVIOR_HOST,
        );

        assert_eq!(
            state.unwrap(),
            SimBuildingState::Inserter(InserterRuntime {
                pickup_direction: Direction::East,
                drop_direction: Direction::West,
                cooldown_remaining_ticks: 0,
                carried: None,
            })
        );
    }
}
