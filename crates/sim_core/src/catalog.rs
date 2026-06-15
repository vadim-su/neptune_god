//! Core data definitions: items, buildings, recipes, terrain, and test fixtures.

use std::collections::{BTreeMap, HashMap};

use crate::energy::PowerDef;
use crate::ids::ItemKindId;
use crate::units::{DistanceUnits, UnitsPerTick};
use behavior_api::BehaviorConfigValue;
use serde::{Deserialize, Serialize};

pub const TEST_IRON_ORE: ItemKindId = ItemKindId(1);
pub const TEST_COPPER_ORE: ItemKindId = ItemKindId(2);
pub const TEST_IRON_PLATE: ItemKindId = ItemKindId(3);
pub const TEST_COPPER_PLATE: ItemKindId = ItemKindId(4);
pub const TEST_IRON_GEAR: ItemKindId = ItemKindId(5);
pub const TEST_COPPER_CABLE: ItemKindId = ItemKindId(6);
pub const TEST_IRON_STICK: ItemKindId = ItemKindId(7);
pub const TEST_COAL: ItemKindId = ItemKindId(8);
pub const TEST_WOOD: ItemKindId = ItemKindId(9);
const DEFAULT_CORE_TERRAIN_ID: &str = "ground";
const TEST_BEHAVIOR_ID: &str = "test:behavior";

#[derive(Clone, Debug, PartialEq)]
pub struct CoreItemDef {
    pub id: ItemKindId,
    pub def_id: String,
    pub max_stack: u32,
    pub weight_grams: u32,
    pub bulk_units: u32,
    pub size_class: CoreItemSizeClass,
    pub tags: Vec<String>,
    pub equipment: Option<CoreEquipmentDef>,
}

#[derive(Clone, Copy, Debug, Deserialize, PartialEq, Eq, PartialOrd, Ord, Serialize)]
pub enum CoreItemSizeClass {
    Tiny,
    Small,
    Medium,
    Large,
    Oversized,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CoreContainerPolicy {
    pub slots: usize,
    pub max_stack: u32,
    pub stack_limits: Vec<CoreItemStackLimit>,
    pub comfortable_weight_limit_grams: Option<u32>,
    pub hard_weight_limit_grams: Option<u32>,
    pub max_bulk_units: Option<u32>,
    pub max_item_size: CoreItemSizeClass,
    pub accepts_tags: Vec<String>,
    pub rejects_tags: Vec<String>,
    pub accepts_items: Vec<ItemKindId>,
    pub rejects_items: Vec<ItemKindId>,
    pub pickup_priority: i32,
    pub quick_access: bool,
}

#[cfg(test)]
impl CoreContainerPolicy {
    pub fn universal_for_tests(
        slots: usize,
        max_weight_grams: u32,
        max_bulk_units: u32,
        max_item_size: CoreItemSizeClass,
        pickup_priority: i32,
        quick_access: bool,
    ) -> Self {
        Self {
            slots,
            max_stack: 100,
            stack_limits: Vec::new(),
            comfortable_weight_limit_grams: None,
            hard_weight_limit_grams: Some(max_weight_grams),
            max_bulk_units: Some(max_bulk_units),
            max_item_size,
            accepts_tags: Vec::new(),
            rejects_tags: Vec::new(),
            accepts_items: Vec::new(),
            rejects_items: Vec::new(),
            pickup_priority,
            quick_access,
        }
    }

    pub fn quick_filtered_for_tests(
        slots: usize,
        max_weight_grams: u32,
        max_bulk_units: u32,
        max_item_size: CoreItemSizeClass,
        accepts_tags: Vec<String>,
        pickup_priority: i32,
    ) -> Self {
        let mut policy = Self::universal_for_tests(
            slots,
            max_weight_grams,
            max_bulk_units,
            max_item_size,
            pickup_priority,
            true,
        );
        policy.accepts_tags = accepts_tags;
        policy
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CoreProvidedContainerDef {
    pub id: String,
    pub name: String,
    pub policy: CoreContainerPolicy,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CoreEquipmentDef {
    pub slot: String,
    pub provides_containers: Vec<CoreProvidedContainerDef>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum CoreBuildingKind {
    Machine,
    Transport,
    Passive,
    Inserter,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CoreInventoryRole {
    Input,
    Output,
    Fuel,
    Storage,
    InserterHand,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CoreInventoryDef {
    pub role: CoreInventoryRole,
    pub slots: usize,
    pub max_stack: u32,
    pub stack_limits: Vec<CoreItemStackLimit>,
    pub comfortable_weight_limit_grams: Option<u32>,
    pub hard_weight_limit_grams: Option<u32>,
    pub max_bulk_units: Option<u32>,
    pub max_item_size: CoreItemSizeClass,
    pub accepts_tags: Vec<String>,
    pub rejects_tags: Vec<String>,
    pub accepts: Vec<ItemKindId>,
    pub rejects: Vec<ItemKindId>,
    pub pickup_priority: i32,
    pub quick_access: bool,
}

impl CoreInventoryDef {
    pub fn new(role: CoreInventoryRole, slots: usize, max_stack: u32) -> Self {
        Self {
            role,
            slots,
            max_stack,
            stack_limits: Vec::new(),
            comfortable_weight_limit_grams: None,
            hard_weight_limit_grams: None,
            max_bulk_units: None,
            max_item_size: CoreItemSizeClass::Oversized,
            accepts_tags: Vec::new(),
            rejects_tags: Vec::new(),
            accepts: Vec::new(),
            rejects: Vec::new(),
            pickup_priority: 0,
            quick_access: false,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CorePersonalInventoryDefs {
    pub player: CoreInventoryDef,
    pub cursor: CoreInventoryDef,
    pub starting_equipment: Vec<CoreStartingEquipment>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CoreStartingEquipment {
    pub slot: String,
    pub item: ItemKindId,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CoreItemStackLimit {
    pub item: ItemKindId,
    pub max_stack: u32,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CoreInserterDepositLimit {
    pub role: CoreInventoryRole,
    pub item: ItemKindId,
    pub max_amount: u32,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CorePortRole {
    Input,
    Output,
    Fuel,
    Storage,
    BeltLane,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CorePortSide {
    North,
    East,
    South,
    West,
    OutputDirection,
    OppositeOutput,
    OutputDirectionLeft,
    OutputDirectionRight,
    AllEdges,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CorePortDef {
    pub role: CorePortRole,
    pub side: CorePortSide,
    pub offsets: Vec<i32>,
    pub accepts: Vec<ItemKindId>,
}

pub const CORE_TRANSPORT_BEHAVIOR_ID: &str = "core:transport";
pub const CORE_INSERTER_BEHAVIOR_ID: &str = "core:inserter";
pub const CORE_UNDERGROUND_BEHAVIOR_ID: &str = "core:underground";
pub const CORE_SPLITTER_BEHAVIOR_ID: &str = "core:splitter";
pub const CORE_CONVEYOR_LIFT_BEHAVIOR_ID: &str = "core:conveyor_lift";

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CoreBuildingBehavior {
    pub behavior_id: String,
    pub config: BTreeMap<String, BehaviorConfigValue>,
    pub driver: CoreBuildingDriver,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum CoreBuildingDriver {
    Noop,
    Transport {
        speed_units_per_tick: UnitsPerTick,
    },
    Underground {
        speed_units_per_tick: UnitsPerTick,
        max_range_tiles: u8,
    },
    ConveyorLift {
        speed_units_per_tick: UnitsPerTick,
    },
    Splitter {
        speed_units_per_tick: UnitsPerTick,
    },
    Inserter {
        cooldown_ticks: u32,
    },
    BehaviorHost,
}

impl CoreBuildingBehavior {
    pub fn noop(behavior_id: impl Into<String>) -> Self {
        Self {
            behavior_id: behavior_id.into(),
            config: BTreeMap::new(),
            driver: CoreBuildingDriver::Noop,
        }
    }

    pub fn transport(speed_units_per_tick: UnitsPerTick) -> Self {
        Self {
            behavior_id: CORE_TRANSPORT_BEHAVIOR_ID.to_string(),
            config: BTreeMap::new(),
            driver: CoreBuildingDriver::Transport {
                speed_units_per_tick,
            },
        }
    }

    pub fn underground(speed_units_per_tick: UnitsPerTick, max_range_tiles: u8) -> Self {
        Self {
            behavior_id: CORE_UNDERGROUND_BEHAVIOR_ID.to_string(),
            config: BTreeMap::new(),
            driver: CoreBuildingDriver::Underground {
                speed_units_per_tick,
                max_range_tiles,
            },
        }
    }

    pub fn conveyor_lift(speed_units_per_tick: UnitsPerTick) -> Self {
        Self {
            behavior_id: CORE_CONVEYOR_LIFT_BEHAVIOR_ID.to_string(),
            config: BTreeMap::new(),
            driver: CoreBuildingDriver::ConveyorLift {
                speed_units_per_tick,
            },
        }
    }

    pub fn splitter(speed_units_per_tick: UnitsPerTick) -> Self {
        Self {
            behavior_id: CORE_SPLITTER_BEHAVIOR_ID.to_string(),
            config: BTreeMap::new(),
            driver: CoreBuildingDriver::Splitter {
                speed_units_per_tick,
            },
        }
    }

    pub fn inserter(cooldown_ticks: u32) -> Self {
        Self {
            behavior_id: CORE_INSERTER_BEHAVIOR_ID.to_string(),
            config: BTreeMap::new(),
            driver: CoreBuildingDriver::Inserter { cooldown_ticks },
        }
    }

    pub fn hosted(
        behavior_id: impl Into<String>,
        config: BTreeMap<String, BehaviorConfigValue>,
    ) -> Self {
        Self {
            behavior_id: behavior_id.into(),
            config,
            driver: CoreBuildingDriver::BehaviorHost,
        }
    }

    pub fn requires_behavior_host(&self) -> bool {
        matches!(self.driver, CoreBuildingDriver::BehaviorHost)
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CoreBuildingDef {
    pub id: String,
    pub kind: CoreBuildingKind,
    pub footprint: Vec<(i32, i32)>,
    pub rotate_footprint: bool,
    pub inputs: Vec<CorePortDef>,
    pub outputs: Vec<CorePortDef>,
    pub inventories: Vec<CoreInventoryDef>,
    pub inserter_deposit_limits: Vec<CoreInserterDepositLimit>,
    pub behavior: CoreBuildingBehavior,
    pub power: PowerDef,
}

#[derive(Clone, Copy, Debug, Deserialize, PartialEq, Eq, Serialize)]
pub struct CoreItemStack {
    pub kind: ItemKindId,
    pub amount: u32,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CoreTerrainDef {
    pub id: String,
    pub buildable: bool,
    pub weight: u32,
}

pub fn behavior_config_value_string(value: impl Into<String>) -> BehaviorConfigValue {
    BehaviorConfigValue::String(value.into())
}

pub fn behavior_config_from_parts(
    role: &str,
    recipes: Vec<String>,
    work_area: Vec<(i32, i32)>,
) -> BTreeMap<String, BehaviorConfigValue> {
    BTreeMap::from([
        (
            "role".to_string(),
            behavior_config_value_string(role.to_string()),
        ),
        (
            "recipes".to_string(),
            BehaviorConfigValue::StringList(recipes),
        ),
        (
            "work_area".to_string(),
            BehaviorConfigValue::TileOffsets(
                work_area
                    .into_iter()
                    .map(|(x, y)| behavior_api::BehaviorTileOffset { x, y })
                    .collect(),
            ),
        ),
    ])
}

pub fn core_behavior_binding(
    role: &str,
    recipes: Vec<String>,
    work_area: Vec<(i32, i32)>,
) -> CoreBuildingBehavior {
    CoreBuildingBehavior::hosted(
        TEST_BEHAVIOR_ID,
        behavior_config_from_parts(role, recipes, work_area),
    )
}

/// Resolved item, terrain, and building definitions indexed by def id.
#[derive(Clone, Debug)]
pub struct CoreCatalog {
    pub items: Vec<CoreItemDef>,
    pub terrains: Vec<CoreTerrainDef>,
    pub buildings: Vec<CoreBuildingDef>,
    pub personal_inventories: CorePersonalInventoryDefs,
    item_ids_by_def_id: HashMap<String, ItemKindId>,
    def_ids_by_item_id: HashMap<ItemKindId, String>,
    terrain_ids_by_def_id: HashMap<String, usize>,
    default_terrain_id: String,
}

impl Default for CoreCatalog {
    fn default() -> Self {
        Self::new(Vec::new(), Vec::new(), Vec::new())
    }
}

impl CoreCatalog {
    pub fn new(
        items: Vec<CoreItemDef>,
        terrains: Vec<CoreTerrainDef>,
        buildings: Vec<CoreBuildingDef>,
    ) -> Self {
        Self::new_with_personal_inventories(
            items,
            terrains,
            buildings,
            default_personal_inventory_defs(),
        )
    }

    pub fn new_with_personal_inventories(
        items: Vec<CoreItemDef>,
        mut terrains: Vec<CoreTerrainDef>,
        buildings: Vec<CoreBuildingDef>,
        personal_inventories: CorePersonalInventoryDefs,
    ) -> Self {
        if terrains.is_empty() {
            terrains.push(default_core_terrain());
        }
        let item_ids_by_def_id = items
            .iter()
            .map(|item| (item.def_id.clone(), item.id))
            .collect::<HashMap<_, _>>();
        let def_ids_by_item_id = items
            .iter()
            .map(|item| (item.id, item.def_id.clone()))
            .collect::<HashMap<_, _>>();
        let terrain_ids_by_def_id = terrains
            .iter()
            .enumerate()
            .map(|(index, terrain)| (terrain.id.clone(), index))
            .collect::<HashMap<_, _>>();
        let default_terrain_id = if terrain_ids_by_def_id.contains_key(DEFAULT_CORE_TERRAIN_ID) {
            DEFAULT_CORE_TERRAIN_ID.to_string()
        } else {
            terrains
                .first()
                .expect("core catalog has at least one terrain")
                .id
                .clone()
        };
        Self {
            items,
            terrains,
            buildings,
            personal_inventories,
            item_ids_by_def_id,
            def_ids_by_item_id,
            terrain_ids_by_def_id,
            default_terrain_id,
        }
    }

    pub fn item(&self, id: ItemKindId) -> Option<&CoreItemDef> {
        self.items.iter().find(|item| item.id == id)
    }

    pub fn items(&self) -> impl Iterator<Item = &CoreItemDef> {
        self.items.iter()
    }

    pub fn item_weights(&self) -> BTreeMap<ItemKindId, u32> {
        self.items
            .iter()
            .map(|item| (item.id, item.weight_grams))
            .collect()
    }

    pub fn item_bulk_units(&self) -> BTreeMap<ItemKindId, u32> {
        self.items
            .iter()
            .map(|item| (item.id, item.bulk_units))
            .collect()
    }

    pub fn item_tags(&self) -> BTreeMap<ItemKindId, Vec<String>> {
        self.items
            .iter()
            .map(|item| (item.id, item.tags.clone()))
            .collect()
    }

    pub fn item_size_classes(&self) -> BTreeMap<ItemKindId, CoreItemSizeClass> {
        self.items
            .iter()
            .map(|item| (item.id, item.size_class))
            .collect()
    }

    pub fn item_id_by_def_id(&self, def_id: &str) -> Option<ItemKindId> {
        self.item_ids_by_def_id.get(def_id).copied()
    }

    pub fn def_id_by_item_id(&self, id: ItemKindId) -> Option<&str> {
        self.def_ids_by_item_id.get(&id).map(String::as_str)
    }

    pub fn terrain(&self, id: &str) -> Option<&CoreTerrainDef> {
        self.terrain_ids_by_def_id
            .get(id)
            .and_then(|index| self.terrains.get(*index))
    }

    pub fn default_terrain(&self) -> &CoreTerrainDef {
        self.terrain(&self.default_terrain_id)
            .expect("core catalog default terrain exists")
    }

    pub fn building_by_id(&self, id: &str) -> Option<&CoreBuildingDef> {
        self.buildings.iter().find(|building| building.id == id)
    }

    pub fn player_inventory_def(&self) -> &CoreInventoryDef {
        &self.personal_inventories.player
    }

    pub fn cursor_inventory_def(&self) -> &CoreInventoryDef {
        &self.personal_inventories.cursor
    }

    #[cfg(test)]
    pub fn rebuild_item_maps_for_tests(&mut self) {
        self.item_ids_by_def_id = self
            .items
            .iter()
            .map(|item| (item.def_id.clone(), item.id))
            .collect();
        self.def_ids_by_item_id = self
            .items
            .iter()
            .map(|item| (item.id, item.def_id.clone()))
            .collect();
    }

    pub fn for_tests() -> Self {
        Self::new(
            vec![
                ore_item(TEST_IRON_ORE, "iron_ore"),
                ore_item(TEST_COPPER_ORE, "copper_ore"),
                component_item(TEST_IRON_PLATE, "iron_plate", 100),
                component_item(TEST_COPPER_PLATE, "copper_plate", 100),
                component_item(TEST_IRON_GEAR, "iron_gear", 100),
                component_item(TEST_COPPER_CABLE, "copper_cable", 200),
                component_item(TEST_IRON_STICK, "iron_stick", 100),
                ore_item(TEST_COAL, "coal"),
                CoreItemDef {
                    id: TEST_WOOD,
                    def_id: "wood".to_string(),
                    max_stack: 100,
                    weight_grams: 500,
                    bulk_units: 4,
                    size_class: CoreItemSizeClass::Medium,
                    tags: vec!["raw_resource".to_string()],
                    equipment: None,
                },
            ],
            vec![
                CoreTerrainDef {
                    id: "ground".to_string(),
                    buildable: true,
                    weight: 1,
                },
                CoreTerrainDef {
                    id: "water".to_string(),
                    buildable: false,
                    weight: 1,
                },
                CoreTerrainDef {
                    id: "stone".to_string(),
                    buildable: false,
                    weight: 1,
                },
            ],
            vec![
                CoreBuildingDef {
                    id: "basic_miner".to_string(),
                    kind: CoreBuildingKind::Machine,
                    footprint: rectangle(2, 2),
                    rotate_footprint: false,
                    inputs: Vec::new(),
                    outputs: vec![CorePortDef {
                        role: CorePortRole::Output,
                        side: CorePortSide::AllEdges,
                        offsets: Vec::new(),
                        accepts: vec![TEST_IRON_ORE, TEST_COPPER_ORE, TEST_COAL],
                    }],
                    inventories: vec![
                        CoreInventoryDef {
                            role: CoreInventoryRole::Output,
                            slots: 1,
                            max_stack: 1,
                            stack_limits: Vec::new(),
                            comfortable_weight_limit_grams: None,
                            hard_weight_limit_grams: None,
                            accepts: vec![TEST_IRON_ORE, TEST_COPPER_ORE, TEST_COAL],
                            ..CoreInventoryDef::new(CoreInventoryRole::Output, 1, 1)
                        },
                        CoreInventoryDef {
                            role: CoreInventoryRole::Fuel,
                            slots: 1,
                            max_stack: 100,
                            stack_limits: Vec::new(),
                            comfortable_weight_limit_grams: None,
                            hard_weight_limit_grams: None,
                            accepts: vec![TEST_WOOD, TEST_COAL],
                            ..CoreInventoryDef::new(CoreInventoryRole::Fuel, 1, 100)
                        },
                    ],
                    inserter_deposit_limits: Vec::new(),
                    behavior: core_behavior_binding(
                        "extractor",
                        vec![
                            "mine_iron_ore".to_string(),
                            "mine_copper_ore".to_string(),
                            "mine_coal".to_string(),
                        ],
                        rectangle(5, 5),
                    ),
                    power: PowerDef::none(),
                },
                CoreBuildingDef {
                    id: "basic_assembler".to_string(),
                    kind: CoreBuildingKind::Machine,
                    footprint: rectangle(3, 3),
                    rotate_footprint: false,
                    inputs: Vec::new(),
                    outputs: vec![CorePortDef {
                        role: CorePortRole::Output,
                        side: CorePortSide::OutputDirection,
                        offsets: Vec::new(),
                        accepts: Vec::new(),
                    }],
                    inventories: vec![
                        CoreInventoryDef {
                            role: CoreInventoryRole::Input,
                            slots: 1,
                            max_stack: 100,
                            stack_limits: Vec::new(),
                            comfortable_weight_limit_grams: None,
                            hard_weight_limit_grams: None,
                            accepts: vec![TEST_IRON_PLATE, TEST_COPPER_PLATE],
                            ..CoreInventoryDef::new(CoreInventoryRole::Input, 1, 100)
                        },
                        CoreInventoryDef {
                            role: CoreInventoryRole::Output,
                            slots: 1,
                            max_stack: 200,
                            stack_limits: Vec::new(),
                            comfortable_weight_limit_grams: None,
                            hard_weight_limit_grams: None,
                            accepts: vec![TEST_IRON_GEAR, TEST_COPPER_CABLE, TEST_IRON_STICK],
                            ..CoreInventoryDef::new(CoreInventoryRole::Output, 1, 200)
                        },
                    ],
                    inserter_deposit_limits: Vec::new(),
                    behavior: core_behavior_binding(
                        "processor",
                        vec![
                            "iron_gear".to_string(),
                            "copper_cable".to_string(),
                            "iron_stick".to_string(),
                        ],
                        Vec::new(),
                    ),
                    power: PowerDef::none(),
                },
                CoreBuildingDef {
                    id: "wooden_chest".to_string(),
                    kind: CoreBuildingKind::Passive,
                    footprint: rectangle(2, 2),
                    rotate_footprint: false,
                    inputs: Vec::new(),
                    outputs: Vec::new(),
                    inventories: vec![CoreInventoryDef {
                        role: CoreInventoryRole::Storage,
                        slots: 40,
                        max_stack: 100,
                        stack_limits: Vec::new(),
                        comfortable_weight_limit_grams: None,
                        hard_weight_limit_grams: None,
                        accepts: Vec::new(),
                        ..CoreInventoryDef::new(CoreInventoryRole::Storage, 40, 100)
                    }],
                    inserter_deposit_limits: Vec::new(),
                    behavior: CoreBuildingBehavior::noop("test:storage"),
                    power: PowerDef::none(),
                },
                CoreBuildingDef {
                    id: "basic_belt".to_string(),
                    kind: CoreBuildingKind::Transport,
                    footprint: rectangle(1, 1),
                    rotate_footprint: true,
                    inputs: Vec::new(),
                    outputs: vec![CorePortDef {
                        role: CorePortRole::BeltLane,
                        side: CorePortSide::OutputDirection,
                        offsets: vec![0],
                        accepts: Vec::new(),
                    }],
                    inventories: Vec::new(),
                    inserter_deposit_limits: Vec::new(),
                    behavior: CoreBuildingBehavior::transport(UnitsPerTick::new(4)),
                    power: PowerDef::none(),
                },
                CoreBuildingDef {
                    id: "accelerated_belt".to_string(),
                    kind: CoreBuildingKind::Transport,
                    footprint: rectangle(1, 1),
                    rotate_footprint: true,
                    inputs: Vec::new(),
                    outputs: vec![CorePortDef {
                        role: CorePortRole::BeltLane,
                        side: CorePortSide::OutputDirection,
                        offsets: vec![0],
                        accepts: Vec::new(),
                    }],
                    inventories: Vec::new(),
                    inserter_deposit_limits: Vec::new(),
                    behavior: CoreBuildingBehavior::transport(UnitsPerTick::new(6)),
                    power: PowerDef::none(),
                },
                CoreBuildingDef {
                    id: "fast_belt".to_string(),
                    kind: CoreBuildingKind::Transport,
                    footprint: rectangle(1, 1),
                    rotate_footprint: true,
                    inputs: Vec::new(),
                    outputs: vec![CorePortDef {
                        role: CorePortRole::BeltLane,
                        side: CorePortSide::OutputDirection,
                        offsets: vec![0],
                        accepts: Vec::new(),
                    }],
                    inventories: Vec::new(),
                    inserter_deposit_limits: Vec::new(),
                    behavior: CoreBuildingBehavior::transport(UnitsPerTick::new(8)),
                    power: PowerDef::none(),
                },
                CoreBuildingDef {
                    id: "basic_splitter".to_string(),
                    kind: CoreBuildingKind::Transport,
                    footprint: vec![(0, 0), (0, 1)],
                    rotate_footprint: true,
                    inputs: Vec::new(),
                    outputs: Vec::new(),
                    inventories: Vec::new(),
                    inserter_deposit_limits: Vec::new(),
                    behavior: CoreBuildingBehavior::splitter(UnitsPerTick::new(4)),
                    power: PowerDef::none(),
                },
                CoreBuildingDef {
                    id: "basic_inserter".to_string(),
                    kind: CoreBuildingKind::Inserter,
                    footprint: rectangle(1, 1),
                    rotate_footprint: true,
                    inputs: Vec::new(),
                    outputs: Vec::new(),
                    inventories: vec![CoreInventoryDef {
                        role: CoreInventoryRole::InserterHand,
                        slots: 1,
                        max_stack: 1,
                        stack_limits: Vec::new(),
                        comfortable_weight_limit_grams: None,
                        hard_weight_limit_grams: None,
                        accepts: Vec::new(),
                        ..CoreInventoryDef::new(CoreInventoryRole::InserterHand, 1, 1)
                    }],
                    inserter_deposit_limits: Vec::new(),
                    behavior: CoreBuildingBehavior::inserter(27),
                    power: PowerDef::none(),
                },
                CoreBuildingDef {
                    id: "basic_underground_belt".to_string(),
                    kind: CoreBuildingKind::Passive,
                    footprint: rectangle(1, 1),
                    rotate_footprint: true,
                    inputs: Vec::new(),
                    outputs: vec![CorePortDef {
                        role: CorePortRole::BeltLane,
                        side: CorePortSide::OutputDirection,
                        offsets: vec![0],
                        accepts: Vec::new(),
                    }],
                    inventories: Vec::new(),
                    inserter_deposit_limits: Vec::new(),
                    behavior: CoreBuildingBehavior::underground(UnitsPerTick::new(4), 4),
                    power: PowerDef::none(),
                },
                CoreBuildingDef {
                    id: "basic_conveyor_lift".to_string(),
                    kind: CoreBuildingKind::Passive,
                    footprint: rectangle(1, 1),
                    rotate_footprint: true,
                    inputs: Vec::new(),
                    outputs: vec![CorePortDef {
                        role: CorePortRole::BeltLane,
                        side: CorePortSide::OutputDirection,
                        offsets: vec![0],
                        accepts: Vec::new(),
                    }],
                    inventories: Vec::new(),
                    inserter_deposit_limits: Vec::new(),
                    behavior: CoreBuildingBehavior::conveyor_lift(UnitsPerTick::new(4)),
                    power: PowerDef::none(),
                },
                CoreBuildingDef {
                    id: "stone_furnace".to_string(),
                    kind: CoreBuildingKind::Machine,
                    footprint: rectangle(3, 3),
                    rotate_footprint: false,
                    inputs: Vec::new(),
                    outputs: vec![CorePortDef {
                        role: CorePortRole::Output,
                        side: CorePortSide::OutputDirection,
                        offsets: vec![0, 1, 2],
                        accepts: vec![TEST_IRON_PLATE],
                    }],
                    inventories: vec![
                        CoreInventoryDef {
                            role: CoreInventoryRole::Input,
                            slots: 1,
                            max_stack: 100,
                            stack_limits: Vec::new(),
                            comfortable_weight_limit_grams: None,
                            hard_weight_limit_grams: None,
                            accepts: vec![TEST_IRON_ORE, TEST_COPPER_ORE],
                            ..CoreInventoryDef::new(CoreInventoryRole::Input, 1, 100)
                        },
                        CoreInventoryDef {
                            role: CoreInventoryRole::Output,
                            slots: 1,
                            max_stack: 100,
                            stack_limits: Vec::new(),
                            comfortable_weight_limit_grams: None,
                            hard_weight_limit_grams: None,
                            accepts: vec![TEST_IRON_PLATE, TEST_COPPER_PLATE],
                            ..CoreInventoryDef::new(CoreInventoryRole::Output, 1, 100)
                        },
                        CoreInventoryDef {
                            role: CoreInventoryRole::Fuel,
                            slots: 1,
                            max_stack: 100,
                            stack_limits: Vec::new(),
                            comfortable_weight_limit_grams: None,
                            hard_weight_limit_grams: None,
                            accepts: vec![TEST_COAL],
                            ..CoreInventoryDef::new(CoreInventoryRole::Fuel, 1, 100)
                        },
                    ],
                    inserter_deposit_limits: Vec::new(),
                    behavior: core_behavior_binding(
                        "processor",
                        vec!["iron_plate".to_string(), "copper_plate".to_string()],
                        Vec::new(),
                    ),
                    power: PowerDef::none(),
                },
            ],
        )
    }
}

fn default_personal_inventory_defs() -> CorePersonalInventoryDefs {
    CorePersonalInventoryDefs {
        player: CoreInventoryDef {
            role: CoreInventoryRole::Storage,
            slots: 80,
            max_stack: 100,
            stack_limits: Vec::new(),
            comfortable_weight_limit_grams: Some(40_000),
            hard_weight_limit_grams: Some(46_000),
            accepts: Vec::new(),
            ..CoreInventoryDef::new(CoreInventoryRole::Storage, 80, 100)
        },
        cursor: CoreInventoryDef {
            role: CoreInventoryRole::Storage,
            slots: 1,
            max_stack: 100,
            stack_limits: Vec::new(),
            comfortable_weight_limit_grams: None,
            hard_weight_limit_grams: None,
            accepts: Vec::new(),
            ..CoreInventoryDef::new(CoreInventoryRole::Storage, 1, 100)
        },
        starting_equipment: Vec::new(),
    }
}

fn default_core_terrain() -> CoreTerrainDef {
    CoreTerrainDef {
        id: DEFAULT_CORE_TERRAIN_ID.to_string(),
        buildable: true,
        weight: 1,
    }
}

fn ore_item(id: ItemKindId, def_id: &str) -> CoreItemDef {
    CoreItemDef {
        id,
        def_id: def_id.to_string(),
        max_stack: 100,
        weight_grams: 500,
        bulk_units: 3,
        size_class: CoreItemSizeClass::Small,
        tags: vec!["ore".to_string(), "raw_resource".to_string()],
        equipment: None,
    }
}

fn component_item(id: ItemKindId, def_id: &str, max_stack: u32) -> CoreItemDef {
    CoreItemDef {
        id,
        def_id: def_id.to_string(),
        max_stack,
        weight_grams: 1_000,
        bulk_units: 2,
        size_class: CoreItemSizeClass::Small,
        tags: vec!["component".to_string(), "small_part".to_string()],
        equipment: None,
    }
}

pub const BELT_ITEM_SPACING: DistanceUnits = DistanceUnits::new(64);

fn rectangle(width: i32, height: i32) -> Vec<(i32, i32)> {
    let mut offsets = Vec::new();
    for x in 0..width {
        for y in 0..height {
            offsets.push((x, y));
        }
    }
    offsets
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_catalog_defines_storage_chest_and_fueled_furnace_shell() {
        let catalog = CoreCatalog::for_tests();

        let chest = catalog.building_by_id("wooden_chest").unwrap();
        assert_eq!(chest.kind, CoreBuildingKind::Passive);
        assert_eq!(chest.behavior, CoreBuildingBehavior::noop("test:storage"));
        assert_eq!(chest.inventories[0].role, CoreInventoryRole::Storage);
        assert_eq!(chest.inventories[0].slots, 40);

        let furnace = catalog.building_by_id("stone_furnace").unwrap();
        assert_eq!(furnace.kind, CoreBuildingKind::Machine);
        assert!(furnace.inventories.iter().any(|inventory| {
            inventory.role == CoreInventoryRole::Fuel && inventory.accepts == vec![TEST_COAL]
        }));

        let coal = catalog.item(TEST_COAL).unwrap();
        assert_eq!(coal.def_id, "coal");
    }

    #[test]
    fn test_catalog_maps_content_ids_to_runtime_item_ids() {
        let catalog = CoreCatalog::for_tests();

        let coal = catalog.item_id_by_def_id("coal").expect("missing coal id");
        let wood = catalog.item_id_by_def_id("wood").expect("missing wood id");

        assert_ne!(coal, wood);
        assert_eq!(catalog.def_id_by_item_id(coal), Some("coal"));
        assert_eq!(
            catalog.item(coal).expect("missing coal item").max_stack,
            100
        );
    }

    #[test]
    fn catalog_default_terrain_prefers_ground_when_not_first() {
        let catalog = CoreCatalog::new(
            Vec::new(),
            vec![
                CoreTerrainDef {
                    id: "water".to_string(),
                    buildable: false,
                    weight: 1,
                },
                CoreTerrainDef {
                    id: "ground".to_string(),
                    buildable: true,
                    weight: 1,
                },
            ],
            Vec::new(),
        );

        assert_eq!(catalog.default_terrain().id, "ground");
        assert!(catalog.default_terrain().buildable);
    }

    #[test]
    fn catalog_preserves_behavior_bound_inventory_contracts() {
        let catalog = CoreCatalog::new(
            vec![
                ore_item(TEST_IRON_ORE, "iron_ore"),
                ore_item(TEST_COPPER_ORE, "copper_ore"),
                component_item(TEST_IRON_PLATE, "iron_plate", 100),
                component_item(TEST_COPPER_PLATE, "copper_plate", 100),
            ],
            vec![CoreTerrainDef {
                id: "ground".to_string(),
                buildable: true,
                weight: 1,
            }],
            vec![
                CoreBuildingDef {
                    id: "stone_furnace".to_string(),
                    kind: CoreBuildingKind::Machine,
                    footprint: vec![(0, 0)],
                    rotate_footprint: false,
                    inputs: Vec::new(),
                    outputs: Vec::new(),
                    inventories: vec![CoreInventoryDef {
                        role: CoreInventoryRole::Input,
                        slots: 1,
                        max_stack: 100,
                        stack_limits: Vec::new(),
                        comfortable_weight_limit_grams: None,
                        hard_weight_limit_grams: None,
                        accepts: vec![TEST_IRON_ORE],
                        ..CoreInventoryDef::new(CoreInventoryRole::Input, 1, 100)
                    }],
                    inserter_deposit_limits: Vec::new(),
                    behavior: core_behavior_binding(
                        "processor",
                        vec!["copper_plate".to_string()],
                        Vec::new(),
                    ),
                    power: PowerDef::none(),
                },
                CoreBuildingDef {
                    id: "cold_furnace".to_string(),
                    kind: CoreBuildingKind::Machine,
                    footprint: vec![(0, 0)],
                    rotate_footprint: false,
                    inputs: Vec::new(),
                    outputs: Vec::new(),
                    inventories: vec![CoreInventoryDef {
                        role: CoreInventoryRole::Input,
                        slots: 1,
                        max_stack: 100,
                        stack_limits: Vec::new(),
                        comfortable_weight_limit_grams: None,
                        hard_weight_limit_grams: None,
                        accepts: vec![TEST_COPPER_ORE],
                        ..CoreInventoryDef::new(CoreInventoryRole::Input, 1, 100)
                    }],
                    inserter_deposit_limits: Vec::new(),
                    behavior: core_behavior_binding(
                        "processor",
                        vec!["iron_plate".to_string()],
                        Vec::new(),
                    ),
                    power: PowerDef::none(),
                },
            ],
        );

        let stone = catalog.building_by_id("stone_furnace").unwrap();
        let cold = catalog.building_by_id("cold_furnace").unwrap();

        assert_eq!(
            stone
                .inventories
                .iter()
                .find(|inventory| inventory.role == CoreInventoryRole::Input)
                .unwrap()
                .accepts,
            vec![TEST_IRON_ORE]
        );
        assert_eq!(
            cold.inventories
                .iter()
                .find(|inventory| inventory.role == CoreInventoryRole::Input)
                .unwrap()
                .accepts,
            vec![TEST_COPPER_ORE]
        );
    }
}
