//! Central simulation world: tick loop, buildings, transport, energy, behaviors.
//!
//! [`SimWorld`] applies [`SimCommand`]s, runs transport/inserter ticks, and
//! emits [`SimDiff`] plus metrics. Focused tick phases live in child modules under
//! `world/`; this file orchestrates them.

use std::collections::{BTreeMap, BTreeSet};

use crate::activation::scheduler::ActivationScheduler;
use crate::behavior_host::{
    BehaviorCommandInput, BehaviorEffectRejectionPolicy, BehaviorHost, BehaviorRuntime,
    BehaviorRuntimePolicy, NOOP_BEHAVIOR_HOST, behavior_building, behavior_inventories,
    behavior_kind, core_inventory_role, core_stack, core_stacks, core_tile_pos,
};
use crate::building::{
    InserterRuntime, SimBuilding, SimBuildingPort, SimBuildingSnapshot, SimBuildingState,
    SimInventoryRecord, UndergroundRole, UndergroundRuntime, footprint_tiles, initial_state,
};
use crate::catalog::{
    CoreBuildingDef, CoreBuildingDriver, CoreBuildingKind, CoreCatalog, CoreInventoryRole,
    CoreItemStack, CorePortDef, CorePortRole, CorePortSide, CoreTerrainDef,
};
use crate::character_inventory::{
    CharacterContainerId, CharacterContainerSection, CharacterEquipResult, CharacterEquipmentEntry,
    CharacterRouteResult, EquipmentSlotId, EquippedItem, LoadedContainerInstance,
    LoadedContainerSection, SimCharacterInventory, container_from_def,
};
use crate::command::{SimCommand, SimCommandError};
use crate::diff::SimDiff;
use crate::digest::WorldDigest;
use crate::energy::{EnergyNetwork, EnergyView, SuppliedRatio};
use crate::ids::{
    BuildingId, DEFAULT_SURFACE_Z, GroupId, IdAllocator, InventoryId, ItemInstanceId, ItemKindId,
    LineId, SurfaceZ, TilePos,
};
use crate::inserter::{
    CandidateRoleOrder, InserterCandidate, InserterCandidateKind, carried_stack,
    choose_nearest_candidate,
};
use crate::inventory::{
    InsertMode, InventoryInsertResult, InventoryItemRules, InventoryRejection, InventorySlotEntry,
    SimInventory,
};
use crate::metrics::SimMetricsSnapshot;
use crate::tick::{
    AppliedBehaviorEffect, BehaviorEffectApplication, BehaviorEffectRejectionReason,
    BehaviorEffectReport, BehaviorHostFailurePhase, CoreRemovalDrop, CoreResourceDepletion,
    CoreSurfaceDrop, RejectedBehaviorEffect, SimTick, SimTickOutput,
};
use crate::topology::builder::TopologyBuilder;
use crate::topology::graph::{BeltTile, Direction, TopologyGraph};
use crate::transport::line::{LineEndpoint, LinePath, LineTile, TransportLine};
use crate::transport::node::TransportNodeId;
use crate::transport::storage::TransportStorage;
use crate::transport::stream::{MIN_ITEM_SPACING, PackedItemStream};
use crate::units::{DistanceUnits, UnitsPerTick};
use crate::view::{VisibleItem, VisibleSplitterItemPhase, VisibleTileBounds};
use crate::worldgen::GeneratedMapRegion;
use behavior_api::{BehaviorCatalog, BehaviorConfigValue, BehaviorEffect, BehaviorId};

mod behaviors;
mod belt_io;
mod day_night;
mod digest_impl;
mod digesting;
mod inserters;
mod ports;
mod snapshot;
#[cfg(test)]
mod test_behavior;
#[cfg(test)]
mod tests;
mod transport_rebuild;
mod transport_runtime;
mod underground;
mod underground_corridor;
mod view;
mod view_impl;

pub use day_night::{DAY_LENGTH_TICKS, DayNightSettings, SolarCurveSettings, TimeOfDay};
pub use snapshot::{SimBehaviorQuarantineSnapshot, SimWorldSnapshot};

use self::belt_io::{insert_belt_item_from_side, lane_rank, snap_belt_insert_distance};
use self::ports::{
    adjacent_tile, building_ports, direction_between_adjacent, drop_role_order, pickup_role_order,
    port_inventory_role,
};
#[cfg(test)]
use self::transport_rebuild::side_load_source_lane_uses_exit_half;
use self::transport_rebuild::{
    BuiltLineRecord, OldTransportLine, drop_old_items_without_surviving_tile,
    insert_side_load_item, line_by_first_tile, line_containing_tile, line_ending_from_tile_to_tile,
    remap_lanes_for_new_line, side_load_insert_distances, source_side_for_target,
    splitter_channel_geometry,
};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct SplitterItemPosition {
    node: TransportNodeId,
    tile: TilePos,
    phase: VisibleSplitterItemPhase,
    channel: usize,
    lane: usize,
    progress: DistanceUnits,
    item: ItemKindId,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum UndergroundEndpointPhase {
    Entrance,
    Exit,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct UndergroundItemPosition {
    node: TransportNodeId,
    tile: TilePos,
    phase: UndergroundEndpointPhase,
    lane: usize,
    progress: DistanceUnits,
    distance: DistanceUnits,
    direction: Direction,
    item: ItemKindId,
}

fn footprint_offsets_for_direction(
    offsets: &[(i32, i32)],
    rotate_footprint: bool,
    direction: Direction,
) -> Vec<(i32, i32)> {
    let mut rotated = if rotate_footprint {
        offsets
            .iter()
            .map(|&(x, y)| match direction {
                Direction::East => (x, y),
                Direction::North => (-y, x),
                Direction::West => (-x, -y),
                Direction::South => (y, -x),
            })
            .collect::<Vec<_>>()
    } else {
        offsets.to_vec()
    };
    let min_x = rotated.iter().map(|(x, _)| *x).min().unwrap_or(0);
    let min_y = rotated.iter().map(|(_, y)| *y).min().unwrap_or(0);
    for (x, y) in &mut rotated {
        *x -= min_x;
        *y -= min_y;
    }
    rotated
}

fn extractor_requires_resource_footprint(def: &CoreBuildingDef) -> bool {
    matches!(
        def.behavior.config.get("role"),
        Some(BehaviorConfigValue::String(role)) if role == "extractor"
    )
}

fn extractor_resource_kinds(def: &CoreBuildingDef) -> BTreeSet<ItemKindId> {
    let mut resources = BTreeSet::new();
    for output in &def.outputs {
        resources.extend(output.accepts.iter().copied());
    }
    for inventory in &def.inventories {
        if inventory.role == CoreInventoryRole::Output {
            resources.extend(inventory.accepts.iter().copied());
        }
    }
    resources
}

fn splitter_internal_channel_tiles(origin: TilePos, direction: Direction) -> [TilePos; 2] {
    match direction {
        Direction::East | Direction::West => [origin, TilePos::new(origin.x, origin.y + 1)],
        Direction::North | Direction::South => [origin, TilePos::new(origin.x + 1, origin.y)],
    }
}

fn remove_frontmost_matching<T>(
    items: &mut Vec<T>,
    matches: impl Fn(&T) -> bool,
    progress: impl Fn(&T) -> DistanceUnits,
    item_kind: impl Fn(&T) -> ItemKindId,
) -> Option<ItemKindId> {
    let index = items
        .iter()
        .enumerate()
        .filter(|(_, item)| matches(item))
        .max_by_key(|(_, item)| progress(item))
        .map(|(index, _)| index)?;
    Some(item_kind(&items.remove(index)))
}

fn underground_endpoint_phase(
    distance: DistanceUnits,
    progress: DistanceUnits,
    phase: UndergroundEndpointPhase,
) -> bool {
    let half_tile = DistanceUnits::new(DistanceUnits::UNITS_PER_TILE / 2);
    match phase {
        UndergroundEndpointPhase::Entrance => progress < half_tile,
        UndergroundEndpointPhase::Exit => progress >= distance - half_tile,
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct BehaviorEffectRejection {
    effect: BehaviorEffect,
    reason: BehaviorEffectRejectionReason,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct BehaviorEffectApplyResult {
    resource_depletions: Vec<CoreResourceDepletion>,
    report: BehaviorEffectReport,
    rejection_reason: Option<BehaviorEffectRejectionReason>,
}

/// Authoritative factory state for one save or runtime instance.
pub struct SimWorld {
    tick: SimTick,
    time_of_day: TimeOfDay,
    day_night_settings: DayNightSettings,
    ids: IdAllocator,
    catalog: CoreCatalog,
    character_inventory: SimCharacterInventory,
    loaded_containers: BTreeMap<ItemInstanceId, LoadedContainerInstance>,
    player_inventory: SimInventory,
    cursor_inventory: SimInventory,
    terrain: BTreeMap<TilePos, String>,
    surface_z: BTreeMap<TilePos, SurfaceZ>,
    buildings: BTreeMap<BuildingId, SimBuilding>,
    building_by_origin: BTreeMap<TilePos, BuildingId>,
    building_occupancy: BTreeMap<TilePos, BuildingId>,
    inventories: BTreeMap<InventoryId, SimInventoryRecord>,
    resources: BTreeMap<TilePos, (ItemKindId, u32)>,
    occupied_tiles: BTreeMap<TilePos, UnitsPerTick>,
    topology_graph: TopologyGraph,
    topology_revision_seen: u64,
    transport: TransportStorage,
    energy: EnergyNetwork,
    activation: ActivationScheduler,
    metrics: SimMetricsSnapshot,
    removed_item_drops: Vec<CoreRemovalDrop>,
    surface_item_drops: Vec<CoreSurfaceDrop>,
    occupied_surface_tiles: BTreeSet<TilePos>,
    behavior_quarantine: BTreeMap<BuildingId, BehaviorEffectRejectionReason>,
    underground_corridors: underground_corridor::UndergroundCorridors,
}

impl Default for SimWorld {
    fn default() -> Self {
        Self::with_catalog(CoreCatalog::default())
    }
}

impl SimWorld {
    pub fn current_tick(&self) -> SimTick {
        self.tick
    }

    pub fn time_of_day(&self) -> TimeOfDay {
        self.time_of_day
    }

    pub fn set_time_of_day(&mut self, time_of_day: TimeOfDay) {
        self.time_of_day = time_of_day.with_day_length(self.day_night_settings.day_length_ticks);
    }

    pub fn day_night_settings(&self) -> DayNightSettings {
        self.day_night_settings
    }

    pub fn set_day_night_settings(&mut self, settings: DayNightSettings) {
        self.day_night_settings = settings.normalized();
        self.time_of_day = self
            .time_of_day
            .with_day_length(self.day_night_settings.day_length_ticks);
    }

    pub fn solar_factor(&self) -> SuppliedRatio {
        self.day_night_settings.solar_factor(self.time_of_day)
    }

    pub fn behavior_quarantine_count(&self) -> usize {
        self.behavior_quarantine.len()
    }

    pub fn behavior_quarantine_snapshots(&self) -> Vec<SimBehaviorQuarantineSnapshot> {
        self.behavior_quarantine
            .iter()
            .filter_map(|(building, reason)| {
                self.behavior_quarantine_snapshot(*building, reason.clone())
            })
            .collect()
    }

    pub fn clear_behavior_quarantine(
        &mut self,
        building: BuildingId,
    ) -> Option<SimBehaviorQuarantineSnapshot> {
        let reason = self.behavior_quarantine.remove(&building)?;
        self.behavior_quarantine_snapshot(building, reason)
    }

    pub fn clear_all_behavior_quarantines(&mut self) -> usize {
        let cleared = self.behavior_quarantine.len();
        self.behavior_quarantine.clear();
        cleared
    }

    #[cfg(test)]
    pub fn set_behavior_quarantine_for_tests(&mut self, quarantine: SimBehaviorQuarantineSnapshot) {
        self.behavior_quarantine
            .insert(quarantine.building, quarantine.reason);
    }

    pub fn apply_core_command_for_tests(
        &mut self,
        command: SimCommand,
    ) -> Result<(), SimCommandError> {
        let behavior_catalog = BehaviorCatalog::default();
        self.apply_command_with_behavior_runtime(
            command,
            BehaviorRuntime::new(&NOOP_BEHAVIOR_HOST, &behavior_catalog),
        )
    }

    pub fn apply_command_with_behavior_runtime(
        &mut self,
        command: SimCommand,
        behavior_runtime: BehaviorRuntime<'_>,
    ) -> Result<(), SimCommandError> {
        match command {
            SimCommand::PlaceBuilding {
                def_id,
                origin,
                direction,
                inserter_drop_direction,
            } => self.place_core_building(
                def_id,
                origin,
                direction,
                inserter_drop_direction,
                behavior_runtime.host(),
            ),
            SimCommand::PlaceUndergroundBelt {
                def_id,
                entrance,
                exit,
                direction,
            } => self.place_underground_belt_pair(def_id, entrance, exit, direction),
            SimCommand::PlaceUnderground {
                def_id,
                pos,
                direction,
            } => self.place_underground(def_id, pos, direction),
            SimCommand::RotateUnderground { pos } => self.rotate_underground(pos),
            SimCommand::PlaceBelt {
                pos,
                direction,
                input_direction,
                speed,
            } => self.place_belt_tile(pos, direction, input_direction, speed),
            SimCommand::SeedResource { pos, kind, amount } => {
                if amount > 0 {
                    self.resources.insert(pos, (kind, amount));
                }
                Ok(())
            }
            SimCommand::RemoveBuilding { pos } => {
                if let Some(building) = self.building_occupancy.get(&pos).copied() {
                    self.remove_core_building(
                        building,
                        behavior_runtime.host(),
                        behavior_runtime.catalog(),
                    )?;
                    return Ok(());
                }

                if self.occupied_tiles.remove(&pos).is_none() {
                    return Err(SimCommandError::MissingBuilding { pos });
                }
                self.topology_graph.remove_belt(pos);
                self.rebuild_transport_lines();
                Ok(())
            }
            SimCommand::ApplyBehaviorCommand { building, command } => {
                let Some(existing_building) = self.buildings.get(&building).cloned() else {
                    return Err(SimCommandError::MissingBuildingId { building });
                };
                let SimBuildingState::Behavior(state) = existing_building.state.clone() else {
                    return Err(SimCommandError::InvalidBehaviorCommand);
                };
                let behavior_id = Some(state.behavior_id.clone());
                let Some(def) = self.catalog.building_by_id(&existing_building.def_id) else {
                    return Err(SimCommandError::UnknownBuildingKind);
                };
                if !def.behavior.requires_behavior_host() {
                    return Err(SimCommandError::InvalidBehaviorCommand);
                }
                let config = def.behavior.config.clone();
                let inventories = self.take_building_inventories(building);
                let behavior_building = behavior_building(&existing_building);
                let output = behavior_runtime
                    .host()
                    .apply_behavior_command(BehaviorCommandInput {
                        catalog: behavior_runtime.catalog(),
                        building: &behavior_building,
                        config: &config,
                        state,
                        command,
                        inventories: behavior_inventories(inventories),
                    })
                    .map_err(|error| SimCommandError::BehaviorHostFailed {
                        building: Some(building),
                        phase: BehaviorHostFailurePhase::Command,
                        error,
                    })?;
                let effect_result = self.apply_behavior_effects(
                    building,
                    existing_building.origin,
                    behavior_id,
                    output.effects,
                    behavior_runtime.policy(),
                );
                if let Some(reason) = effect_result.rejection_reason {
                    return Err(SimCommandError::BehaviorEffectRejected { building, reason });
                }
                Ok(())
            }
            SimCommand::InsertIntoInventory {
                building,
                role,
                stack,
            } => self.insert_into_inventory_atomic(building, role, stack),
            SimCommand::TakeFromInventory {
                building,
                role,
                slot,
                amount,
            } => self
                .take_from_inventory_stack(building, role, slot, amount)
                .map(drop),
            SimCommand::InsertItemAtLineStart {
                line_index,
                lane,
                item,
            } => {
                if lane >= 2 {
                    return Err(SimCommandError::InvalidPort);
                }

                let Some(line_id) = self.transport.line_ids_sorted().nth(line_index) else {
                    return Err(SimCommandError::CapacityExceeded);
                };
                let Some(line) = self.transport.line_mut(line_id) else {
                    return Err(SimCommandError::CapacityExceeded);
                };

                *line.lane_mut(lane) = PackedItemStream::from_gaps(
                    vec![item],
                    DistanceUnits::new(64),
                    vec![],
                    DistanceUnits::new(128),
                );
                self.activation.wake_line(line_id);
                Ok(())
            }
            SimCommand::DropItemOnBeltTile {
                pos,
                lane,
                distance_numerator,
                distance_denominator,
                item,
            } => self.drop_item_on_belt_tile(
                pos,
                lane,
                distance_numerator,
                distance_denominator,
                item,
            ),
            SimCommand::CreateSource { .. } | SimCommand::CreateSink { .. } => Ok(()),
        }
    }

    pub fn with_catalog(catalog: CoreCatalog) -> Self {
        let character_inventory = SimCharacterInventory::from_catalog(&catalog);
        let player_inventory = SimInventory::from_def(catalog.player_inventory_def());
        let cursor_inventory = SimInventory::from_def(catalog.cursor_inventory_def());
        Self {
            catalog,
            character_inventory,
            loaded_containers: BTreeMap::new(),
            player_inventory,
            cursor_inventory,
            tick: SimTick::default(),
            time_of_day: TimeOfDay::default(),
            day_night_settings: DayNightSettings::default(),
            ids: IdAllocator::default(),
            terrain: BTreeMap::new(),
            surface_z: BTreeMap::new(),
            buildings: BTreeMap::new(),
            building_by_origin: BTreeMap::new(),
            building_occupancy: BTreeMap::new(),
            inventories: BTreeMap::new(),
            resources: BTreeMap::new(),
            occupied_tiles: BTreeMap::new(),
            topology_graph: TopologyGraph::default(),
            topology_revision_seen: 0,
            transport: TransportStorage::default(),
            energy: EnergyNetwork::default(),
            activation: ActivationScheduler::default(),
            metrics: SimMetricsSnapshot::default(),
            removed_item_drops: Vec::new(),
            surface_item_drops: Vec::new(),
            occupied_surface_tiles: BTreeSet::new(),
            behavior_quarantine: BTreeMap::new(),
            underground_corridors: BTreeMap::new(),
        }
    }

    pub fn player_inventory_snapshot(&self) -> crate::inventory::SimInventorySnapshot {
        self.player_inventory.snapshot()
    }

    pub fn character_container_sections(&self) -> Vec<CharacterContainerSection> {
        let rules = InventoryItemRules::from_catalog(&self.catalog);
        self.character_inventory
            .containers
            .iter()
            .map(|container| container.section_snapshot(&rules))
            .collect()
    }

    pub fn character_equipment(&self) -> Vec<CharacterEquipmentEntry> {
        self.character_inventory
            .equipment
            .values()
            .map(|item| CharacterEquipmentEntry {
                slot: item.slot.clone(),
                item: item.item,
            })
            .collect()
    }

    pub fn take_from_character_equipment_slot(&mut self, slot: &str) -> Option<InventorySlotEntry> {
        let slot_id = EquipmentSlotId(slot.to_string());
        let equipped = self.character_inventory.equipment.remove(&slot_id)?;
        let item = self.catalog.item(equipped.item)?;
        let equipment = item.equipment.as_ref()?;
        let provided_ids = equipment
            .provides_containers
            .iter()
            .map(|provided| provided.id.as_str())
            .collect::<BTreeSet<_>>();
        let mut loaded_sections = Vec::new();
        let mut index = 0;
        while index < self.character_inventory.containers.len() {
            if self.character_inventory.containers[index].source_slot != slot_id
                || !provided_ids.contains(self.character_inventory.containers[index].id.as_str())
            {
                index += 1;
                continue;
            }
            let container = self.character_inventory.containers.remove(index);
            if container
                .inventory
                .snapshot()
                .slots
                .iter()
                .any(Option::is_some)
            {
                loaded_sections.push(LoadedContainerSection {
                    container_id: container.id,
                    inventory: container.inventory,
                });
            }
        }

        let instance = if loaded_sections.is_empty() {
            None
        } else {
            let instance = self.ids.next_item_instance();
            self.loaded_containers.insert(
                instance,
                LoadedContainerInstance {
                    item: equipped.item,
                    containers: loaded_sections,
                },
            );
            Some(instance)
        };

        Some(InventorySlotEntry {
            stack: CoreItemStack {
                kind: equipped.item,
                amount: 1,
            },
            instance,
        })
    }

    pub fn equip_character_item(&mut self, entry: InventorySlotEntry) -> CharacterEquipResult {
        if entry.stack.amount != 1 {
            return CharacterEquipResult {
                equipped: None,
                replaced: None,
                rejected: Some(entry),
                rejection: Some(InventoryRejection::StackLimitExceeded),
            };
        }
        let Some(item) = self.catalog.item(entry.stack.kind) else {
            return CharacterEquipResult {
                equipped: None,
                replaced: None,
                rejected: Some(entry),
                rejection: Some(InventoryRejection::UnknownItem),
            };
        };
        let Some(equipment) = item.equipment.clone() else {
            return CharacterEquipResult {
                equipped: None,
                replaced: None,
                rejected: Some(entry),
                rejection: Some(InventoryRejection::MissingEquipmentSlot),
            };
        };
        let slot_id = EquipmentSlotId(equipment.slot.clone());
        let loaded_sections = match entry.instance {
            Some(instance) => {
                let Some(loaded) = self.loaded_containers.get(&instance) else {
                    return CharacterEquipResult {
                        equipped: None,
                        replaced: None,
                        rejected: Some(entry),
                        rejection: Some(InventoryRejection::MissingContainer),
                    };
                };
                if loaded.item != entry.stack.kind {
                    return CharacterEquipResult {
                        equipped: None,
                        replaced: None,
                        rejected: Some(entry),
                        rejection: Some(InventoryRejection::ItemNotAccepted),
                    };
                }
                loaded.containers.clone()
            }
            None => Vec::new(),
        };

        let replaced = self
            .character_inventory
            .equipment
            .contains_key(&slot_id)
            .then(|| self.take_from_character_equipment_slot(slot_id.as_str()))
            .flatten();

        if let Some(instance) = entry.instance {
            self.loaded_containers.remove(&instance);
        }
        self.character_inventory.equipment.insert(
            slot_id.clone(),
            EquippedItem {
                slot: slot_id.clone(),
                item: entry.stack.kind,
            },
        );

        for provided in &equipment.provides_containers {
            let mut container = container_from_def(provided, slot_id.clone(), entry.stack.kind);
            if let Some(loaded) = loaded_sections
                .iter()
                .find(|loaded| loaded.container_id.as_str() == provided.id.as_str())
            {
                container.inventory = loaded.inventory.clone();
            }
            self.character_inventory.containers.push(container);
        }
        self.character_inventory
            .containers
            .sort_by_key(|container| std::cmp::Reverse(container.pickup_priority));

        CharacterEquipResult {
            equipped: Some(entry.stack),
            replaced,
            rejected: None,
            rejection: None,
        }
    }

    pub fn character_container_slot(
        &self,
        container_id: &str,
        slot: usize,
    ) -> Option<CoreItemStack> {
        self.character_inventory
            .container(container_id)?
            .inventory
            .snapshot()
            .slots
            .get(slot)
            .copied()
            .flatten()
    }

    pub fn insert_into_character_container(
        &mut self,
        container_id: &str,
        stack: CoreItemStack,
        mode: InsertMode,
    ) -> InventoryInsertResult {
        let rules = InventoryItemRules::from_catalog(&self.catalog);
        let Some(container) = self.character_inventory.container_mut(container_id) else {
            return InventoryInsertResult {
                accepted: None,
                rejected: Some(stack),
                rejection: Some(InventoryRejection::MissingContainer),
            };
        };
        container.inventory.insert_with_mode(stack, mode, &rules)
    }

    pub fn insert_into_character_container_slot(
        &mut self,
        container_id: &str,
        slot: usize,
        stack: CoreItemStack,
        mode: InsertMode,
    ) -> InventoryInsertResult {
        let rules = InventoryItemRules::from_catalog(&self.catalog);
        let Some(container) = self.character_inventory.container_mut(container_id) else {
            return InventoryInsertResult {
                accepted: None,
                rejected: Some(stack),
                rejection: Some(InventoryRejection::MissingContainer),
            };
        };
        container
            .inventory
            .insert_into_slot_with_mode(slot, stack, mode, &rules)
    }

    pub fn take_from_character_container_slot(
        &mut self,
        container_id: &str,
        slot: usize,
        amount: u32,
    ) -> Option<CoreItemStack> {
        self.character_inventory
            .container_mut(container_id)?
            .inventory
            .take_slot_amount(slot, amount)
    }

    pub fn insert_loaded_item_into_character_container(
        &mut self,
        container_id: &str,
        entry: InventorySlotEntry,
        mode: InsertMode,
    ) -> InventoryInsertResult {
        if entry.instance.is_some() {
            return InventoryInsertResult {
                accepted: None,
                rejected: Some(entry.stack),
                rejection: Some(InventoryRejection::LoadedContainerNotAllowed),
            };
        }
        self.insert_into_character_container(container_id, entry.stack, mode)
    }

    pub fn route_stack_into_character(
        &mut self,
        stack: CoreItemStack,
        mode: InsertMode,
    ) -> CharacterRouteResult {
        let rules = InventoryItemRules::from_catalog(&self.catalog);
        let route = self
            .character_inventory
            .route_order_for_stack(stack, &rules);
        match mode {
            InsertMode::AtomicAllOrNothing => {
                let mut last = CharacterRouteResult {
                    accepted: None,
                    rejected: Some(stack),
                    rejection: Some(InventoryRejection::CapacityExceeded),
                    accepted_container: None,
                };
                for container_id in route {
                    let result = self.insert_into_character_container(&container_id, stack, mode);
                    if result.accepted.is_some() {
                        return CharacterRouteResult {
                            accepted_container: Some(container_id),
                            accepted: result.accepted,
                            rejected: result.rejected,
                            rejection: result.rejection,
                        };
                    }
                    last = result.into();
                }
                last
            }
            InsertMode::PartialFit => {
                let mut remaining = Some(stack);
                let mut accepted_amount = 0u32;
                let mut accepted_container = None;
                let mut last_rejection = Some(InventoryRejection::CapacityExceeded);

                for container_id in route {
                    let Some(stack_to_insert) = remaining else {
                        break;
                    };
                    let result = self.insert_into_character_container(
                        &container_id,
                        stack_to_insert,
                        InsertMode::PartialFit,
                    );
                    if let Some(accepted) = result.accepted {
                        accepted_amount = accepted_amount.saturating_add(accepted.amount);
                        accepted_container.get_or_insert(container_id);
                    }
                    remaining = result.rejected;
                    last_rejection = result.rejection;
                }

                CharacterRouteResult {
                    accepted: (accepted_amount > 0).then_some(CoreItemStack {
                        kind: stack.kind,
                        amount: accepted_amount,
                    }),
                    rejected: remaining,
                    rejection: remaining.and(last_rejection),
                    accepted_container,
                }
            }
        }
    }

    pub fn take_one_from_character_slot(
        &mut self,
        container_id: &str,
        slot: usize,
    ) -> Option<CoreItemStack> {
        self.character_inventory
            .container_mut(container_id)?
            .inventory
            .take_slot_amount(slot, 1)
    }

    pub fn split_half_from_character_slot(
        &mut self,
        container_id: &str,
        slot: usize,
    ) -> Option<CoreItemStack> {
        let amount = self.character_container_slot(container_id, slot)?.amount / 2;
        if amount == 0 {
            return None;
        }
        self.character_inventory
            .container_mut(container_id)?
            .inventory
            .take_slot_amount(slot, amount)
    }

    pub fn cursor_inventory_snapshot(&self) -> crate::inventory::SimInventorySnapshot {
        self.cursor_inventory.snapshot()
    }

    pub fn cursor_stack(&self) -> Option<CoreItemStack> {
        self.cursor_inventory_snapshot()
            .slots
            .into_iter()
            .flatten()
            .next()
    }

    pub fn cursor_entry(&self) -> Option<InventorySlotEntry> {
        let snapshot = self.cursor_inventory_snapshot();
        snapshot
            .slots
            .into_iter()
            .zip(snapshot.slot_instances)
            .find_map(|(slot, instance)| slot.map(|stack| InventorySlotEntry { stack, instance }))
    }

    pub fn take_from_cursor_inventory(&mut self, amount: u32) -> Option<CoreItemStack> {
        self.cursor_inventory.take_slot_amount(0, amount)
    }

    pub fn player_inventory_slot(&self, slot: usize) -> Option<CoreItemStack> {
        self.player_inventory_snapshot()
            .slots
            .get(slot)
            .copied()
            .flatten()
    }

    pub fn take_from_player_inventory_slot(
        &mut self,
        slot: usize,
        amount: u32,
    ) -> Option<CoreItemStack> {
        self.player_inventory.take_slot_amount(slot, amount)
    }

    pub fn insert_into_player_inventory_slot(
        &mut self,
        slot: usize,
        stack: CoreItemStack,
        mode: InsertMode,
    ) -> InventoryInsertResult {
        let Some(limited_stack) = self.limit_player_insert_stack(stack, mode) else {
            return InventoryInsertResult {
                accepted: None,
                rejected: Some(stack),
                rejection: Some(InventoryRejection::StackLimitExceeded),
            };
        };
        let item_rules = InventoryItemRules::from_catalog(&self.catalog);
        let mut result = self.player_inventory.insert_into_slot_with_mode(
            slot,
            limited_stack,
            mode,
            &item_rules,
        );
        self.restore_player_insert_rejection(stack, &mut result);
        result
    }

    pub fn take_all_from_cursor_inventory(&mut self) -> Option<CoreItemStack> {
        self.cursor_inventory.drain().into_iter().next()
    }

    pub fn take_all_from_cursor_inventory_entry(&mut self) -> Option<InventorySlotEntry> {
        self.cursor_inventory.drain_entries().into_iter().next()
    }

    pub fn set_cursor_inventory_stack(
        &mut self,
        stack: Option<CoreItemStack>,
    ) -> InventoryInsertResult {
        let mut inventory = SimInventory::from_def(self.catalog.cursor_inventory_def());
        let item_rules = InventoryItemRules::from_catalog(&self.catalog);
        let result = match stack {
            Some(stack) => {
                inventory.insert_with_mode(stack, InsertMode::AtomicAllOrNothing, &item_rules)
            }
            None => InventoryInsertResult {
                accepted: None,
                rejected: None,
                rejection: None,
            },
        };
        if result.rejected.is_none() {
            self.cursor_inventory = inventory;
        }
        result
    }

    pub fn set_cursor_inventory_entry(
        &mut self,
        entry: Option<InventorySlotEntry>,
    ) -> InventoryInsertResult {
        let mut inventory = SimInventory::from_def(self.catalog.cursor_inventory_def());
        let item_rules = InventoryItemRules::from_catalog(&self.catalog);
        let result = match entry {
            Some(entry) => {
                inventory.insert_entry_with_mode(entry, InsertMode::AtomicAllOrNothing, &item_rules)
            }
            None => InventoryInsertResult {
                accepted: None,
                rejected: None,
                rejection: None,
            },
        };
        if result.rejected.is_none() {
            self.cursor_inventory = inventory;
        }
        result
    }

    pub fn insert_into_player_inventory(
        &mut self,
        stack: CoreItemStack,
        mode: InsertMode,
    ) -> InventoryInsertResult {
        let Some(limited_stack) = self.limit_player_insert_stack(stack, mode) else {
            return InventoryInsertResult {
                accepted: None,
                rejected: Some(stack),
                rejection: Some(InventoryRejection::StackLimitExceeded),
            };
        };
        let item_rules = InventoryItemRules::from_catalog(&self.catalog);
        let mut result = self
            .player_inventory
            .insert_with_mode(limited_stack, mode, &item_rules);
        self.restore_player_insert_rejection(stack, &mut result);
        result
    }

    fn limit_player_insert_stack(
        &self,
        stack: CoreItemStack,
        mode: InsertMode,
    ) -> Option<CoreItemStack> {
        let stack_limit = self.player_inventory.stack_limit_for(stack.kind);
        let current_amount = self.player_inventory.count(stack.kind);
        if current_amount >= stack_limit {
            return None;
        }

        let allowed_amount = stack.amount.min(stack_limit - current_amount);
        if matches!(mode, InsertMode::AtomicAllOrNothing) && allowed_amount < stack.amount {
            return None;
        }
        Some(CoreItemStack {
            kind: stack.kind,
            amount: allowed_amount,
        })
    }

    fn restore_player_insert_rejection(
        &self,
        stack: CoreItemStack,
        result: &mut InventoryInsertResult,
    ) {
        let accepted_amount = result.accepted.map_or(0, |accepted| accepted.amount);
        let rejected_amount = stack.amount.saturating_sub(accepted_amount);
        if rejected_amount > 0 {
            result.rejected = Some(CoreItemStack {
                kind: stack.kind,
                amount: rejected_amount,
            });
            result
                .rejection
                .get_or_insert(InventoryRejection::StackLimitExceeded);
        }
    }

    pub fn install_catalog(&mut self, catalog: CoreCatalog) -> bool {
        if !self.is_empty_for_catalog_install() {
            return false;
        }
        self.character_inventory = SimCharacterInventory::from_catalog(&catalog);
        self.loaded_containers.clear();
        self.player_inventory = SimInventory::from_def(catalog.player_inventory_def());
        self.cursor_inventory = SimInventory::from_def(catalog.cursor_inventory_def());
        self.catalog = catalog;
        true
    }

    pub fn catalog(&self) -> &CoreCatalog {
        &self.catalog
    }

    pub fn terrain_at(&self, pos: TilePos) -> &CoreTerrainDef {
        self.terrain
            .get(&pos)
            .and_then(|id| self.catalog.terrain(id))
            .unwrap_or_else(|| self.catalog.default_terrain())
    }

    pub fn surface_z_at(&self, pos: TilePos) -> SurfaceZ {
        self.surface_z
            .get(&pos)
            .copied()
            .unwrap_or(DEFAULT_SURFACE_Z)
    }

    pub fn set_surface_z(&mut self, pos: TilePos, surface_z: SurfaceZ) {
        if surface_z == DEFAULT_SURFACE_Z {
            self.surface_z.remove(&pos);
        } else {
            self.surface_z.insert(pos, surface_z);
        }
    }

    pub fn set_terrain(
        &mut self,
        pos: TilePos,
        terrain_id: impl Into<String>,
    ) -> Result<(), String> {
        let terrain_id = terrain_id.into();
        if self.catalog.terrain(&terrain_id).is_none() {
            return Err(format!("unknown terrain '{terrain_id}'"));
        }
        if terrain_id == self.catalog.default_terrain().id.as_str() {
            self.terrain.remove(&pos);
        } else {
            self.terrain.insert(pos, terrain_id);
        }
        Ok(())
    }

    pub fn set_terrain_chunk(
        &mut self,
        tiles: impl IntoIterator<Item = (TilePos, String)>,
    ) -> Result<(), String> {
        for (pos, terrain_id) in tiles {
            self.set_terrain(pos, terrain_id)?;
        }
        Ok(())
    }

    pub fn apply_generated_region(&mut self, generated: &GeneratedMapRegion) -> Result<(), String> {
        self.set_terrain_chunk(
            generated
                .terrain_tiles
                .iter()
                .map(|tile| (tile.pos, tile.terrain_id.clone())),
        )?;
        for tile in &generated.terrain_tiles {
            self.set_surface_z(tile.pos, tile.surface_z);
        }
        for resource in &generated.resource_tiles {
            let Some(kind) = self.catalog.item_id_by_def_id(&resource.item_def_id) else {
                return Err(format!("unknown resource item '{}'", resource.item_def_id));
            };
            if resource.amount > 0 {
                self.resources.insert(resource.pos, (kind, resource.amount));
            }
        }
        Ok(())
    }

    pub fn terrain_tiles_in_rect(&self, min: TilePos, max: TilePos) -> Vec<(TilePos, String)> {
        let min = TilePos::new(min.x.min(max.x), min.y.min(max.y));
        let max = TilePos::new(min.x.max(max.x), min.y.max(max.y));
        let mut tiles = Vec::with_capacity(
            ((max.x - min.x + 1) as usize).saturating_mul((max.y - min.y + 1) as usize),
        );
        for y in min.y..=max.y {
            for x in min.x..=max.x {
                let pos = TilePos::new(x, y);
                tiles.push((pos, self.terrain_at(pos).id.clone()));
            }
        }
        tiles
    }

    pub fn building_footprint_for(
        &self,
        def_id: &str,
        origin: TilePos,
        direction: Direction,
    ) -> Result<Vec<TilePos>, SimCommandError> {
        let def = self
            .catalog
            .building_by_id(def_id)
            .ok_or(SimCommandError::UnknownBuildingKind)?;
        let offsets =
            footprint_offsets_for_direction(&def.footprint, def.rotate_footprint, direction);
        Ok(footprint_tiles(origin, &offsets))
    }

    pub fn can_place_core_building(
        &self,
        def_id: &str,
        origin: TilePos,
        direction: Direction,
    ) -> Result<(), SimCommandError> {
        let def = self
            .catalog
            .building_by_id(def_id)
            .ok_or(SimCommandError::UnknownBuildingKind)?;
        if matches!(&def.behavior.driver, CoreBuildingDriver::Underground { .. }) {
            return Err(SimCommandError::InvalidPort);
        }

        let offsets =
            footprint_offsets_for_direction(&def.footprint, def.rotate_footprint, direction);
        let footprint = footprint_tiles(origin, &offsets);
        for &pos in &footprint {
            self.ensure_buildable(pos)?;
        }
        self.ensure_flat_footprint(origin, &footprint)?;
        for &pos in &footprint {
            if self.building_occupancy.contains_key(&pos) || self.occupied_tiles.contains_key(&pos)
            {
                return Err(SimCommandError::OccupiedTile { pos });
            }
        }
        if extractor_requires_resource_footprint(def)
            && !self.footprint_has_matching_resource(def, &footprint)
        {
            return Err(SimCommandError::InvalidRecipe);
        }
        Ok(())
    }

    fn footprint_has_matching_resource(
        &self,
        def: &CoreBuildingDef,
        footprint: &[TilePos],
    ) -> bool {
        let accepted_resources = extractor_resource_kinds(def);
        footprint.iter().any(|pos| {
            self.resources.get(pos).is_some_and(|(kind, amount)| {
                *amount > 0 && (accepted_resources.is_empty() || accepted_resources.contains(kind))
            })
        })
    }

    pub fn resource_tiles_in_rect(
        &self,
        min: TilePos,
        max: TilePos,
    ) -> Vec<(TilePos, String, u32)> {
        let min = TilePos::new(min.x.min(max.x), min.y.min(max.y));
        let max = TilePos::new(min.x.max(max.x), min.y.max(max.y));
        self.resources
            .iter()
            .filter_map(|(pos, (kind, amount))| {
                if pos.x < min.x || pos.x > max.x || pos.y < min.y || pos.y > max.y {
                    return None;
                }
                let def_id = self.catalog.def_id_by_item_id(*kind)?;
                Some((*pos, def_id.to_string(), *amount))
            })
            .collect()
    }

    pub fn is_empty_for_catalog_install(&self) -> bool {
        self.buildings.is_empty()
            && self.building_by_origin.is_empty()
            && self.building_occupancy.is_empty()
            && self.inventories.is_empty()
            && self.terrain.is_empty()
            && self.surface_z.is_empty()
            && self.occupied_tiles.is_empty()
            && self.loaded_containers.is_empty()
            && self.character_inventory == SimCharacterInventory::from_catalog(&self.catalog)
            && self.player_inventory == SimInventory::from_def(self.catalog.player_inventory_def())
            && self.cursor_inventory == SimInventory::from_def(self.catalog.cursor_inventory_def())
    }

    pub fn building_snapshots(&self) -> Vec<SimBuildingSnapshot> {
        self.buildings
            .values()
            .map(|building| SimBuildingSnapshot {
                id: building.id,
                def_id: building.def_id.clone(),
                kind: building.kind,
                origin: building.origin,
                surface_z: building.surface_z,
                direction: building.direction,
                state: building.state.clone(),
                inventories: building
                    .inventories
                    .iter()
                    .filter_map(|id| self.inventories.get(id))
                    .map(|record| record.inventory.snapshot())
                    .collect(),
            })
            .collect()
    }

    pub fn belt_tile_at(&self, pos: TilePos) -> Option<BeltTile> {
        self.topology_graph.belt(pos)
    }

    pub fn is_occupied_for_tests(&self, pos: TilePos) -> bool {
        self.building_occupancy.contains_key(&pos)
    }
    pub fn building_id_at_origin_for_tests(&self, pos: TilePos) -> Option<BuildingId> {
        self.building_by_origin.get(&pos).copied()
    }

    #[cfg(any(test, debug_assertions))]
    pub fn building_at(&self, pos: TilePos) -> Option<&SimBuilding> {
        self.building_occupancy
            .get(&pos)
            .and_then(|id| self.buildings.get(id))
    }
    pub fn seed_resource_for_tests(&mut self, pos: TilePos, kind: ItemKindId, amount: u32) {
        self.resources.insert(pos, (kind, amount));
    }
    pub fn resource_amount_for_tests(&self, pos: TilePos) -> Option<u32> {
        self.resources.get(&pos).map(|(_, amount)| *amount)
    }

    pub fn tick_core_only_for_tests(&mut self) -> SimTickOutput {
        let behavior_catalog = BehaviorCatalog::default();
        self.tick_with_behavior_runtime(BehaviorRuntime::new(
            &NOOP_BEHAVIOR_HOST,
            &behavior_catalog,
        ))
    }

    pub fn tick_with_behavior_runtime(
        &mut self,
        behavior_runtime: BehaviorRuntime<'_>,
    ) -> SimTickOutput {
        self.tick.advance();
        self.time_of_day.advance_one_tick();
        self.rebuild_dirty_energy_topology();
        let solar_factor = self.solar_factor();
        crate::energy::solver::solve_energy_with_solar_factor(
            &mut self.energy,
            &self.catalog,
            solar_factor,
        );

        let mut diff = SimDiff {
            topology_revision: self.topology_graph.revision(),
            ..SimDiff::default()
        };
        let mut metrics = SimMetricsSnapshot {
            sim_ticks: self.tick.raw(),
            active_interactions: self.transport.interactions_sorted().count(),
            dirty_chunks: self
                .occupied_tiles
                .keys()
                .map(|pos| pos.chunk_pos())
                .collect::<BTreeSet<_>>()
                .len(),
            ..SimMetricsSnapshot::default()
        };
        let mut resource_depletions = Vec::new();
        let mut behavior_effect_reports = Vec::new();

        let line_ids = self.activation.active_lines().collect::<Vec<_>>();
        for line_id in line_ids {
            let Some(line) = self.transport.line_mut(line_id) else {
                continue;
            };

            metrics.simulated_items += line.lane(0).item_count() + line.lane(1).item_count();
            if line.sleeping() {
                metrics.sleeping_lines += 1;
            } else {
                metrics.active_lines += 1;
            }

            let previous_revision = line.revision();
            let report = line.advance();
            metrics.items_scanned += report.items_scanned;
            if line.sleeping() {
                self.activation.sleep_line(line_id);
            }
            if line.revision() != previous_revision {
                diff.changed_lines.push(line_id);
            }
        }

        let changed_lines_before_interactions = diff.changed_lines.len();
        self.process_belt_interactions(&mut diff);
        if diff.changed_lines.len() != changed_lines_before_interactions {
            self.refresh_transport_metrics(&mut metrics);
        }

        let changed_lines_before_inserters = diff.changed_lines.len();
        self.tick_inserters(
            &mut metrics,
            &mut diff,
            behavior_runtime.host(),
            behavior_runtime.catalog(),
        );
        if diff.changed_lines.len() != changed_lines_before_inserters {
            self.refresh_transport_metrics(&mut metrics);
        }
        self.tick_behaviors(
            &mut metrics,
            &mut diff,
            &mut resource_depletions,
            &mut behavior_effect_reports,
            behavior_runtime.host(),
            behavior_runtime.catalog(),
            behavior_runtime.policy(),
        );
        record_behavior_effect_metrics(
            &mut metrics,
            &behavior_effect_reports,
            self.behavior_quarantine.len(),
        );

        diff.sort_and_dedup();
        self.topology_revision_seen = diff.topology_revision;
        self.metrics = metrics;

        SimTickOutput {
            tick: self.tick,
            diff,
            metrics,
            removal_drops: take_uninstanced_removal_drops(&mut self.removed_item_drops),
            surface_drops: take_uninstanced_surface_drops(&mut self.surface_item_drops),
            resource_depletions,
            behavior_effect_reports,
        }
    }

    pub fn insert_many_at_line_start(
        &mut self,
        line_index: usize,
        lane: usize,
        item: ItemKindId,
        count: usize,
    ) -> Result<(), SimCommandError> {
        if lane >= 2 {
            return Err(SimCommandError::InvalidPort);
        }

        let Some(line_id) = self.transport.line_ids_sorted().nth(line_index) else {
            return Err(SimCommandError::CapacityExceeded);
        };
        let Some(line) = self.transport.line_mut(line_id) else {
            return Err(SimCommandError::CapacityExceeded);
        };

        let items = vec![item; count];
        let gaps = if count == 0 {
            Vec::new()
        } else {
            vec![MIN_ITEM_SPACING; count.saturating_sub(1)]
        };
        *line.lane_mut(lane) = PackedItemStream::from_gaps(
            items,
            DistanceUnits::new(64),
            gaps,
            DistanceUnits::new(256),
        );
        self.activation.wake_line(line_id);
        Ok(())
    }

    pub fn drop_item_on_belt_tile(
        &mut self,
        pos: TilePos,
        lane: usize,
        distance_numerator: u16,
        distance_denominator: u16,
        item: ItemKindId,
    ) -> Result<(), SimCommandError> {
        if lane >= 2 || distance_denominator == 0 {
            return Err(SimCommandError::InvalidPort);
        }
        let Some((line_id, _slot, min_distance, max_distance)) = self.line_window_for_tile(pos)
        else {
            return Err(SimCommandError::MissingBuilding { pos });
        };
        let span = max_distance.raw() - min_distance.raw();
        let numerator = i32::from(distance_numerator.min(distance_denominator));
        let denominator = i32::from(distance_denominator);
        let Some(line) = self.transport.line(line_id) else {
            return Err(SimCommandError::CapacityExceeded);
        };
        let distance = snap_belt_insert_distance(
            DistanceUnits::new(
                min_distance.raw() + (span * (denominator - numerator) / denominator),
            ),
            line.path().total_len(),
        );
        let Some(line) = self.transport.line_mut(line_id) else {
            return Err(SimCommandError::CapacityExceeded);
        };
        if !line.insert_one_with_nudge_in_window(lane, item, distance) {
            return Err(SimCommandError::CapacityExceeded);
        }
        self.activation.wake_line(line_id);
        Ok(())
    }

    pub fn first_item_stack_on_belt_tile(&self, pos: TilePos) -> Option<CoreItemStack> {
        let item = self.first_item_kind_on_belt_tile(pos)?;
        Some(CoreItemStack {
            kind: item,
            amount: 1,
        })
    }

    fn first_item_kind_on_belt_tile(&self, pos: TilePos) -> Option<ItemKindId> {
        if let Some(position) = self.first_underground_item_position_on_tile(pos) {
            return Some(position.item);
        }
        if let Some(position) = self.first_splitter_item_position_on_tile(pos) {
            return Some(position.item);
        }
        let (_line_id, _lane, _distance, item) = self.first_belt_item_position_on_tile(pos)?;
        Some(item)
    }

    pub fn take_item_from_belt_tile(
        &mut self,
        pos: TilePos,
    ) -> Result<CoreItemStack, SimCommandError> {
        if let Some(position) = self.first_underground_item_position_on_tile(pos) {
            let Some(item) =
                self.remove_underground_item(position.node, position.phase, position.lane)
            else {
                return Err(SimCommandError::MissingBuilding { pos });
            };
            return Ok(CoreItemStack {
                kind: item,
                amount: 1,
            });
        }

        if let Some(position) = self.first_splitter_item_position_on_tile(pos) {
            let Some(item) = self.remove_splitter_item(
                position.node,
                position.phase,
                position.channel,
                position.lane,
            ) else {
                return Err(SimCommandError::MissingBuilding { pos });
            };
            return Ok(CoreItemStack {
                kind: item,
                amount: 1,
            });
        }

        let Some((line_id, lane, distance, _item)) = self.first_belt_item_position_on_tile(pos)
        else {
            return Err(SimCommandError::MissingBuilding { pos });
        };
        let Some(line) = self.transport.line_mut(line_id) else {
            return Err(SimCommandError::MissingBuilding { pos });
        };
        let Some(item) = line.remove_one_at_distance(lane, distance) else {
            return Err(SimCommandError::MissingBuilding { pos });
        };
        self.activation.wake_line(line_id);
        Ok(CoreItemStack {
            kind: item,
            amount: 1,
        })
    }

    fn first_belt_item_position_on_tile(
        &self,
        pos: TilePos,
    ) -> Option<(LineId, usize, DistanceUnits, ItemKindId)> {
        let (line_id, _slot, min_distance, max_distance) = self.line_window_for_tile(pos)?;
        let line = self.transport.line(line_id)?;
        (0..2)
            .filter_map(|lane| {
                line.first_in_window(lane, min_distance, max_distance)
                    .map(|position| (line_id, lane, position.distance, position.item))
            })
            .min_by_key(|(_, _, distance, _)| *distance)
    }

    fn first_underground_item_position_on_tile(
        &self,
        pos: TilePos,
    ) -> Option<UndergroundItemPosition> {
        self.underground_item_positions_on_tile(pos)
            .into_iter()
            .max_by_key(|position| {
                (
                    position.progress,
                    match position.phase {
                        UndergroundEndpointPhase::Exit => 1,
                        UndergroundEndpointPhase::Entrance => 0,
                    },
                    std::cmp::Reverse(position.lane),
                )
            })
    }

    fn underground_item_positions_on_tile(&self, pos: TilePos) -> Vec<UndergroundItemPosition> {
        self.underground_item_positions_matching(|tile| tile == pos)
    }

    fn underground_item_positions_for_bounds(
        &self,
        bounds: VisibleTileBounds,
    ) -> Vec<UndergroundItemPosition> {
        self.underground_item_positions_matching(|tile| bounds.contains(tile))
    }

    fn underground_item_positions_matching(
        &self,
        mut tile_matches: impl FnMut(TilePos) -> bool,
    ) -> Vec<UndergroundItemPosition> {
        let mut positions = Vec::new();
        for node in self.transport.nodes_sorted() {
            if node.kind != crate::transport::node::TransportNodeKind::Underground {
                continue;
            }
            let crate::transport::node::TransportNodeRuntime::Underground(runtime) = &node.runtime
            else {
                continue;
            };
            let Some(direction) = node.direction else {
                continue;
            };
            let Some(entrance) = node.input_ports().next().map(|port| port.tile) else {
                continue;
            };
            let Some(exit) = node.output_ports().next().map(|port| port.tile) else {
                continue;
            };
            let entrance_matches = tile_matches(entrance);
            let exit_matches = tile_matches(exit);
            if !entrance_matches && !exit_matches {
                continue;
            }

            for item in &runtime.items {
                if item.lane >= 2 {
                    continue;
                }
                if entrance_matches
                    && underground_endpoint_phase(
                        runtime.distance,
                        item.progress,
                        UndergroundEndpointPhase::Entrance,
                    )
                {
                    positions.push(UndergroundItemPosition {
                        node: node.id,
                        tile: entrance,
                        phase: UndergroundEndpointPhase::Entrance,
                        lane: item.lane,
                        progress: item.progress,
                        distance: runtime.distance,
                        direction,
                        item: item.item,
                    });
                }
                if exit_matches
                    && underground_endpoint_phase(
                        runtime.distance,
                        item.progress,
                        UndergroundEndpointPhase::Exit,
                    )
                {
                    positions.push(UndergroundItemPosition {
                        node: node.id,
                        tile: exit,
                        phase: UndergroundEndpointPhase::Exit,
                        lane: item.lane,
                        progress: item.progress,
                        distance: runtime.distance,
                        direction,
                        item: item.item,
                    });
                }
            }
        }
        positions
    }

    fn first_splitter_item_position_on_tile(&self, pos: TilePos) -> Option<SplitterItemPosition> {
        self.splitter_item_positions_on_tile(pos)
            .into_iter()
            .max_by_key(|position| {
                (
                    position.progress,
                    match position.phase {
                        VisibleSplitterItemPhase::Egress => 1,
                        VisibleSplitterItemPhase::Ingress => 0,
                    },
                    std::cmp::Reverse(position.lane),
                )
            })
    }

    fn splitter_item_positions_on_tile(&self, pos: TilePos) -> Vec<SplitterItemPosition> {
        let mut positions = Vec::new();
        for node in self.transport.nodes_sorted() {
            if node.kind != crate::transport::node::TransportNodeKind::Splitter2x1 {
                continue;
            }
            let Some(direction) = node.direction else {
                continue;
            };
            let crate::transport::node::TransportNodeRuntime::Splitter(runtime) = &node.runtime
            else {
                continue;
            };
            let channel_tiles = splitter_internal_channel_tiles(node.sort_tile, direction);
            for item in &runtime.ingress_items {
                let Some(&tile) = channel_tiles.get(item.input_channel) else {
                    continue;
                };
                if tile == pos {
                    positions.push(SplitterItemPosition {
                        node: node.id,
                        tile,
                        phase: VisibleSplitterItemPhase::Ingress,
                        channel: item.input_channel,
                        lane: item.lane,
                        progress: item.progress,
                        item: item.item,
                    });
                }
            }
            for item in &runtime.egress_items {
                let Some(&tile) = channel_tiles.get(item.output_channel) else {
                    continue;
                };
                if tile == pos {
                    positions.push(SplitterItemPosition {
                        node: node.id,
                        tile,
                        phase: VisibleSplitterItemPhase::Egress,
                        channel: item.output_channel,
                        lane: item.lane,
                        progress: item.progress,
                        item: item.item,
                    });
                }
            }
        }
        positions
    }

    fn remove_underground_item(
        &mut self,
        node: TransportNodeId,
        phase: UndergroundEndpointPhase,
        lane: usize,
    ) -> Option<ItemKindId> {
        if lane >= 2 {
            return None;
        }
        let wake_lines = self
            .transport
            .nodes_sorted()
            .find(|candidate| candidate.id == node)
            .map(|candidate| {
                candidate
                    .input_ports()
                    .chain(candidate.output_ports())
                    .map(|port| port.line)
                    .collect::<Vec<_>>()
            })
            .unwrap_or_default();
        let item = {
            let runtime = self.transport.underground_runtime_mut(node)?;
            let distance = runtime.distance;
            remove_frontmost_matching(
                &mut runtime.items,
                |item| {
                    item.lane == lane && underground_endpoint_phase(distance, item.progress, phase)
                },
                |item| item.progress,
                |item| item.item,
            )?
        };
        for line in wake_lines {
            self.activation.wake_line(line);
        }
        Some(item)
    }

    fn remove_splitter_item(
        &mut self,
        node: TransportNodeId,
        phase: VisibleSplitterItemPhase,
        channel: usize,
        lane: usize,
    ) -> Option<ItemKindId> {
        let (sort_tile, wake_lines) = self
            .transport
            .nodes_sorted()
            .find(|candidate| candidate.id == node)
            .map(|candidate| {
                (
                    candidate.sort_tile,
                    candidate
                        .input_ports()
                        .chain(candidate.output_ports())
                        .map(|port| port.line)
                        .collect::<Vec<_>>(),
                )
            })
            .unwrap_or((TilePos::new(0, 0), Vec::new()));
        let (item, runtime) = {
            let runtime = self.transport.splitter_runtime_mut(node)?;
            let item = match phase {
                VisibleSplitterItemPhase::Ingress => remove_frontmost_matching(
                    &mut runtime.ingress_items,
                    |item| item.input_channel == channel && item.lane == lane,
                    |item| item.progress,
                    |item| item.item,
                ),
                VisibleSplitterItemPhase::Egress => remove_frontmost_matching(
                    &mut runtime.egress_items,
                    |item| item.output_channel == channel && item.lane == lane,
                    |item| item.progress,
                    |item| item.item,
                ),
            }?;
            (item, runtime.clone())
        };
        self.sync_splitter_building_runtime(sort_tile, runtime);
        for line in wake_lines {
            self.activation.wake_line(line);
        }
        Some(item)
    }

    fn sync_splitter_building_runtime(
        &mut self,
        origin: TilePos,
        runtime: crate::transport::node::SplitterRuntime,
    ) {
        if let Some(building_id) = self.building_by_origin.get(&origin).copied()
            && let Some(building) = self.buildings.get_mut(&building_id)
            && self
                .catalog
                .building_by_id(&building.def_id)
                .is_some_and(|def| {
                    matches!(def.behavior.driver, CoreBuildingDriver::Splitter { .. })
                })
        {
            building.state = SimBuildingState::Splitter(runtime);
        }
    }

    pub fn insert_many_at_line_start_for_tests(
        &mut self,
        line_index: usize,
        lane: usize,
        item: ItemKindId,
        count: usize,
    ) -> Result<(), SimCommandError> {
        self.insert_many_at_line_start(line_index, lane, item, count)
    }
    pub fn insert_into_inventory_for_tests(
        &mut self,
        building: BuildingId,
        role: CoreInventoryRole,
        stack: crate::catalog::CoreItemStack,
    ) -> Result<(), SimCommandError> {
        self.insert_into_inventory_atomic(building, role, stack)
    }

    pub fn insert_loaded_item_into_building_inventory_for_tests(
        &mut self,
        building: BuildingId,
        role: CoreInventoryRole,
        entry: InventorySlotEntry,
        mode: InsertMode,
    ) -> InventoryInsertResult {
        let Some(building) = self.buildings.get(&building) else {
            return InventoryInsertResult {
                accepted: None,
                rejected: Some(entry.stack),
                rejection: Some(InventoryRejection::MissingInventory),
            };
        };
        let Some(inventory_id) = building.inventories.iter().copied().find(|id| {
            self.inventories
                .get(id)
                .is_some_and(|record| record.inventory.role() == role)
        }) else {
            return InventoryInsertResult {
                accepted: None,
                rejected: Some(entry.stack),
                rejection: Some(InventoryRejection::MissingInventory),
            };
        };
        let item_rules = InventoryItemRules::from_catalog(&self.catalog);
        let Some(record) = self.inventories.get_mut(&inventory_id) else {
            return InventoryInsertResult {
                accepted: None,
                rejected: Some(entry.stack),
                rejection: Some(InventoryRejection::MissingInventory),
            };
        };
        record
            .inventory
            .insert_entry_with_mode(entry, mode, &item_rules)
    }

    pub fn create_loaded_container_item_for_tests(
        &mut self,
        item_def_id: &str,
        container_id: &str,
        contents: Vec<CoreItemStack>,
    ) -> InventorySlotEntry {
        let item = self
            .catalog
            .item_id_by_def_id(item_def_id)
            .unwrap_or_else(|| panic!("missing test item '{item_def_id}'"));
        let policy = self
            .catalog
            .item(item)
            .and_then(|item| item.equipment.as_ref())
            .and_then(|equipment| equipment.provides_containers.first())
            .map(|container| container.policy.clone())
            .unwrap_or_else(|| panic!("test item '{item_def_id}' does not provide a container"));
        let mut inventory = SimInventory::from_container_policy(&policy);
        let rules = InventoryItemRules::from_catalog(&self.catalog);
        for stack in contents {
            let result = inventory.insert_with_mode(stack, InsertMode::AtomicAllOrNothing, &rules);
            assert_eq!(result.rejected, None);
        }

        let instance = self.ids.next_item_instance();
        self.loaded_containers.insert(
            instance,
            LoadedContainerInstance {
                item,
                containers: vec![LoadedContainerSection {
                    container_id: CharacterContainerId::new(container_id),
                    inventory,
                }],
            },
        );
        InventorySlotEntry {
            stack: CoreItemStack {
                kind: item,
                amount: 1,
            },
            instance: Some(instance),
        }
    }

    pub fn drop_loaded_container_on_surface_for_tests(
        &mut self,
        origin: TilePos,
        entry: InventorySlotEntry,
    ) {
        assert!(entry.instance.is_some());
        self.push_surface_item_drop_entry(origin, entry);
    }

    pub fn loaded_container_contents_for_tests(
        &self,
        instance: ItemInstanceId,
    ) -> Vec<CoreItemStack> {
        self.loaded_containers
            .get(&instance)
            .map(|container| {
                container
                    .containers
                    .iter()
                    .flat_map(|section| section.inventory.snapshot().slots.into_iter().flatten())
                    .collect()
            })
            .unwrap_or_default()
    }

    pub fn insert_into_player_inventory_for_tests(
        &mut self,
        stack: CoreItemStack,
    ) -> Result<(), SimCommandError> {
        let result = self.insert_into_player_inventory(stack, InsertMode::AtomicAllOrNothing);
        if result.rejected.is_some() {
            return Err(SimCommandError::CapacityExceeded);
        }
        Ok(())
    }

    pub fn set_cursor_stack_for_tests(
        &mut self,
        stack: CoreItemStack,
    ) -> Result<(), SimCommandError> {
        let mut inventory = SimInventory::from_def(self.catalog.cursor_inventory_def());
        let item_rules = InventoryItemRules::from_catalog(&self.catalog);
        let result = inventory.insert_with_mode(stack, InsertMode::AtomicAllOrNothing, &item_rules);
        if result.rejected.is_some() {
            return Err(SimCommandError::CapacityExceeded);
        }
        self.cursor_inventory = inventory;
        Ok(())
    }
    pub fn removed_item_drops_for_tests(&self) -> Vec<CoreItemStack> {
        self.removed_item_drops
            .iter()
            .map(|drop| drop.stack)
            .collect()
    }
    pub fn removed_item_drop_records_for_tests(&self) -> &[CoreRemovalDrop] {
        &self.removed_item_drops
    }

    pub fn surface_item_drops_snapshot(&self) -> Vec<CoreSurfaceDrop> {
        self.surface_item_drops.clone()
    }

    pub fn surface_item_drops_snapshot_for_tests(&self) -> Vec<CoreSurfaceDrop> {
        self.surface_item_drops_snapshot()
    }

    pub fn rebuild_energy_topology_for_tests(&mut self) {
        crate::energy::topology::rebuild_energy_topology(
            &mut self.energy,
            &self.catalog,
            &self.buildings,
        );
    }

    pub fn energy_view(&self) -> EnergyView {
        EnergyView::from_network(&self.energy)
    }

    #[cfg(test)]
    pub fn energy_view_for_tests(&self) -> EnergyView {
        self.energy_view()
    }

    pub fn last_metrics(&self) -> SimMetricsSnapshot {
        self.metrics
    }

    pub fn take_pending_item_drops(&mut self) -> (Vec<CoreRemovalDrop>, Vec<CoreSurfaceDrop>) {
        (
            take_uninstanced_removal_drops(&mut self.removed_item_drops),
            take_uninstanced_surface_drops(&mut self.surface_item_drops),
        )
    }

    pub fn drop_inventory_entry_on_surface(&mut self, origin: TilePos, entry: InventorySlotEntry) {
        self.push_surface_item_drop_entry(origin, entry);
    }

    pub fn take_loaded_surface_item_drop_at(
        &mut self,
        origin: TilePos,
        instance: ItemInstanceId,
    ) -> Option<InventorySlotEntry> {
        let index = self
            .surface_item_drops
            .iter()
            .position(|drop| drop.origin == origin && drop.instance == Some(instance))?;
        let drop = self.surface_item_drops.remove(index);
        if !self
            .surface_item_drops
            .iter()
            .any(|remaining| remaining.origin == origin)
        {
            self.occupied_surface_tiles.remove(&origin);
        }
        Some(InventorySlotEntry {
            stack: drop.stack,
            instance: drop.instance,
        })
    }

    fn push_removed_item_drop(&mut self, origin: TilePos, stack: CoreItemStack) {
        self.push_removed_item_drop_entry(
            origin,
            InventorySlotEntry {
                stack,
                instance: None,
            },
        );
    }

    fn push_surface_item_drop(&mut self, origin: TilePos, stack: CoreItemStack) {
        self.push_surface_item_drop_entry(
            origin,
            InventorySlotEntry {
                stack,
                instance: None,
            },
        );
    }

    fn push_removed_item_drop_entry(&mut self, origin: TilePos, entry: InventorySlotEntry) {
        self.removed_item_drops.push(CoreRemovalDrop {
            origin,
            stack: entry.stack,
            instance: entry.instance,
        });
    }

    fn push_surface_item_drop_entry(&mut self, origin: TilePos, entry: InventorySlotEntry) {
        self.occupied_surface_tiles.insert(origin);
        self.surface_item_drops.push(CoreSurfaceDrop {
            origin,
            stack: entry.stack,
            instance: entry.instance,
        });
    }

    fn ensure_buildable(&self, pos: TilePos) -> Result<(), SimCommandError> {
        if self.terrain_at(pos).buildable {
            Ok(())
        } else {
            Err(SimCommandError::UnbuildableTile { pos })
        }
    }

    fn ensure_flat_footprint(
        &self,
        origin: TilePos,
        footprint: &[TilePos],
    ) -> Result<(), SimCommandError> {
        let Some(first) = footprint.first().copied() else {
            return Ok(());
        };
        let expected_z = self.surface_z_at(first);
        for &pos in footprint {
            let found_z = self.surface_z_at(pos);
            if found_z != expected_z {
                return Err(SimCommandError::UnevenTerrain {
                    origin,
                    pos,
                    expected_z,
                    found_z,
                });
            }
        }
        Ok(())
    }

    pub fn set_occupied_surface_tiles(&mut self, tiles: impl IntoIterator<Item = TilePos>) {
        self.occupied_surface_tiles = tiles.into_iter().collect();
        self.occupied_surface_tiles
            .extend(self.surface_item_drops.iter().map(|drop| drop.origin));
    }

    fn extend_removed_item_drops(
        &mut self,
        origin: TilePos,
        stacks: impl IntoIterator<Item = CoreItemStack>,
    ) {
        let drops = stacks.into_iter().map(|stack| CoreRemovalDrop {
            origin,
            stack,
            instance: None,
        });
        self.removed_item_drops.extend(drops);
    }

    fn extend_removed_item_drop_entries(
        &mut self,
        origin: TilePos,
        entries: impl IntoIterator<Item = InventorySlotEntry>,
    ) {
        let drops = entries.into_iter().map(|entry| CoreRemovalDrop {
            origin,
            stack: entry.stack,
            instance: entry.instance,
        });
        self.removed_item_drops.extend(drops);
    }

    #[cfg(test)]
    fn apply_behavior_effects_for_tests(
        &mut self,
        building: BuildingId,
        effects: Vec<BehaviorEffect>,
    ) -> BehaviorEffectApplyResult {
        let building_ref = self
            .buildings
            .get(&building)
            .expect("test building id should exist");
        let origin = building_ref.origin;
        let behavior_id = building_ref
            .state
            .behavior_state()
            .map(|state| state.behavior_id.clone());

        self.apply_behavior_effects(
            building,
            origin,
            behavior_id,
            effects,
            BehaviorRuntimePolicy {
                effect_rejection: BehaviorEffectRejectionPolicy::ReportOnly,
            },
        )
    }

    pub(super) fn apply_behavior_effects(
        &mut self,
        building: BuildingId,
        origin: TilePos,
        behavior_id: Option<BehaviorId>,
        effects: Vec<BehaviorEffect>,
        policy: BehaviorRuntimePolicy,
    ) -> BehaviorEffectApplyResult {
        let effects = canonical_behavior_effects(effects);
        if let Err(rejection) = self.validate_behavior_effects(building, &effects) {
            let reason = rejection.reason.clone();
            let rejected_effect = RejectedBehaviorEffect {
                effect: rejection.effect.clone(),
                reason: rejection.reason.clone(),
            };
            let application = match policy.effect_rejection {
                BehaviorEffectRejectionPolicy::Panic => {
                    panic!("behavior effect rejected: {rejection:?}");
                }
                BehaviorEffectRejectionPolicy::ReportOnly => BehaviorEffectApplication::Rejected {
                    effects: vec![rejected_effect],
                },
                BehaviorEffectRejectionPolicy::QuarantineInstance => {
                    self.behavior_quarantine.insert(building, reason.clone());
                    BehaviorEffectApplication::Quarantined {
                        effects: vec![rejected_effect],
                    }
                }
            };
            return BehaviorEffectApplyResult {
                resource_depletions: Vec::new(),
                report: BehaviorEffectReport {
                    building,
                    origin,
                    behavior_id,
                    application,
                },
                rejection_reason: Some(reason),
            };
        }

        let applied_effects = effects
            .iter()
            .cloned()
            .map(|effect| AppliedBehaviorEffect { effect })
            .collect();
        let mut resource_depletions = Vec::new();
        for effect in effects {
            match effect {
                BehaviorEffect::SetState(state) => {
                    self.replace_behavior_state(building, state);
                }
                BehaviorEffect::SetPowerOutput { max_output } => {
                    let output = crate::energy::PowerUnits::new(i64::from(max_output));
                    self.energy.set_source_output(building, output);
                }
                BehaviorEffect::TakeInventory { role, stack } => {
                    let _ = self.remove_from_inventory_atomic(
                        building,
                        core_inventory_role(role),
                        core_stack(stack),
                    );
                }
                BehaviorEffect::InsertInventory { role, stack } => {
                    let _ = self.insert_into_inventory_atomic(
                        building,
                        core_inventory_role(role),
                        core_stack(stack),
                    );
                }
                BehaviorEffect::DrainInventory { role } => {
                    let entries = self.drain_inventory_role(building, core_inventory_role(role));
                    self.extend_removed_item_drop_entries(origin, entries);
                }
                BehaviorEffect::DepleteResource { pos } => {
                    if let Some(depletion) = self.decrement_resource(core_tile_pos(pos)) {
                        resource_depletions.push(depletion);
                    }
                }
                BehaviorEffect::DropItems { stacks } => {
                    self.extend_removed_item_drops(origin, core_stacks(stacks));
                }
            }
        }
        BehaviorEffectApplyResult {
            resource_depletions,
            report: BehaviorEffectReport {
                building,
                origin,
                behavior_id,
                application: BehaviorEffectApplication::Applied {
                    effects: applied_effects,
                },
            },
            rejection_reason: None,
        }
    }

    fn validate_behavior_effects(
        &self,
        building: BuildingId,
        effects: &[BehaviorEffect],
    ) -> Result<(), BehaviorEffectRejection> {
        let mut inventories = self.inventories.clone();
        let mut resources = self.resources.clone();
        let rules = InventoryItemRules::from_catalog(&self.catalog);

        for effect in effects {
            match effect {
                BehaviorEffect::SetState(_) | BehaviorEffect::DropItems { .. } => {}
                BehaviorEffect::SetPowerOutput { max_output } => {
                    let Some(building_def) = self
                        .buildings
                        .get(&building)
                        .and_then(|building| self.catalog.building_by_id(&building.def_id))
                    else {
                        return Err(BehaviorEffectRejection {
                            effect: effect.clone(),
                            reason: BehaviorEffectRejectionReason::PowerOutputRejected,
                        });
                    };
                    let Some(generator) = &building_def.power.generator else {
                        return Err(BehaviorEffectRejection {
                            effect: effect.clone(),
                            reason: BehaviorEffectRejectionReason::PowerOutputRejected,
                        });
                    };
                    if i64::from(*max_output) > generator.max_output.raw() {
                        return Err(BehaviorEffectRejection {
                            effect: effect.clone(),
                            reason: BehaviorEffectRejectionReason::PowerOutputRejected,
                        });
                    }
                }
                BehaviorEffect::TakeInventory { role, stack } => {
                    Self::remove_from_inventory_records(
                        &self.buildings,
                        &mut inventories,
                        building,
                        core_inventory_role(*role),
                        core_stack(*stack),
                    )
                    .map_err(|_| BehaviorEffectRejection {
                        effect: effect.clone(),
                        reason: BehaviorEffectRejectionReason::InventoryRejected,
                    })?;
                }
                BehaviorEffect::InsertInventory { role, stack } => {
                    Self::insert_into_inventory_records(
                        &self.buildings,
                        &mut inventories,
                        building,
                        core_inventory_role(*role),
                        core_stack(*stack),
                        &rules,
                    )
                    .map_err(|_| BehaviorEffectRejection {
                        effect: effect.clone(),
                        reason: BehaviorEffectRejectionReason::InventoryRejected,
                    })?;
                }
                BehaviorEffect::DrainInventory { role } => {
                    Self::inventory_id_for_role(
                        &self.buildings,
                        &inventories,
                        building,
                        core_inventory_role(*role),
                    )
                    .ok_or_else(|| BehaviorEffectRejection {
                        effect: effect.clone(),
                        reason: BehaviorEffectRejectionReason::InventoryRejected,
                    })?;
                }
                BehaviorEffect::DepleteResource { pos } => {
                    let pos = core_tile_pos(*pos);
                    let Some((_, amount)) = resources.get_mut(&pos) else {
                        return Err(BehaviorEffectRejection {
                            effect: effect.clone(),
                            reason: BehaviorEffectRejectionReason::MissingResource { pos },
                        });
                    };
                    if *amount == 0 {
                        return Err(BehaviorEffectRejection {
                            effect: effect.clone(),
                            reason: BehaviorEffectRejectionReason::MissingResource { pos },
                        });
                    }
                    *amount -= 1;
                }
            }
        }

        Ok(())
    }

    fn inventory_id_for_role(
        buildings: &BTreeMap<BuildingId, SimBuilding>,
        inventories: &BTreeMap<InventoryId, SimInventoryRecord>,
        building: BuildingId,
        role: CoreInventoryRole,
    ) -> Option<InventoryId> {
        buildings
            .get(&building)?
            .inventories
            .iter()
            .copied()
            .find(|id| {
                inventories
                    .get(id)
                    .is_some_and(|record| record.inventory.role() == role)
            })
    }

    fn insert_into_inventory_records(
        buildings: &BTreeMap<BuildingId, SimBuilding>,
        inventories: &mut BTreeMap<InventoryId, SimInventoryRecord>,
        building: BuildingId,
        role: CoreInventoryRole,
        stack: CoreItemStack,
        rules: &InventoryItemRules,
    ) -> Result<(), SimCommandError> {
        let Some(inventory_id) =
            Self::inventory_id_for_role(buildings, inventories, building, role)
        else {
            return Err(SimCommandError::InventoryRejected);
        };
        let Some(record) = inventories.get_mut(&inventory_id) else {
            return Err(SimCommandError::InventoryRejected);
        };
        let mut next = record.inventory.clone();
        let result = next.insert_with_mode(stack, InsertMode::AtomicAllOrNothing, rules);
        if result.rejected.is_some() {
            return Err(SimCommandError::InventoryRejected);
        }
        record.inventory = next;
        Ok(())
    }

    fn remove_from_inventory_records(
        buildings: &BTreeMap<BuildingId, SimBuilding>,
        inventories: &mut BTreeMap<InventoryId, SimInventoryRecord>,
        building: BuildingId,
        role: CoreInventoryRole,
        stack: CoreItemStack,
    ) -> Result<(), SimCommandError> {
        let Some(inventory_id) =
            Self::inventory_id_for_role(buildings, inventories, building, role)
        else {
            return Err(SimCommandError::InventoryRejected);
        };
        let Some(record) = inventories.get_mut(&inventory_id) else {
            return Err(SimCommandError::InventoryRejected);
        };
        let mut next = record.inventory.clone();
        if !next.remove(stack) {
            return Err(SimCommandError::InventoryRejected);
        }
        record.inventory = next;
        Ok(())
    }

    #[cfg(test)]
    fn active_line_ids_for_tests(&self) -> Vec<crate::ids::LineId> {
        self.activation.active_lines().collect()
    }

    pub fn build_straight_belt_line(
        &mut self,
        start: TilePos,
        len: i32,
        direction: Direction,
        speed: UnitsPerTick,
    ) -> Result<(), SimCommandError> {
        if len <= 0 {
            return Err(SimCommandError::InvalidPosition { pos: start });
        }

        let (dx, dy) = direction.delta();
        let positions = (0..len)
            .map(|offset| TilePos::new(start.x + dx * offset, start.y + dy * offset))
            .collect::<Vec<_>>();

        for &pos in &positions {
            self.ensure_buildable(pos)?;
        }
        for &pos in &positions {
            if self.occupied_tiles.contains_key(&pos) || self.building_occupancy.contains_key(&pos)
            {
                return Err(SimCommandError::OccupiedTile { pos });
            }
        }
        for pos in positions {
            self.occupied_tiles.insert(pos, speed);
            self.topology_graph.set_belt(
                pos,
                BeltTile::new(direction).on_surface(self.surface_z_at(pos)),
            );
        }
        self.rebuild_transport_lines();
        Ok(())
    }

    fn place_core_building(
        &mut self,
        def_id: String,
        origin: TilePos,
        direction: Direction,
        inserter_drop_direction: Option<Direction>,
        behavior_host: &(impl BehaviorHost + ?Sized),
    ) -> Result<(), SimCommandError> {
        let def = self
            .catalog
            .building_by_id(&def_id)
            .cloned()
            .ok_or(SimCommandError::UnknownBuildingKind)?;
        let kind = def.kind;
        let footprint_offsets =
            footprint_offsets_for_direction(&def.footprint, def.rotate_footprint, direction);
        let footprint = footprint_tiles(origin, &footprint_offsets);
        for &pos in &footprint {
            self.ensure_buildable(pos)?;
        }
        self.ensure_flat_footprint(origin, &footprint)?;
        for &pos in &footprint {
            if self.building_occupancy.contains_key(&pos) || self.occupied_tiles.contains_key(&pos)
            {
                return Err(SimCommandError::OccupiedTile { pos });
            }
        }

        let is_splitter = matches!(&def.behavior.driver, CoreBuildingDriver::Splitter { .. });
        match &def.behavior.driver {
            CoreBuildingDriver::Transport {
                speed_units_per_tick,
            } => {
                self.place_belt_tile(origin, direction, direction, *speed_units_per_tick)?;
                if self.refresh_connected_belt_inputs() {
                    self.rebuild_transport_lines();
                }
            }
            CoreBuildingDriver::Underground { .. } => {
                return Err(SimCommandError::InvalidPort);
            }
            CoreBuildingDriver::Splitter { .. } => {}
            CoreBuildingDriver::Noop
            | CoreBuildingDriver::Inserter { .. }
            | CoreBuildingDriver::BehaviorHost => {}
        }
        let surface_z = self.surface_z_at(origin);
        let ports = building_ports(origin, &footprint, direction, surface_z, &def);

        let state = initial_state(
            kind,
            &def.behavior,
            direction,
            inserter_drop_direction,
            behavior_host,
        )
        .map_err(|error| SimCommandError::BehaviorHostFailed {
            building: None,
            phase: BehaviorHostFailurePhase::Init,
            error,
        })?;

        let id = self.ids.next_building();
        let mut inventory_ids = Vec::new();
        for inventory_def in &def.inventories {
            let inventory_id = self.ids.next_inventory();
            self.inventories.insert(
                inventory_id,
                SimInventoryRecord {
                    id: inventory_id,
                    owner: id,
                    inventory: SimInventory::from_def(inventory_def),
                },
            );
            inventory_ids.push(inventory_id);
        }

        for &pos in &footprint {
            self.building_occupancy.insert(pos, id);
        }
        self.building_by_origin.insert(origin, id);
        self.buildings.insert(
            id,
            SimBuilding {
                id,
                def_id,
                kind,
                origin,
                surface_z,
                direction,
                footprint,
                ports,
                inventories: inventory_ids,
                state,
            },
        );
        if def.power.is_electric() {
            self.energy.mark_dirty();
        }
        if is_splitter {
            self.refresh_connected_belt_inputs();
            self.rebuild_transport_lines();
        }

        Ok(())
    }

    fn place_belt_tile(
        &mut self,
        pos: TilePos,
        direction: Direction,
        input_direction: Direction,
        speed: UnitsPerTick,
    ) -> Result<(), SimCommandError> {
        self.ensure_buildable(pos)?;
        if self.occupied_tiles.contains_key(&pos) || self.building_occupancy.contains_key(&pos) {
            return Err(SimCommandError::OccupiedTile { pos });
        }
        self.occupied_tiles.insert(pos, speed);
        self.topology_graph.set_belt(
            pos,
            BeltTile::turn(input_direction, direction).on_surface(self.surface_z_at(pos)),
        );
        self.rebuild_transport_lines();
        Ok(())
    }

    pub fn insert_into_inventory_atomic(
        &mut self,
        building: BuildingId,
        role: CoreInventoryRole,
        stack: crate::catalog::CoreItemStack,
    ) -> Result<(), SimCommandError> {
        let Some(building) = self.buildings.get(&building) else {
            return Err(SimCommandError::MissingBuildingId { building });
        };
        let Some(inventory_id) = building.inventories.iter().copied().find(|id| {
            self.inventories
                .get(id)
                .is_some_and(|record| record.inventory.role() == role)
        }) else {
            return Err(SimCommandError::InventoryRejected);
        };
        let item_rules = InventoryItemRules::from_catalog(&self.catalog);
        let Some(record) = self.inventories.get_mut(&inventory_id) else {
            return Err(SimCommandError::InventoryRejected);
        };
        let mut next = record.inventory.clone();
        if next
            .insert_with_mode(stack, InsertMode::AtomicAllOrNothing, &item_rules)
            .rejected
            .is_some()
        {
            return Err(SimCommandError::InventoryRejected);
        }
        record.inventory = next;
        Ok(())
    }

    pub fn inventory_slot(
        &self,
        building: BuildingId,
        role: CoreInventoryRole,
        slot: usize,
    ) -> Option<CoreItemStack> {
        let building = self.buildings.get(&building)?;
        let inventory_id = building.inventories.iter().copied().find(|id| {
            self.inventories
                .get(id)
                .is_some_and(|record| record.inventory.role() == role)
        })?;
        self.inventories
            .get(&inventory_id)?
            .inventory
            .snapshot()
            .slots
            .get(slot)
            .copied()
            .flatten()
    }

    pub fn insert_into_inventory_slot(
        &mut self,
        building: BuildingId,
        role: CoreInventoryRole,
        slot: usize,
        stack: crate::catalog::CoreItemStack,
        mode: InsertMode,
    ) -> InventoryInsertResult {
        let Some(building) = self.buildings.get(&building) else {
            return InventoryInsertResult {
                accepted: None,
                rejected: Some(stack),
                rejection: Some(crate::inventory::InventoryRejection::MissingInventory),
            };
        };
        let Some(inventory_id) = building.inventories.iter().copied().find(|id| {
            self.inventories
                .get(id)
                .is_some_and(|record| record.inventory.role() == role)
        }) else {
            return InventoryInsertResult {
                accepted: None,
                rejected: Some(stack),
                rejection: Some(crate::inventory::InventoryRejection::MissingInventory),
            };
        };
        let item_rules = InventoryItemRules::from_catalog(&self.catalog);
        let Some(record) = self.inventories.get_mut(&inventory_id) else {
            return InventoryInsertResult {
                accepted: None,
                rejected: Some(stack),
                rejection: Some(crate::inventory::InventoryRejection::MissingInventory),
            };
        };
        record
            .inventory
            .insert_into_slot_with_mode(slot, stack, mode, &item_rules)
    }

    pub fn take_from_inventory_stack(
        &mut self,
        building: BuildingId,
        role: CoreInventoryRole,
        slot: usize,
        amount: u32,
    ) -> Result<CoreItemStack, SimCommandError> {
        let Some(building) = self.buildings.get(&building) else {
            return Err(SimCommandError::MissingBuildingId { building });
        };
        let Some(inventory_id) = building.inventories.iter().copied().find(|id| {
            self.inventories
                .get(id)
                .is_some_and(|record| record.inventory.role() == role)
        }) else {
            return Err(SimCommandError::InventoryRejected);
        };
        let Some(record) = self.inventories.get_mut(&inventory_id) else {
            return Err(SimCommandError::InventoryRejected);
        };
        record
            .inventory
            .take_slot_amount(slot, amount)
            .ok_or(SimCommandError::InventoryRejected)
    }

    fn remove_from_inventory_atomic(
        &mut self,
        building: BuildingId,
        role: CoreInventoryRole,
        stack: CoreItemStack,
    ) -> Result<(), SimCommandError> {
        let Some(building) = self.buildings.get(&building) else {
            return Err(SimCommandError::MissingBuildingId { building });
        };
        let Some(inventory_id) = building.inventories.iter().copied().find(|id| {
            self.inventories
                .get(id)
                .is_some_and(|record| record.inventory.role() == role)
        }) else {
            return Err(SimCommandError::InventoryRejected);
        };
        let Some(record) = self.inventories.get_mut(&inventory_id) else {
            return Err(SimCommandError::InventoryRejected);
        };
        let mut next = record.inventory.clone();
        if !next.remove(stack) {
            return Err(SimCommandError::InventoryRejected);
        }
        record.inventory = next;
        Ok(())
    }

    fn drain_inventory_role(
        &mut self,
        building: BuildingId,
        role: CoreInventoryRole,
    ) -> Vec<InventorySlotEntry> {
        let Some(building) = self.buildings.get(&building) else {
            return Vec::new();
        };
        let Some(inventory_id) = building.inventories.iter().copied().find(|id| {
            self.inventories
                .get(id)
                .is_some_and(|record| record.inventory.role() == role)
        }) else {
            return Vec::new();
        };
        self.inventories
            .get_mut(&inventory_id)
            .map(|record| record.inventory.drain_entries())
            .unwrap_or_default()
    }

    fn take_building_inventories(&mut self, building: BuildingId) -> Vec<SimInventory> {
        let inventory_ids = self
            .buildings
            .get(&building)
            .map(|building| building.inventories.clone())
            .unwrap_or_default();
        inventory_ids
            .iter()
            .filter_map(|id| {
                self.inventories
                    .get(id)
                    .map(|record| record.inventory.clone())
            })
            .collect()
    }

    fn restore_building_inventories(
        &mut self,
        building: BuildingId,
        inventories: Vec<SimInventory>,
    ) {
        let Some(building) = self.buildings.get(&building) else {
            return;
        };
        for (inventory_id, inventory) in building.inventories.iter().zip(inventories) {
            if let Some(record) = self.inventories.get_mut(inventory_id) {
                record.inventory = inventory;
            }
        }
    }
    pub fn build_straight_belt_line_for_tests(
        &mut self,
        start: TilePos,
        len: i32,
        direction: Direction,
        speed: UnitsPerTick,
    ) -> Result<(), SimCommandError> {
        self.build_straight_belt_line(start, len, direction, speed)
    }

    fn remove_core_building(
        &mut self,
        id: BuildingId,
        behavior_host: &(impl BehaviorHost + ?Sized),
        behavior_catalog: &BehaviorCatalog,
    ) -> Result<(), SimCommandError> {
        let Some(building) = self.buildings.get(&id).cloned() else {
            return Ok(());
        };
        let removed_was_electric = self
            .catalog
            .building_by_id(&building.def_id)
            .is_some_and(|def| def.power.is_electric());

        let removed_behavior_effects = match &building.state {
            SimBuildingState::Behavior(state) => {
                if let Some(def) = self.catalog.building_by_id(&building.def_id)
                    && def.behavior.requires_behavior_host()
                {
                    behavior_host
                        .removed_behavior_effects(behavior_catalog, &def.behavior.config, state)
                        .map_err(|error| SimCommandError::BehaviorHostFailed {
                            building: Some(id),
                            phase: BehaviorHostFailurePhase::Remove,
                            error,
                        })?
                } else {
                    Vec::new()
                }
            }
            SimBuildingState::Passive
            | SimBuildingState::Transport
            | SimBuildingState::Inserter(_)
            | SimBuildingState::Underground(_)
            | SimBuildingState::Splitter(_) => Vec::new(),
        };

        if matches!(building.state, SimBuildingState::Underground(_)) {
            return self.remove_underground_building(id);
        }

        let Some(building) = self.buildings.remove(&id) else {
            return Ok(());
        };
        self.behavior_quarantine.remove(&id);
        self.building_by_origin.remove(&building.origin);
        let footprint = building.footprint.clone();
        for pos in &footprint {
            self.building_occupancy.remove(pos);
        }
        for inventory in building.inventories {
            if let Some(mut record) = self.inventories.remove(&inventory) {
                self.extend_removed_item_drop_entries(
                    building.origin,
                    record.inventory.drain_entries(),
                );
            }
        }

        match building.state {
            SimBuildingState::Inserter(inserter) => {
                if let Some(carried) = inserter.carried {
                    self.push_removed_item_drop(building.origin, carried);
                }
            }
            SimBuildingState::Underground(_) => {}
            SimBuildingState::Behavior(state) => {
                let _ = state;
                for effect in removed_behavior_effects {
                    if let BehaviorEffect::DropItems { stacks } = effect {
                        self.extend_removed_item_drops(building.origin, core_stacks(stacks));
                    }
                }
            }
            SimBuildingState::Splitter(runtime) => {
                self.extend_removed_item_drops(
                    building.origin,
                    runtime
                        .ingress_items
                        .into_iter()
                        .map(|item| carried_stack(item.item)),
                );
                self.extend_removed_item_drops(
                    building.origin,
                    runtime
                        .buffered_items
                        .into_iter()
                        .map(|item| carried_stack(item.item)),
                );
                self.extend_removed_item_drops(
                    building.origin,
                    runtime
                        .egress_items
                        .into_iter()
                        .map(|item| carried_stack(item.item)),
                );
            }
            SimBuildingState::Passive | SimBuildingState::Transport => {}
        }

        if building.kind == CoreBuildingKind::Transport {
            self.occupied_tiles.remove(&building.origin);
            self.topology_graph.remove_belt(building.origin);
            self.refresh_connected_belt_inputs();
            self.rebuild_transport_lines();
        }
        if removed_was_electric {
            self.energy.mark_dirty();
        }
        Ok(())
    }

    fn rebuild_dirty_energy_topology(&mut self) {
        if self.energy.topology_dirty {
            crate::energy::topology::rebuild_energy_topology(
                &mut self.energy,
                &self.catalog,
                &self.buildings,
            );
        }
    }
}

fn record_behavior_effect_metrics(
    metrics: &mut SimMetricsSnapshot,
    reports: &[BehaviorEffectReport],
    quarantined_instances: usize,
) {
    metrics.behavior_instances_quarantined = quarantined_instances;
    for report in reports {
        match &report.application {
            BehaviorEffectApplication::Applied { effects } => {
                metrics.behavior_effect_batches += 1;
                metrics.behavior_effects_applied += effects.len();
            }
            BehaviorEffectApplication::Rejected { effects } => {
                metrics.behavior_effect_batches += 1;
                metrics.behavior_effects_rejected += effects.len();
            }
            BehaviorEffectApplication::Quarantined { effects } => {
                metrics.behavior_effect_batches += 1;
                metrics.behavior_effects_rejected += effects.len();
            }
            BehaviorEffectApplication::HostFailed { .. } => {
                metrics.behavior_host_errors += 1;
            }
            BehaviorEffectApplication::Skipped { .. } => metrics.behavior_ticks_skipped += 1,
        }
    }
}

fn take_uninstanced_removal_drops(drops: &mut Vec<CoreRemovalDrop>) -> Vec<CoreRemovalDrop> {
    let mut uninstanced = Vec::new();
    let mut instanced = Vec::new();
    for drop in std::mem::take(drops) {
        if drop.instance.is_some() {
            instanced.push(drop);
        } else {
            uninstanced.push(drop);
        }
    }
    *drops = instanced;
    uninstanced
}

fn take_uninstanced_surface_drops(drops: &mut Vec<CoreSurfaceDrop>) -> Vec<CoreSurfaceDrop> {
    let mut uninstanced = Vec::new();
    let mut instanced = Vec::new();
    for drop in std::mem::take(drops) {
        if drop.instance.is_some() {
            instanced.push(drop);
        } else {
            uninstanced.push(drop);
        }
    }
    *drops = instanced;
    uninstanced
}

fn canonical_behavior_effects(mut effects: Vec<BehaviorEffect>) -> Vec<BehaviorEffect> {
    effects.sort_by_key(behavior_effect_phase);
    effects
}

fn behavior_effect_phase(effect: &BehaviorEffect) -> u8 {
    match effect {
        BehaviorEffect::TakeInventory { .. } => 0,
        BehaviorEffect::InsertInventory { .. } => 1,
        BehaviorEffect::DrainInventory { .. } => 2,
        BehaviorEffect::DepleteResource { .. } => 3,
        BehaviorEffect::DropItems { .. } => 4,
        BehaviorEffect::SetPowerOutput { .. } => 5,
        BehaviorEffect::SetState(_) => 6,
    }
}
