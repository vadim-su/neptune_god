//! Integration tests for [`SimWorld`]: placement, transport, behaviors, save/load.

use super::test_behavior::{
    MachineRuntime, MachineStatus, TEST_MACHINE_BEHAVIOR_ID, TestBehaviorHost, set_recipe_command,
    test_machine_behavior_state, test_machine_runtime,
};
use super::*;
use crate::behavior_host::{
    BehaviorEffectRejectionPolicy, BehaviorHost, BehaviorRuntime, BehaviorRuntimePolicy,
};
use crate::building::UndergroundRole;
use crate::catalog::{
    CoreBuildingBehavior, CoreBuildingDef, CoreBuildingKind, CoreCatalog, CoreContainerPolicy,
    CoreEquipmentDef, CoreInserterDepositLimit, CoreInventoryDef, CoreInventoryRole, CoreItemDef,
    CoreItemSizeClass, CoreItemStack, CoreItemStackLimit, CorePortDef, CorePortRole, CorePortSide,
    CoreProvidedContainerDef, CoreStartingEquipment, CoreTerrainDef, TEST_COAL, TEST_COPPER_CABLE,
    TEST_COPPER_ORE, TEST_COPPER_PLATE, TEST_IRON_GEAR, TEST_IRON_ORE, TEST_IRON_PLATE,
    TEST_IRON_STICK, TEST_WOOD,
};
use crate::command::{SimCommand, SimCommandError};
use crate::energy::{DEFAULT_POWER_CLASS, PowerConnectionDef, PowerDef, PowerUnits, SuppliedRatio};
use crate::ids::{ItemKindId, LineId, TilePos};
use crate::inventory::{InsertMode, InventoryRejection};
use crate::tick::{
    BehaviorEffectApplication, BehaviorEffectRejectionReason, BehaviorHostFailurePhase,
    BehaviorTickSkipReason,
};
use crate::topology::graph::Direction;
use crate::transport::interaction::{BeltInteraction, BeltInteractionKind};
use crate::transport::node::{
    SplitterBufferedItem, SplitterEgressItem, SplitterIngressItem, SplitterRuntime, TransportNode,
    TransportNodeId, TransportNodeKind, TransportNodeRuntime, TransportPort, TransportPortRole,
    UndergroundTransportItem, UndergroundTransportRuntime,
};
use crate::transport::stream::MIN_ITEM_SPACING;
use crate::units::DistanceUnits;
use crate::units::UnitsPerTick;
use crate::view::{
    SimRenderView, VisibleSplitterItem, VisibleSplitterItemPhase, VisibleTileBounds,
};
use behavior_api::{
    BehaviorCatalog, BehaviorCommandInput, BehaviorCommandOutput, BehaviorConfigValue,
    BehaviorEffect, BehaviorFuelDef, BehaviorHostError, BehaviorHostErrorKind, BehaviorHostResult,
    BehaviorId, BehaviorInitInput, BehaviorInstanceState, BehaviorItemDef, BehaviorItemStack,
    BehaviorRecipeDef, BehaviorRecipeEnergyDef, BehaviorRecipeKind, BehaviorStatus,
    BehaviorTickInput, BehaviorTickMetrics, BehaviorTickOutput, BehaviorTilePos,
};
use std::collections::{BTreeMap, BTreeSet};

fn catalog_for_tests() -> CoreCatalog {
    let mut catalog = CoreCatalog::for_tests();
    for building in &mut catalog.buildings {
        if building.behavior.requires_behavior_host() {
            building.behavior.behavior_id = TEST_MACHINE_BEHAVIOR_ID.to_string();
        }
    }
    catalog.buildings.push(CoreBuildingDef {
        id: "fast_underground_belt".to_string(),
        kind: CoreBuildingKind::Passive,
        footprint: vec![(0, 0)],
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
        behavior: CoreBuildingBehavior::underground(UnitsPerTick::new(8), 4),
        power: crate::energy::PowerDef::none(),
    });
    catalog
}

fn character_catalog_for_tests() -> CoreCatalog {
    let mut catalog = catalog_for_tests();
    let tool_belt = ItemKindId(100);
    let work_pants = ItemKindId(101);
    let backpack = ItemKindId(102);

    catalog.items.extend([
        CoreItemDef {
            id: tool_belt,
            def_id: "tool_belt".to_string(),
            max_stack: 1,
            weight_grams: 800,
            bulk_units: 8,
            size_class: CoreItemSizeClass::Medium,
            tags: vec!["equipment".to_string(), "belt".to_string()],
            equipment: Some(CoreEquipmentDef {
                slot: "waist".to_string(),
                provides_containers: vec![CoreProvidedContainerDef {
                    id: "tool_belt".to_string(),
                    name: "Tool belt".to_string(),
                    policy: CoreContainerPolicy::quick_filtered_for_tests(
                        6,
                        12_000,
                        24,
                        CoreItemSizeClass::Medium,
                        vec!["tool".to_string()],
                        40,
                    ),
                }],
            }),
        },
        CoreItemDef {
            id: work_pants,
            def_id: "work_pants".to_string(),
            max_stack: 1,
            weight_grams: 900,
            bulk_units: 10,
            size_class: CoreItemSizeClass::Medium,
            tags: vec!["equipment".to_string(), "clothing".to_string()],
            equipment: Some(CoreEquipmentDef {
                slot: "legs".to_string(),
                provides_containers: vec![
                    CoreProvidedContainerDef {
                        id: "left_pocket".to_string(),
                        name: "Left pocket".to_string(),
                        policy: CoreContainerPolicy::quick_filtered_for_tests(
                            4,
                            4_000,
                            8,
                            CoreItemSizeClass::Small,
                            vec!["small_part".to_string()],
                            30,
                        ),
                    },
                    CoreProvidedContainerDef {
                        id: "right_pocket".to_string(),
                        name: "Right pocket".to_string(),
                        policy: CoreContainerPolicy::quick_filtered_for_tests(
                            4,
                            4_000,
                            10,
                            CoreItemSizeClass::Small,
                            vec!["small_part".to_string()],
                            29,
                        ),
                    },
                ],
            }),
        },
        CoreItemDef {
            id: backpack,
            def_id: "small_backpack".to_string(),
            max_stack: 1,
            weight_grams: 1200,
            bulk_units: 16,
            size_class: CoreItemSizeClass::Large,
            tags: vec!["equipment".to_string(), "container".to_string()],
            equipment: Some(CoreEquipmentDef {
                slot: "back".to_string(),
                provides_containers: vec![CoreProvidedContainerDef {
                    id: "backpack_main".to_string(),
                    name: "Backpack".to_string(),
                    policy: CoreContainerPolicy::universal_for_tests(
                        28,
                        36_000,
                        120,
                        CoreItemSizeClass::Large,
                        10,
                        false,
                    ),
                }],
            }),
        },
    ]);
    catalog.personal_inventories.starting_equipment = vec![
        CoreStartingEquipment {
            slot: "waist".to_string(),
            item: tool_belt,
        },
        CoreStartingEquipment {
            slot: "legs".to_string(),
            item: work_pants,
        },
        CoreStartingEquipment {
            slot: "back".to_string(),
            item: backpack,
        },
    ];
    catalog.rebuild_item_maps_for_tests();
    catalog
}

fn select_iron_plate_recipe(world: &mut SimWorld, building: crate::ids::BuildingId) {
    set_machine_recipe_for_tests(world, building, Some("iron_plate".to_string()));
}

fn set_machine_recipe_for_tests(
    world: &mut SimWorld,
    building: crate::ids::BuildingId,
    recipe: Option<String>,
) {
    world
        .apply_command_with_behavior_runtime(
            SimCommand::ApplyBehaviorCommand {
                building,
                command: set_recipe_command(recipe),
            },
            BehaviorRuntime::new(&TestBehaviorHost, &test_behavior_catalog()),
        )
        .unwrap();
}

fn tick_world_for_tests(world: &mut SimWorld) -> SimTickOutput {
    world.tick_with_behavior_runtime(BehaviorRuntime::new(
        &TestBehaviorHost,
        &test_behavior_catalog(),
    ))
}

fn apply_command_with_behavior_for_tests(
    world: &mut SimWorld,
    command: SimCommand,
) -> Result<(), SimCommandError> {
    world.apply_command_with_behavior_runtime(
        command,
        BehaviorRuntime::new(&TestBehaviorHost, &test_behavior_catalog()),
    )
}

#[derive(Clone, Copy, Debug)]
struct InvalidEffectHost;

impl BehaviorHost for InvalidEffectHost {
    fn initial_behavior_state(
        &self,
        input: BehaviorInitInput<'_>,
    ) -> BehaviorHostResult<BehaviorInstanceState> {
        Ok(BehaviorInstanceState::new(
            BehaviorId::new(input.behavior_id),
            BehaviorStatus::new("old_state"),
        ))
    }

    fn apply_behavior_command(
        &self,
        input: BehaviorCommandInput<'_>,
    ) -> BehaviorHostResult<BehaviorCommandOutput> {
        Ok(BehaviorCommandOutput {
            effects: invalid_effect_batch(input.state.behavior_id),
        })
    }

    fn tick_behavior(
        &self,
        input: BehaviorTickInput<'_>,
    ) -> BehaviorHostResult<BehaviorTickOutput> {
        Ok(BehaviorTickOutput {
            effects: invalid_effect_batch(input.state.behavior_id),
            metrics: BehaviorTickMetrics::default(),
            output_changed: true,
        })
    }

    fn removed_behavior_effects(
        &self,
        _catalog: &BehaviorCatalog,
        _config: &BTreeMap<String, BehaviorConfigValue>,
        _state: &BehaviorInstanceState,
    ) -> BehaviorHostResult<Vec<BehaviorEffect>> {
        Ok(Vec::new())
    }

    fn behavior_accepts_input(
        &self,
        _catalog: &BehaviorCatalog,
        _config: &BTreeMap<String, BehaviorConfigValue>,
        _state: &BehaviorInstanceState,
        _kind: u16,
    ) -> BehaviorHostResult<bool> {
        Ok(true)
    }
}

fn invalid_effect_batch(behavior_id: BehaviorId) -> Vec<BehaviorEffect> {
    vec![
        BehaviorEffect::SetState(BehaviorInstanceState::new(
            behavior_id,
            BehaviorStatus::new("new_state"),
        )),
        BehaviorEffect::DepleteResource {
            pos: BehaviorTilePos { x: 999, y: 999 },
        },
    ]
}

fn invalid_effect_runtime(policy: BehaviorRuntimePolicy) -> BehaviorRuntime<'static> {
    static HOST: InvalidEffectHost = InvalidEffectHost;
    static CATALOG: BehaviorCatalog = BehaviorCatalog {
        items: Vec::new(),
        recipes: Vec::new(),
    };
    BehaviorRuntime::new_with_policy(&HOST, &CATALOG, policy)
}

#[derive(Clone, Copy, Debug)]
struct OversizedInsertHost;

impl BehaviorHost for OversizedInsertHost {
    fn initial_behavior_state(
        &self,
        input: BehaviorInitInput<'_>,
    ) -> BehaviorHostResult<BehaviorInstanceState> {
        Ok(BehaviorInstanceState::new(
            BehaviorId::new(input.behavior_id),
            BehaviorStatus::new("idle"),
        ))
    }

    fn apply_behavior_command(
        &self,
        input: BehaviorCommandInput<'_>,
    ) -> BehaviorHostResult<BehaviorCommandOutput> {
        Ok(BehaviorCommandOutput {
            effects: oversized_insert_effects(input.state.behavior_id),
        })
    }

    fn tick_behavior(
        &self,
        input: BehaviorTickInput<'_>,
    ) -> BehaviorHostResult<BehaviorTickOutput> {
        Ok(BehaviorTickOutput {
            effects: oversized_insert_effects(input.state.behavior_id),
            metrics: BehaviorTickMetrics::default(),
            output_changed: true,
        })
    }

    fn removed_behavior_effects(
        &self,
        _catalog: &BehaviorCatalog,
        _config: &BTreeMap<String, BehaviorConfigValue>,
        _state: &BehaviorInstanceState,
    ) -> BehaviorHostResult<Vec<BehaviorEffect>> {
        Ok(Vec::new())
    }

    fn behavior_accepts_input(
        &self,
        _catalog: &BehaviorCatalog,
        _config: &BTreeMap<String, BehaviorConfigValue>,
        _state: &BehaviorInstanceState,
        _kind: u16,
    ) -> BehaviorHostResult<bool> {
        Ok(true)
    }
}

fn oversized_insert_effects(behavior_id: BehaviorId) -> Vec<BehaviorEffect> {
    vec![
        BehaviorEffect::InsertInventory {
            role: behavior_api::BehaviorInventoryRole::Output,
            stack: behavior_stack(TEST_IRON_GEAR, 1),
        },
        BehaviorEffect::SetState(BehaviorInstanceState::new(
            behavior_id,
            BehaviorStatus::new("inserted"),
        )),
    ]
}

fn oversized_insert_runtime() -> BehaviorRuntime<'static> {
    static HOST: OversizedInsertHost = OversizedInsertHost;
    static CATALOG: BehaviorCatalog = BehaviorCatalog {
        items: Vec::new(),
        recipes: Vec::new(),
    };
    BehaviorRuntime::new_with_policy(&HOST, &CATALOG, report_only_policy())
}

fn report_only_policy() -> BehaviorRuntimePolicy {
    BehaviorRuntimePolicy {
        effect_rejection: BehaviorEffectRejectionPolicy::ReportOnly,
    }
}

fn quarantine_instance_policy() -> BehaviorRuntimePolicy {
    BehaviorRuntimePolicy {
        effect_rejection: BehaviorEffectRejectionPolicy::QuarantineInstance,
    }
}

#[derive(Clone, Copy, Debug)]
enum FailingBehaviorPhase {
    Init,
    Command,
    Tick,
}

#[derive(Clone, Copy, Debug)]
struct FailingBehaviorHost {
    phase: FailingBehaviorPhase,
}

#[derive(Clone, Copy, Debug)]
struct PowerOutputBehaviorHost {
    output: u32,
}

impl BehaviorHost for PowerOutputBehaviorHost {
    fn initial_behavior_state(
        &self,
        input: BehaviorInitInput<'_>,
    ) -> BehaviorHostResult<BehaviorInstanceState> {
        Ok(BehaviorInstanceState::new(
            BehaviorId::new(input.behavior_id),
            BehaviorStatus::new("idle"),
        ))
    }

    fn apply_behavior_command(
        &self,
        input: BehaviorCommandInput<'_>,
    ) -> BehaviorHostResult<BehaviorCommandOutput> {
        Ok(BehaviorCommandOutput {
            effects: vec![BehaviorEffect::SetState(input.state)],
        })
    }

    fn tick_behavior(
        &self,
        input: BehaviorTickInput<'_>,
    ) -> BehaviorHostResult<BehaviorTickOutput> {
        Ok(BehaviorTickOutput {
            effects: vec![
                BehaviorEffect::SetPowerOutput {
                    max_output: self.output,
                },
                BehaviorEffect::SetState(input.state),
            ],
            metrics: BehaviorTickMetrics::default(),
            output_changed: false,
        })
    }

    fn removed_behavior_effects(
        &self,
        _catalog: &BehaviorCatalog,
        _config: &BTreeMap<String, BehaviorConfigValue>,
        _state: &BehaviorInstanceState,
    ) -> BehaviorHostResult<Vec<BehaviorEffect>> {
        Ok(Vec::new())
    }

    fn behavior_accepts_input(
        &self,
        _catalog: &BehaviorCatalog,
        _config: &BTreeMap<String, BehaviorConfigValue>,
        _state: &BehaviorInstanceState,
        _kind: u16,
    ) -> BehaviorHostResult<bool> {
        Ok(true)
    }
}

impl FailingBehaviorHost {
    fn error(self) -> BehaviorHostError {
        BehaviorHostError::new(BehaviorHostErrorKind::RuntimeFailure, "test host failure")
    }
}

impl BehaviorHost for FailingBehaviorHost {
    fn initial_behavior_state(
        &self,
        input: BehaviorInitInput<'_>,
    ) -> BehaviorHostResult<BehaviorInstanceState> {
        if matches!(self.phase, FailingBehaviorPhase::Init) {
            return Err(self.error());
        }
        Ok(BehaviorInstanceState::new(
            BehaviorId::new(input.behavior_id),
            BehaviorStatus::new("old_state"),
        ))
    }

    fn apply_behavior_command(
        &self,
        input: BehaviorCommandInput<'_>,
    ) -> BehaviorHostResult<BehaviorCommandOutput> {
        if matches!(self.phase, FailingBehaviorPhase::Command) {
            return Err(self.error());
        }
        Ok(BehaviorCommandOutput {
            effects: vec![BehaviorEffect::SetState(input.state)],
        })
    }

    fn tick_behavior(
        &self,
        input: BehaviorTickInput<'_>,
    ) -> BehaviorHostResult<BehaviorTickOutput> {
        if matches!(self.phase, FailingBehaviorPhase::Tick) {
            return Err(self.error());
        }
        Ok(BehaviorTickOutput {
            effects: vec![BehaviorEffect::SetState(input.state)],
            metrics: BehaviorTickMetrics::default(),
            output_changed: false,
        })
    }

    fn removed_behavior_effects(
        &self,
        _catalog: &BehaviorCatalog,
        _config: &BTreeMap<String, BehaviorConfigValue>,
        _state: &BehaviorInstanceState,
    ) -> BehaviorHostResult<Vec<BehaviorEffect>> {
        Ok(Vec::new())
    }

    fn behavior_accepts_input(
        &self,
        _catalog: &BehaviorCatalog,
        _config: &BTreeMap<String, BehaviorConfigValue>,
        _state: &BehaviorInstanceState,
        _kind: u16,
    ) -> BehaviorHostResult<bool> {
        Ok(true)
    }
}

fn failing_behavior_runtime(phase: FailingBehaviorPhase) -> BehaviorRuntime<'static> {
    static INIT_HOST: FailingBehaviorHost = FailingBehaviorHost {
        phase: FailingBehaviorPhase::Init,
    };
    static COMMAND_HOST: FailingBehaviorHost = FailingBehaviorHost {
        phase: FailingBehaviorPhase::Command,
    };
    static TICK_HOST: FailingBehaviorHost = FailingBehaviorHost {
        phase: FailingBehaviorPhase::Tick,
    };
    static CATALOG: BehaviorCatalog = BehaviorCatalog {
        items: Vec::new(),
        recipes: Vec::new(),
    };
    let host = match phase {
        FailingBehaviorPhase::Init => &INIT_HOST,
        FailingBehaviorPhase::Command => &COMMAND_HOST,
        FailingBehaviorPhase::Tick => &TICK_HOST,
    };
    BehaviorRuntime::new_with_policy(host, &CATALOG, quarantine_instance_policy())
}

fn place_invalid_effect_building(world: &mut SimWorld) {
    world
        .apply_command_with_behavior_runtime(
            SimCommand::PlaceBuilding {
                def_id: "stone_furnace".to_string(),
                origin: TilePos::new(0, 0),
                direction: Direction::East,
                inserter_drop_direction: None,
            },
            invalid_effect_runtime(report_only_policy()),
        )
        .unwrap();
}

#[derive(Clone, Copy, Debug)]
struct ShuffledEffectHost;

impl BehaviorHost for ShuffledEffectHost {
    fn initial_behavior_state(
        &self,
        input: BehaviorInitInput<'_>,
    ) -> BehaviorHostResult<BehaviorInstanceState> {
        Ok(BehaviorInstanceState::new(
            BehaviorId::new(input.behavior_id),
            BehaviorStatus::new("old_state"),
        ))
    }

    fn apply_behavior_command(
        &self,
        input: BehaviorCommandInput<'_>,
    ) -> BehaviorHostResult<BehaviorCommandOutput> {
        Ok(BehaviorCommandOutput {
            effects: shuffled_effect_batch(input.state.behavior_id),
        })
    }

    fn tick_behavior(
        &self,
        input: BehaviorTickInput<'_>,
    ) -> BehaviorHostResult<BehaviorTickOutput> {
        Ok(BehaviorTickOutput {
            effects: shuffled_effect_batch(input.state.behavior_id),
            metrics: BehaviorTickMetrics::default(),
            output_changed: true,
        })
    }

    fn removed_behavior_effects(
        &self,
        _catalog: &BehaviorCatalog,
        _config: &BTreeMap<String, BehaviorConfigValue>,
        _state: &BehaviorInstanceState,
    ) -> BehaviorHostResult<Vec<BehaviorEffect>> {
        Ok(Vec::new())
    }

    fn behavior_accepts_input(
        &self,
        _catalog: &BehaviorCatalog,
        _config: &BTreeMap<String, BehaviorConfigValue>,
        _state: &BehaviorInstanceState,
        _kind: u16,
    ) -> BehaviorHostResult<bool> {
        Ok(true)
    }
}

fn shuffled_effect_batch(behavior_id: BehaviorId) -> Vec<BehaviorEffect> {
    vec![
        BehaviorEffect::SetState(BehaviorInstanceState::new(
            behavior_id,
            BehaviorStatus::new("new_state"),
        )),
        BehaviorEffect::DropItems {
            stacks: vec![behavior_stack(TEST_WOOD, 1)],
        },
        BehaviorEffect::DepleteResource {
            pos: BehaviorTilePos { x: 5, y: 5 },
        },
        BehaviorEffect::InsertInventory {
            role: behavior_api::BehaviorInventoryRole::Output,
            stack: behavior_stack(TEST_IRON_PLATE, 1),
        },
        BehaviorEffect::TakeInventory {
            role: behavior_api::BehaviorInventoryRole::Input,
            stack: behavior_stack(TEST_IRON_ORE, 1),
        },
    ]
}

fn shuffled_effect_runtime() -> BehaviorRuntime<'static> {
    static HOST: ShuffledEffectHost = ShuffledEffectHost;
    static CATALOG: BehaviorCatalog = BehaviorCatalog {
        items: Vec::new(),
        recipes: Vec::new(),
    };
    BehaviorRuntime::new_with_policy(&HOST, &CATALOG, report_only_policy())
}

#[test]
fn rejected_behavior_effect_batch_keeps_old_state_when_not_strict() {
    let mut world = SimWorld::with_catalog(catalog_for_tests());
    place_invalid_effect_building(&mut world);

    let output = world.tick_with_behavior_runtime(invalid_effect_runtime(report_only_policy()));

    let state = world
        .building_at(TilePos::new(0, 0))
        .unwrap()
        .state
        .behavior_state()
        .unwrap();
    assert_eq!(state.status.as_str(), "old_state");
    assert_eq!(
        world.resource_amount_for_tests(TilePos::new(999, 999)),
        None
    );

    let building = world
        .building_id_at_origin_for_tests(TilePos::new(0, 0))
        .unwrap();
    assert_eq!(output.behavior_effect_reports.len(), 1);
    assert_eq!(output.metrics.behavior_effect_batches, 1);
    assert_eq!(output.metrics.behavior_effects_applied, 0);
    assert_eq!(output.metrics.behavior_effects_rejected, 1);
    let report = &output.behavior_effect_reports[0];
    assert_eq!(report.building, building);
    assert_eq!(report.origin, TilePos::new(0, 0));
    assert_eq!(
        report.behavior_id.as_ref().map(BehaviorId::as_str),
        Some(TEST_MACHINE_BEHAVIOR_ID)
    );
    let BehaviorEffectApplication::Rejected { effects } = &report.application else {
        panic!("expected rejected behavior effect report");
    };
    assert_eq!(effects.len(), 1);
    assert!(matches!(
        effects[0].reason,
        BehaviorEffectRejectionReason::MissingResource {
            pos
        } if pos == TilePos::new(999, 999)
    ));
    assert!(matches!(
        effects[0].effect,
        BehaviorEffect::DepleteResource {
            pos: BehaviorTilePos { x: 999, y: 999 }
        }
    ));
}

#[test]
fn behavior_insert_effect_is_rejected_when_item_violates_core_size_policy() {
    let mut catalog = catalog_for_tests();
    catalog
        .buildings
        .iter_mut()
        .find(|building| building.id == "wooden_chest")
        .unwrap()
        .inventories[0]
        .max_item_size = CoreItemSizeClass::Tiny;
    let mut world = SimWorld::with_catalog(catalog);
    world
        .apply_core_command_for_tests(SimCommand::PlaceBuilding {
            def_id: "wooden_chest".to_string(),
            origin: TilePos::new(0, 0),
            direction: Direction::East,
            inserter_drop_direction: None,
        })
        .unwrap();
    let chest = world
        .building_id_at_origin_for_tests(TilePos::new(0, 0))
        .unwrap();

    let result = world.apply_behavior_effects_for_tests(
        chest,
        vec![BehaviorEffect::InsertInventory {
            role: behavior_api::BehaviorInventoryRole::Storage,
            stack: BehaviorItemStack {
                kind: TEST_IRON_ORE.0,
                amount: 1,
            },
        }],
    );

    assert_eq!(
        result.rejection_reason,
        Some(BehaviorEffectRejectionReason::InventoryRejected)
    );
}

#[test]
fn quarantine_policy_marks_rejected_behavior_and_skips_later_ticks() {
    let mut world = SimWorld::with_catalog(catalog_for_tests());
    place_invalid_effect_building(&mut world);

    let first_output =
        world.tick_with_behavior_runtime(invalid_effect_runtime(quarantine_instance_policy()));

    let building = world
        .building_id_at_origin_for_tests(TilePos::new(0, 0))
        .unwrap();
    assert_eq!(world.behavior_quarantine_count(), 1);
    assert_eq!(first_output.metrics.behavior_effect_batches, 1);
    assert_eq!(first_output.metrics.behavior_effects_rejected, 1);
    assert_eq!(first_output.metrics.behavior_instances_quarantined, 1);
    assert_eq!(first_output.metrics.behavior_ticks_skipped, 0);
    let BehaviorEffectApplication::Quarantined { effects } =
        &first_output.behavior_effect_reports[0].application
    else {
        panic!("expected quarantined behavior effect report");
    };
    assert_eq!(effects.len(), 1);

    let second_output =
        world.tick_with_behavior_runtime(invalid_effect_runtime(quarantine_instance_policy()));

    assert_eq!(world.behavior_quarantine_count(), 1);
    assert_eq!(second_output.metrics.behavior_effect_batches, 0);
    assert_eq!(second_output.metrics.behavior_effects_rejected, 0);
    assert_eq!(second_output.metrics.behavior_instances_quarantined, 1);
    assert_eq!(second_output.metrics.behavior_ticks_skipped, 1);
    assert_eq!(second_output.behavior_effect_reports.len(), 1);
    let report = &second_output.behavior_effect_reports[0];
    assert_eq!(report.building, building);
    let BehaviorEffectApplication::Skipped { reason } = &report.application else {
        panic!("expected skipped behavior report");
    };
    assert_eq!(*reason, BehaviorTickSkipReason::Quarantined);
}

#[test]
fn behavior_host_init_failure_rejects_placement_without_mutation() {
    let mut world = SimWorld::with_catalog(catalog_for_tests());

    let error = world
        .apply_command_with_behavior_runtime(
            SimCommand::PlaceBuilding {
                def_id: "stone_furnace".to_string(),
                origin: TilePos::new(0, 0),
                direction: Direction::East,
                inserter_drop_direction: None,
            },
            failing_behavior_runtime(FailingBehaviorPhase::Init),
        )
        .unwrap_err();

    let SimCommandError::BehaviorHostFailed {
        building,
        phase,
        error,
    } = error
    else {
        panic!("expected behavior host failure");
    };
    assert_eq!(building, None);
    assert_eq!(phase, BehaviorHostFailurePhase::Init);
    assert_eq!(error.kind, BehaviorHostErrorKind::RuntimeFailure);
    assert!(world.building_snapshots().is_empty());
}

#[test]
fn behavior_host_command_failure_rejects_without_state_change() {
    let mut world = SimWorld::with_catalog(catalog_for_tests());
    place_invalid_effect_building(&mut world);
    let building = world
        .building_id_at_origin_for_tests(TilePos::new(0, 0))
        .unwrap();

    let error = world
        .apply_command_with_behavior_runtime(
            SimCommand::ApplyBehaviorCommand {
                building,
                command: set_recipe_command(Some("iron_plate".to_string())),
            },
            failing_behavior_runtime(FailingBehaviorPhase::Command),
        )
        .unwrap_err();

    let SimCommandError::BehaviorHostFailed {
        building: failed_building,
        phase,
        error,
    } = error
    else {
        panic!("expected behavior host failure");
    };
    assert_eq!(failed_building, Some(building));
    assert_eq!(phase, BehaviorHostFailurePhase::Command);
    assert_eq!(error.kind, BehaviorHostErrorKind::RuntimeFailure);
    assert_eq!(
        world
            .building_at(TilePos::new(0, 0))
            .unwrap()
            .state
            .behavior_state()
            .unwrap()
            .status
            .as_str(),
        "old_state"
    );
}

#[test]
fn behavior_host_tick_failure_quarantines_instance_and_reports_error() {
    let mut world = SimWorld::with_catalog(catalog_for_tests());
    place_invalid_effect_building(&mut world);
    let building = world
        .building_id_at_origin_for_tests(TilePos::new(0, 0))
        .unwrap();

    let output =
        world.tick_with_behavior_runtime(failing_behavior_runtime(FailingBehaviorPhase::Tick));

    assert_eq!(world.behavior_quarantine_count(), 1);
    assert_eq!(output.metrics.behavior_host_errors, 1);
    assert_eq!(output.metrics.behavior_instances_quarantined, 1);
    let BehaviorEffectApplication::HostFailed { phase, error } =
        &output.behavior_effect_reports[0].application
    else {
        panic!("expected host failure report");
    };
    assert_eq!(*phase, BehaviorHostFailurePhase::Tick);
    assert_eq!(error.kind, BehaviorHostErrorKind::RuntimeFailure);

    let skipped =
        world.tick_with_behavior_runtime(failing_behavior_runtime(FailingBehaviorPhase::Tick));

    assert_eq!(skipped.behavior_effect_reports[0].building, building);
    let BehaviorEffectApplication::Skipped { reason } =
        skipped.behavior_effect_reports[0].application
    else {
        panic!("expected quarantined behavior to be skipped");
    };
    assert_eq!(reason, BehaviorTickSkipReason::Quarantined);
}

#[test]
fn behavior_effects_are_applied_and_reported_in_canonical_phases() {
    let mut world = SimWorld::with_catalog(catalog_for_tests());
    world
        .apply_command_with_behavior_runtime(
            SimCommand::PlaceBuilding {
                def_id: "stone_furnace".to_string(),
                origin: TilePos::new(0, 0),
                direction: Direction::East,
                inserter_drop_direction: None,
            },
            shuffled_effect_runtime(),
        )
        .unwrap();
    let building = world
        .building_id_at_origin_for_tests(TilePos::new(0, 0))
        .unwrap();
    world
        .insert_into_inventory_for_tests(
            building,
            CoreInventoryRole::Input,
            CoreItemStack {
                kind: TEST_IRON_ORE,
                amount: 1,
            },
        )
        .unwrap();
    world.seed_resource_for_tests(TilePos::new(5, 5), TEST_IRON_ORE, 1);

    let output = world.tick_with_behavior_runtime(shuffled_effect_runtime());

    let BehaviorEffectApplication::Applied { effects } =
        &output.behavior_effect_reports[0].application
    else {
        panic!("expected applied behavior effect report");
    };
    assert_eq!(output.metrics.behavior_effect_batches, 1);
    assert_eq!(output.metrics.behavior_effects_applied, effects.len());
    assert_eq!(output.metrics.behavior_effects_rejected, 0);
    let applied = effects
        .iter()
        .map(|effect| &effect.effect)
        .collect::<Vec<_>>();
    assert!(matches!(applied[0], BehaviorEffect::TakeInventory { .. }));
    assert!(matches!(applied[1], BehaviorEffect::InsertInventory { .. }));
    assert!(matches!(applied[2], BehaviorEffect::DepleteResource { .. }));
    assert!(matches!(applied[3], BehaviorEffect::DropItems { .. }));
    assert!(matches!(applied[4], BehaviorEffect::SetState(_)));
    assert_eq!(world.resource_amount_for_tests(TilePos::new(5, 5)), Some(0));
    assert_eq!(
        world
            .building_at(TilePos::new(0, 0))
            .unwrap()
            .state
            .behavior_state()
            .unwrap()
            .status
            .as_str(),
        "new_state"
    );
}

#[test]
fn behavior_effect_can_drive_generator_output_for_next_energy_solve() {
    let catalog = CoreCatalog::new(
        Vec::new(),
        Vec::new(),
        vec![
            CoreBuildingDef {
                id: "test_pole".to_string(),
                kind: CoreBuildingKind::Passive,
                footprint: vec![(0, 0)],
                rotate_footprint: false,
                inputs: Vec::new(),
                outputs: Vec::new(),
                inventories: Vec::new(),
                inserter_deposit_limits: Vec::new(),
                behavior: CoreBuildingBehavior::noop(""),
                power: PowerDef {
                    connection: Some(PowerConnectionDef {
                        coverage_radius_tiles: 4,
                        connection_range_tiles: 8,
                        edge_capacity: PowerUnits::new(600),
                        loss_per_tile: PowerUnits::ZERO,
                        power_class: DEFAULT_POWER_CLASS.to_string(),
                        input_power_classes: Vec::new(),
                    }),
                    generator: None,
                    storage: None,
                    consumer: None,
                },
            },
            CoreBuildingDef {
                id: "hosted_generator".to_string(),
                kind: CoreBuildingKind::Passive,
                footprint: vec![(0, 0)],
                rotate_footprint: false,
                inputs: Vec::new(),
                outputs: Vec::new(),
                inventories: Vec::new(),
                inserter_deposit_limits: Vec::new(),
                behavior: CoreBuildingBehavior::hosted("test:power_generator", BTreeMap::new()),
                power: PowerDef {
                    connection: Some(PowerConnectionDef {
                        coverage_radius_tiles: 0,
                        connection_range_tiles: 0,
                        edge_capacity: PowerUnits::new(600),
                        loss_per_tile: PowerUnits::ZERO,
                        power_class: DEFAULT_POWER_CLASS.to_string(),
                        input_power_classes: Vec::new(),
                    }),
                    generator: Some(crate::energy::GeneratorPowerDef {
                        max_output: PowerUnits::new(500),
                        initial_output: PowerUnits::ZERO,
                        mode: crate::energy::GeneratorMode::Constant,
                    }),
                    storage: None,
                    consumer: None,
                },
            },
            CoreBuildingDef {
                id: "test_consumer".to_string(),
                kind: CoreBuildingKind::Machine,
                footprint: vec![(0, 0)],
                rotate_footprint: false,
                inputs: Vec::new(),
                outputs: Vec::new(),
                inventories: Vec::new(),
                inserter_deposit_limits: Vec::new(),
                behavior: CoreBuildingBehavior::noop(""),
                power: PowerDef {
                    connection: Some(PowerConnectionDef {
                        coverage_radius_tiles: 0,
                        connection_range_tiles: 0,
                        edge_capacity: PowerUnits::new(600),
                        loss_per_tile: PowerUnits::ZERO,
                        power_class: DEFAULT_POWER_CLASS.to_string(),
                        input_power_classes: Vec::new(),
                    }),
                    generator: None,
                    storage: None,
                    consumer: Some(crate::energy::ConsumerPowerDef {
                        demand: PowerUnits::new(100),
                        priority: 1,
                        offline_below: SuppliedRatio::from_ppm(1),
                        power_sensitivity: crate::energy::PowerSensitivity::Linear,
                    }),
                },
            },
        ],
    );
    let mut world = SimWorld::with_catalog(catalog);
    for (def_id, pos) in [
        ("test_pole", TilePos::new(0, 0)),
        ("hosted_generator", TilePos::new(0, 1)),
        ("test_consumer", TilePos::new(2, 0)),
    ] {
        world
            .apply_command_with_behavior_runtime(
                SimCommand::PlaceBuilding {
                    def_id: def_id.to_string(),
                    origin: pos,
                    direction: Direction::East,
                    inserter_drop_direction: None,
                },
                BehaviorRuntime::new(
                    &PowerOutputBehaviorHost { output: 0 },
                    &BehaviorCatalog::default(),
                ),
            )
            .unwrap();
    }

    world.tick_with_behavior_runtime(BehaviorRuntime::new(
        &PowerOutputBehaviorHost { output: 0 },
        &BehaviorCatalog::default(),
    ));
    assert_eq!(
        world
            .energy_view_for_tests()
            .consumer_for_def("test_consumer")
            .unwrap()
            .supplied
            .raw(),
        0
    );

    world.tick_with_behavior_runtime(BehaviorRuntime::new(
        &PowerOutputBehaviorHost { output: 300 },
        &BehaviorCatalog::default(),
    ));
    world.tick_with_behavior_runtime(BehaviorRuntime::new(
        &PowerOutputBehaviorHost { output: 300 },
        &BehaviorCatalog::default(),
    ));

    let view = world.energy_view_for_tests();
    assert_eq!(
        view.consumer_for_def("test_consumer")
            .unwrap()
            .supplied
            .raw(),
        100
    );
    assert_eq!(
        view.source_for_def("hosted_generator")
            .unwrap()
            .max_output
            .raw(),
        300
    );
}

#[test]
#[should_panic(expected = "behavior effect rejected")]
fn strict_behavior_effect_policy_panics_on_rejected_batch() {
    let mut world = SimWorld::with_catalog(catalog_for_tests());
    place_invalid_effect_building(&mut world);

    world.tick_with_behavior_runtime(invalid_effect_runtime(BehaviorRuntimePolicy {
        effect_rejection: BehaviorEffectRejectionPolicy::Panic,
    }));
}

#[test]
fn behavior_command_returns_error_on_rejected_effect_batch() {
    let mut world = SimWorld::with_catalog(catalog_for_tests());
    place_invalid_effect_building(&mut world);
    let building = world
        .building_id_at_origin_for_tests(TilePos::new(0, 0))
        .unwrap();

    let error = world
        .apply_command_with_behavior_runtime(
            SimCommand::ApplyBehaviorCommand {
                building,
                command: behavior_api::BehaviorCommand::new("invalid"),
            },
            invalid_effect_runtime(report_only_policy()),
        )
        .unwrap_err();

    assert_eq!(
        error,
        SimCommandError::BehaviorEffectRejected {
            building,
            reason: BehaviorEffectRejectionReason::MissingResource {
                pos: TilePos::new(999, 999)
            },
        }
    );
}

fn test_behavior_catalog() -> BehaviorCatalog {
    BehaviorCatalog {
        items: vec![
            behavior_item(TEST_IRON_ORE, "iron_ore", 100, None),
            behavior_item(TEST_COPPER_ORE, "copper_ore", 100, None),
            behavior_item(TEST_IRON_PLATE, "iron_plate", 100, None),
            behavior_item(TEST_COPPER_PLATE, "copper_plate", 100, None),
            behavior_item(TEST_IRON_GEAR, "iron_gear", 100, None),
            behavior_item(TEST_COPPER_CABLE, "copper_cable", 200, None),
            behavior_item(TEST_IRON_STICK, "iron_stick", 100, None),
            behavior_item(
                TEST_COAL,
                "coal",
                100,
                Some(BehaviorFuelDef {
                    energy: 8.0,
                    burn_temperature: 900.0,
                }),
            ),
            behavior_item(
                TEST_WOOD,
                "wood",
                100,
                Some(BehaviorFuelDef {
                    energy: 4.0,
                    burn_temperature: 600.0,
                }),
            ),
        ],
        recipes: vec![
            BehaviorRecipeDef {
                id: "mine_iron_ore".to_string(),
                machines: vec!["basic_miner".to_string()],
                kind: BehaviorRecipeKind::Extraction {
                    resource: TEST_IRON_ORE.0,
                },
                duration_ticks: 60,
                inputs: Vec::new(),
                outputs: vec![behavior_stack(TEST_IRON_ORE, 1)],
                energy: Some(BehaviorRecipeEnergyDef {
                    required_per_second: 1.0,
                    min_temperature: 0.0,
                }),
            },
            BehaviorRecipeDef {
                id: "mine_copper_ore".to_string(),
                machines: vec!["basic_miner".to_string()],
                kind: BehaviorRecipeKind::Extraction {
                    resource: TEST_COPPER_ORE.0,
                },
                duration_ticks: 60,
                inputs: Vec::new(),
                outputs: vec![behavior_stack(TEST_COPPER_ORE, 1)],
                energy: Some(BehaviorRecipeEnergyDef {
                    required_per_second: 1.0,
                    min_temperature: 0.0,
                }),
            },
            BehaviorRecipeDef {
                id: "mine_coal".to_string(),
                machines: vec!["basic_miner".to_string()],
                kind: BehaviorRecipeKind::Extraction {
                    resource: TEST_COAL.0,
                },
                duration_ticks: 60,
                inputs: Vec::new(),
                outputs: vec![behavior_stack(TEST_COAL, 1)],
                energy: Some(BehaviorRecipeEnergyDef {
                    required_per_second: 1.0,
                    min_temperature: 0.0,
                }),
            },
            BehaviorRecipeDef {
                id: "iron_plate".to_string(),
                machines: vec!["stone_furnace".to_string()],
                kind: BehaviorRecipeKind::Processing,
                duration_ticks: 120,
                inputs: vec![behavior_stack(TEST_IRON_ORE, 1)],
                outputs: vec![behavior_stack(TEST_IRON_PLATE, 1)],
                energy: Some(BehaviorRecipeEnergyDef {
                    required_per_second: 1.0,
                    min_temperature: 700.0,
                }),
            },
            BehaviorRecipeDef {
                id: "iron_plate_alt".to_string(),
                machines: vec!["stone_furnace".to_string()],
                kind: BehaviorRecipeKind::Processing,
                duration_ticks: 90,
                inputs: vec![behavior_stack(TEST_IRON_ORE, 1)],
                outputs: vec![behavior_stack(TEST_IRON_PLATE, 1)],
                energy: Some(BehaviorRecipeEnergyDef {
                    required_per_second: 1.0,
                    min_temperature: 700.0,
                }),
            },
            BehaviorRecipeDef {
                id: "copper_plate".to_string(),
                machines: vec!["stone_furnace".to_string()],
                kind: BehaviorRecipeKind::Processing,
                duration_ticks: 120,
                inputs: vec![behavior_stack(TEST_COPPER_ORE, 1)],
                outputs: vec![behavior_stack(TEST_COPPER_PLATE, 1)],
                energy: Some(BehaviorRecipeEnergyDef {
                    required_per_second: 1.0,
                    min_temperature: 700.0,
                }),
            },
            BehaviorRecipeDef {
                id: "iron_gear".to_string(),
                machines: vec!["basic_assembler".to_string()],
                kind: BehaviorRecipeKind::Processing,
                duration_ticks: 30,
                inputs: vec![behavior_stack(TEST_IRON_PLATE, 2)],
                outputs: vec![behavior_stack(TEST_IRON_GEAR, 1)],
                energy: None,
            },
            BehaviorRecipeDef {
                id: "copper_cable".to_string(),
                machines: vec!["basic_assembler".to_string()],
                kind: BehaviorRecipeKind::Processing,
                duration_ticks: 30,
                inputs: vec![behavior_stack(TEST_COPPER_PLATE, 1)],
                outputs: vec![behavior_stack(TEST_COPPER_CABLE, 2)],
                energy: None,
            },
            BehaviorRecipeDef {
                id: "iron_stick".to_string(),
                machines: vec!["basic_assembler".to_string()],
                kind: BehaviorRecipeKind::Processing,
                duration_ticks: 30,
                inputs: vec![behavior_stack(TEST_IRON_PLATE, 1)],
                outputs: vec![behavior_stack(TEST_IRON_STICK, 2)],
                energy: None,
            },
        ],
    }
}

fn behavior_item(
    id: ItemKindId,
    def_id: &str,
    max_stack: u32,
    fuel: Option<BehaviorFuelDef>,
) -> BehaviorItemDef {
    BehaviorItemDef {
        kind: id.0,
        def_id: def_id.to_string(),
        max_stack,
        fuel,
    }
}

fn behavior_stack(kind: ItemKindId, amount: u32) -> BehaviorItemStack {
    BehaviorItemStack {
        kind: kind.0,
        amount,
    }
}

fn drain_output_for_tests(world: &mut SimWorld, building: crate::ids::BuildingId) {
    let Some(stack) = world
        .building_snapshots()
        .into_iter()
        .find(|snapshot| snapshot.id == building)
        .and_then(|snapshot| {
            snapshot
                .inventories
                .into_iter()
                .find(|inventory| inventory.role == CoreInventoryRole::Output)
        })
        .and_then(|inventory| inventory.slots.into_iter().flatten().next())
    else {
        return;
    };

    world
        .apply_core_command_for_tests(SimCommand::TakeFromInventory {
            building,
            role: CoreInventoryRole::Output,
            slot: 0,
            amount: stack.amount,
        })
        .unwrap();
}

fn output_stack_for_tests(
    world: &SimWorld,
    building: crate::ids::BuildingId,
) -> Option<CoreItemStack> {
    world
        .building_snapshots()
        .into_iter()
        .find(|snapshot| snapshot.id == building)
        .and_then(|snapshot| {
            snapshot
                .inventories
                .into_iter()
                .find(|inventory| inventory.role == CoreInventoryRole::Output)
        })
        .and_then(|inventory| inventory.slots.into_iter().flatten().next())
}

fn machine_runtime_for_tests(snapshot: &crate::building::SimBuildingSnapshot) -> MachineRuntime {
    let SimBuildingState::Behavior(state) = &snapshot.state else {
        panic!("expected machine behavior state");
    };
    test_machine_runtime(state).expect("expected machine behavior state")
}

#[allow(dead_code, reason = "used by cfg-disabled save/load factory loop test")]
fn output_amount(world: &SimWorld, building: crate::ids::BuildingId, item: ItemKindId) -> u32 {
    world
        .building_snapshots()
        .into_iter()
        .find(|snapshot| snapshot.id == building)
        .and_then(|snapshot| {
            snapshot
                .inventories
                .into_iter()
                .find(|inventory| inventory.role == CoreInventoryRole::Output)
        })
        .map(|inventory| {
            inventory
                .slots
                .into_iter()
                .flatten()
                .filter(|stack| stack.kind == item)
                .map(|stack| stack.amount)
                .sum()
        })
        .unwrap_or(0)
}

fn only_transport_line_speed_for_tests(world: &SimWorld) -> UnitsPerTick {
    let line_id = world.transport.line_ids_sorted().next().unwrap();
    assert_eq!(world.transport.line_ids_sorted().count(), 1);
    world.transport.line(line_id).unwrap().speed()
}

fn test_terrain() -> CoreTerrainDef {
    CoreTerrainDef {
        id: "ground".to_string(),
        buildable: true,
        weight: 1,
    }
}

fn place_catalog_belt(world: &mut SimWorld, def_id: &str, origin: TilePos, direction: Direction) {
    world
        .apply_core_command_for_tests(SimCommand::PlaceBuilding {
            def_id: def_id.to_string(),
            origin,
            direction,
            inserter_drop_direction: None,
        })
        .unwrap();
}

fn line_id_by_exact_path(world: &SimWorld, expected: &[TilePos]) -> LineId {
    world
        .transport
        .line_ids_sorted()
        .find(|&line_id| {
            world.transport.line(line_id).is_some_and(|line| {
                line.path()
                    .tiles()
                    .iter()
                    .map(|tile| tile.pos)
                    .eq(expected.iter().copied())
            })
        })
        .unwrap_or_else(|| panic!("expected line path {expected:?}"))
}

fn splitter_port(
    node: TransportNodeId,
    role: TransportPortRole,
    tile: TilePos,
    side: Direction,
    lane: usize,
    line: LineId,
) -> TransportPort {
    TransportPort {
        node,
        role,
        tile,
        side: Some(side),
        lane,
        line,
    }
}

#[test]
fn belt_cannot_be_placed_on_unbuildable_terrain() {
    let mut world = SimWorld::with_catalog(catalog_for_tests());
    world.set_terrain(TilePos::new(0, 0), "water").unwrap();

    let error = world
        .apply_core_command_for_tests(SimCommand::PlaceBelt {
            pos: TilePos::new(0, 0),
            direction: Direction::East,
            input_direction: Direction::East,
            speed: UnitsPerTick::new(8),
        })
        .unwrap_err();

    assert_eq!(
        error,
        SimCommandError::UnbuildableTile {
            pos: TilePos::new(0, 0)
        }
    );
}

#[test]
fn building_footprint_checks_all_terrain_tiles() {
    let mut world = SimWorld::with_catalog(catalog_for_tests());
    world.set_terrain(TilePos::new(1, 1), "water").unwrap();

    let error = world
        .apply_core_command_for_tests(SimCommand::PlaceBuilding {
            def_id: "wooden_chest".to_string(),
            origin: TilePos::new(0, 0),
            direction: Direction::East,
            inserter_drop_direction: None,
        })
        .unwrap_err();

    assert_eq!(
        error,
        SimCommandError::UnbuildableTile {
            pos: TilePos::new(1, 1)
        }
    );
}

#[test]
fn buildable_default_terrain_keeps_existing_placement_behavior() {
    let mut world = SimWorld::with_catalog(catalog_for_tests());

    world
        .apply_core_command_for_tests(SimCommand::PlaceBelt {
            pos: TilePos::new(0, 0),
            direction: Direction::East,
            input_direction: Direction::East,
            speed: UnitsPerTick::new(8),
        })
        .unwrap();

    assert!(world.terrain_at(TilePos::new(0, 0)).buildable);
}

#[test]
fn surface_z_defaults_to_zero_and_can_be_reset_to_default() {
    let mut world = SimWorld::with_catalog(catalog_for_tests());
    let pos = TilePos::new(3, -2);

    assert_eq!(world.surface_z_at(pos), crate::ids::DEFAULT_SURFACE_Z);

    world.set_surface_z(pos, 2);
    assert_eq!(world.surface_z_at(pos), 2);
    assert_eq!(world.snapshot().surface_z.get(&pos), Some(&2));

    world.set_surface_z(pos, crate::ids::DEFAULT_SURFACE_Z);
    assert_eq!(world.surface_z_at(pos), crate::ids::DEFAULT_SURFACE_Z);
    assert!(!world.snapshot().surface_z.contains_key(&pos));
}

#[test]
fn building_footprint_rejects_mixed_surface_levels() {
    let mut world = SimWorld::with_catalog(catalog_for_tests());
    world.set_surface_z(TilePos::new(1, 1), 2);

    let error = world
        .apply_core_command_for_tests(SimCommand::PlaceBuilding {
            def_id: "wooden_chest".to_string(),
            origin: TilePos::new(0, 0),
            direction: Direction::East,
            inserter_drop_direction: None,
        })
        .unwrap_err();

    assert_eq!(
        error,
        SimCommandError::UnevenTerrain {
            origin: TilePos::new(0, 0),
            pos: TilePos::new(1, 1),
            expected_z: crate::ids::DEFAULT_SURFACE_Z,
            found_z: 2,
        }
    );
}

#[test]
fn placed_building_snapshot_and_ports_store_surface_z() {
    let mut world = SimWorld::with_catalog(catalog_for_tests());
    let footprint = world
        .building_footprint_for("wooden_chest", TilePos::new(0, 0), Direction::East)
        .unwrap();
    for pos in footprint {
        world.set_surface_z(pos, 2);
    }

    world
        .apply_core_command_for_tests(SimCommand::PlaceBuilding {
            def_id: "wooden_chest".to_string(),
            origin: TilePos::new(0, 0),
            direction: Direction::East,
            inserter_drop_direction: None,
        })
        .unwrap();

    let snapshot = world.building_snapshots().remove(0);
    assert_eq!(snapshot.surface_z, 2);
    let building = world.building_at(TilePos::new(0, 0)).unwrap();
    assert!(building.ports.iter().all(|port| port.surface_z == 2));
}

#[test]
fn save_roundtrip_preserves_building_surface_z_and_ports() {
    let mut world = SimWorld::with_catalog(catalog_for_tests());
    let footprint = world
        .building_footprint_for("wooden_chest", TilePos::new(0, 0), Direction::East)
        .unwrap();
    for pos in footprint {
        world.set_surface_z(pos, 3);
    }
    world
        .apply_core_command_for_tests(SimCommand::PlaceBuilding {
            def_id: "wooden_chest".to_string(),
            origin: TilePos::new(0, 0),
            direction: Direction::East,
            inserter_drop_direction: None,
        })
        .unwrap();

    let restored = SimWorld::from_snapshot(catalog_for_tests(), world.snapshot()).unwrap();

    let snapshot = restored.building_snapshots().remove(0);
    assert_eq!(snapshot.surface_z, 3);
    let building = restored.building_at(TilePos::new(0, 0)).unwrap();
    assert!(building.ports.iter().all(|port| port.surface_z == 3));
}

#[test]
fn ordinary_belts_on_different_surface_z_do_not_connect() {
    let mut world = SimWorld::with_catalog(catalog_for_tests());
    world.set_surface_z(TilePos::new(1, 0), 1);

    world
        .apply_core_command_for_tests(SimCommand::PlaceBelt {
            pos: TilePos::new(0, 0),
            direction: Direction::East,
            input_direction: Direction::East,
            speed: UnitsPerTick::new(8),
        })
        .unwrap();
    world
        .apply_core_command_for_tests(SimCommand::PlaceBelt {
            pos: TilePos::new(1, 0),
            direction: Direction::East,
            input_direction: Direction::East,
            speed: UnitsPerTick::new(8),
        })
        .unwrap();

    assert_eq!(
        world
            .topology_graph
            .belt(TilePos::new(0, 0))
            .unwrap()
            .surface_z,
        0
    );
    assert_eq!(
        world
            .topology_graph
            .belt(TilePos::new(1, 0))
            .unwrap()
            .surface_z,
        1
    );

    let mut line_paths = world
        .transport
        .line_ids_sorted()
        .filter_map(|line_id| world.transport.line(line_id))
        .map(|line| {
            line.path()
                .tiles()
                .iter()
                .map(|tile| tile.pos)
                .collect::<Vec<_>>()
        })
        .collect::<Vec<_>>();
    line_paths.sort_by_key(|path| path[0]);

    assert_eq!(
        line_paths,
        vec![vec![TilePos::new(0, 0)], vec![TilePos::new(1, 0)]]
    );
    assert!(world.transport.nodes_sorted().all(|node| {
        !matches!(
            node.kind,
            TransportNodeKind::EndTransfer | TransportNodeKind::SideLoad { .. }
        )
    }));
}

#[test]
fn inserter_does_not_pick_up_storage_across_surface_levels() {
    let mut world = SimWorld::with_catalog(catalog_for_tests());
    world.set_surface_z(TilePos::new(2, 0), 1);
    world
        .apply_core_command_for_tests(SimCommand::PlaceBuilding {
            def_id: "wooden_chest".to_string(),
            origin: TilePos::new(0, 0),
            direction: Direction::East,
            inserter_drop_direction: None,
        })
        .unwrap();
    let chest_id = world.building_snapshots()[0].id;
    world
        .insert_into_inventory_for_tests(
            chest_id,
            CoreInventoryRole::Storage,
            CoreItemStack {
                kind: TEST_WOOD,
                amount: 1,
            },
        )
        .unwrap();
    world
        .apply_core_command_for_tests(SimCommand::PlaceBuilding {
            def_id: "basic_inserter".to_string(),
            origin: TilePos::new(2, 0),
            direction: Direction::West,
            inserter_drop_direction: Some(Direction::East),
        })
        .unwrap();
    let inserter_building = world
        .buildings
        .get(&world.building_snapshots()[1].id)
        .unwrap()
        .clone();
    let SimBuildingState::Inserter(inserter) = inserter_building.state.clone() else {
        panic!("expected inserter");
    };

    assert_eq!(
        world.try_inserter_pickup(&inserter_building, &inserter, &mut SimDiff::default()),
        None
    );
}

#[test]
fn inserter_picks_up_storage_on_same_surface_level() {
    let mut world = SimWorld::with_catalog(catalog_for_tests());
    for pos in [
        TilePos::new(0, 0),
        TilePos::new(0, 1),
        TilePos::new(1, 0),
        TilePos::new(1, 1),
        TilePos::new(2, 0),
    ] {
        world.set_surface_z(pos, 1);
    }
    world
        .apply_core_command_for_tests(SimCommand::PlaceBuilding {
            def_id: "wooden_chest".to_string(),
            origin: TilePos::new(0, 0),
            direction: Direction::East,
            inserter_drop_direction: None,
        })
        .unwrap();
    let chest_id = world.building_snapshots()[0].id;
    world
        .insert_into_inventory_for_tests(
            chest_id,
            CoreInventoryRole::Storage,
            CoreItemStack {
                kind: TEST_WOOD,
                amount: 1,
            },
        )
        .unwrap();
    world
        .apply_core_command_for_tests(SimCommand::PlaceBuilding {
            def_id: "basic_inserter".to_string(),
            origin: TilePos::new(2, 0),
            direction: Direction::West,
            inserter_drop_direction: Some(Direction::East),
        })
        .unwrap();
    let inserter_building = world
        .buildings
        .get(&world.building_snapshots()[1].id)
        .unwrap()
        .clone();
    let SimBuildingState::Inserter(inserter) = inserter_building.state.clone() else {
        panic!("expected inserter");
    };

    assert_eq!(
        world.try_inserter_pickup(&inserter_building, &inserter, &mut SimDiff::default()),
        Some(CoreItemStack {
            kind: TEST_WOOD,
            amount: 1,
        })
    );
}

#[test]
fn apply_generated_region_imports_surface_z() {
    let mut world = SimWorld::with_catalog(catalog_for_tests());
    let mut generated = crate::worldgen::WorldGenerator::new_default(123)
        .generate_rect(TilePos::new(0, 0), TilePos::new(0, 0));
    generated.terrain_tiles[0].surface_z = 3;

    world.apply_generated_region(&generated).unwrap();

    assert_eq!(world.surface_z_at(TilePos::new(0, 0)), 3);
}

#[test]
fn save_roundtrip_preserves_surface_z() {
    let mut world = SimWorld::with_catalog(catalog_for_tests());
    world.set_surface_z(TilePos::new(2, 5), 1);
    world.set_surface_z(TilePos::new(3, 5), 2);

    let restored = SimWorld::from_snapshot(catalog_for_tests(), world.snapshot()).unwrap();

    assert_eq!(restored.surface_z_at(TilePos::new(2, 5)), 1);
    assert_eq!(restored.surface_z_at(TilePos::new(3, 5)), 2);
    assert_eq!(
        restored.surface_z_at(TilePos::new(99, 99)),
        crate::ids::DEFAULT_SURFACE_Z
    );
}

#[test]
fn restore_normalizes_default_surface_z_entries() {
    let mut snapshot = SimWorld::with_catalog(catalog_for_tests()).snapshot();
    let pos = TilePos::new(6, 7);
    snapshot
        .surface_z
        .insert(pos, crate::ids::DEFAULT_SURFACE_Z);

    let restored = SimWorld::from_snapshot(catalog_for_tests(), snapshot).unwrap();

    assert_eq!(restored.surface_z_at(pos), crate::ids::DEFAULT_SURFACE_Z);
    assert!(!restored.snapshot().surface_z.contains_key(&pos));
}

#[test]
fn default_world_keeps_existing_belt_placement_behavior() {
    let mut world = SimWorld::default();

    world
        .apply_core_command_for_tests(SimCommand::PlaceBelt {
            pos: TilePos::new(0, 0),
            direction: Direction::East,
            input_direction: Direction::East,
            speed: UnitsPerTick::new(8),
        })
        .unwrap();

    assert_eq!(world.terrain_at(TilePos::new(0, 0)).id, "ground");
}

#[test]
fn place_building_creates_footprint_inventories_and_snapshots() {
    let mut world = SimWorld::with_catalog(catalog_for_tests());

    world
        .apply_core_command_for_tests(SimCommand::PlaceBuilding {
            def_id: "stone_furnace".to_string(),
            origin: TilePos::new(5, 7),
            direction: Direction::East,
            inserter_drop_direction: None,
        })
        .unwrap();

    assert!(world.is_occupied_for_tests(TilePos::new(7, 9)));
    let snapshots = world.building_snapshots();
    assert_eq!(snapshots.len(), 1);
    assert_eq!(snapshots[0].kind, CoreBuildingKind::Machine);
    assert_eq!(snapshots[0].def_id, "stone_furnace");
    assert!(
        snapshots[0]
            .inventories
            .iter()
            .any(|inventory| inventory.role == CoreInventoryRole::Fuel)
    );
}

#[test]
fn splitter_placement_rotates_footprint_without_creating_belt_tile() {
    let catalog = catalog_for_tests();
    for (direction, expected_footprint) in [
        (
            Direction::East,
            vec![TilePos::new(5, 7), TilePos::new(5, 8)],
        ),
        (
            Direction::North,
            vec![TilePos::new(5, 7), TilePos::new(6, 7)],
        ),
        (
            Direction::West,
            vec![TilePos::new(5, 7), TilePos::new(5, 8)],
        ),
        (
            Direction::South,
            vec![TilePos::new(5, 7), TilePos::new(6, 7)],
        ),
    ] {
        let mut world = SimWorld::with_catalog(catalog.clone());
        world
            .apply_core_command_for_tests(SimCommand::PlaceBuilding {
                def_id: "basic_splitter".to_string(),
                origin: TilePos::new(5, 7),
                direction,
                inserter_drop_direction: None,
            })
            .unwrap();

        let snapshot = world.snapshot();
        let splitter = snapshot.buildings.values().next().unwrap();
        assert_eq!(splitter.footprint, expected_footprint);
        assert!(matches!(
            &splitter.state,
            SimBuildingState::Splitter(runtime) if runtime.next_output == 0
        ));
        assert!(snapshot.transport.lines.is_empty());

        SimWorld::from_snapshot(catalog.clone(), snapshot).unwrap();
    }
}

#[test]
fn connected_splitter_builds_transport_node_with_real_line_ports() {
    let mut world = SimWorld::with_catalog(catalog_for_tests());
    place_catalog_belt(
        &mut world,
        "basic_belt",
        TilePos::new(-1, 0),
        Direction::East,
    );
    place_catalog_belt(
        &mut world,
        "basic_belt",
        TilePos::new(-1, 1),
        Direction::East,
    );
    place_catalog_belt(
        &mut world,
        "basic_belt",
        TilePos::new(1, 0),
        Direction::East,
    );
    place_catalog_belt(
        &mut world,
        "basic_belt",
        TilePos::new(1, 1),
        Direction::East,
    );
    world
        .apply_core_command_for_tests(SimCommand::PlaceBuilding {
            def_id: "basic_splitter".to_string(),
            origin: TilePos::new(0, 0),
            direction: Direction::East,
            inserter_drop_direction: None,
        })
        .unwrap();

    world.rebuild_transport_lines();

    assert_eq!(world.transport.line_ids_sorted().count(), 4);
    let top_input = line_id_by_exact_path(&world, &[TilePos::new(-1, 0)]);
    let bottom_input = line_id_by_exact_path(&world, &[TilePos::new(-1, 1)]);
    let top_output = line_id_by_exact_path(&world, &[TilePos::new(1, 0)]);
    let bottom_output = line_id_by_exact_path(&world, &[TilePos::new(1, 1)]);
    let splitter_nodes = world
        .transport
        .nodes_sorted()
        .filter(|node| node.kind == TransportNodeKind::Splitter2x1)
        .collect::<Vec<_>>();

    assert_eq!(splitter_nodes.len(), 1);
    let inputs = splitter_nodes[0].input_ports().copied().collect::<Vec<_>>();
    let outputs = splitter_nodes[0]
        .output_ports()
        .copied()
        .collect::<Vec<_>>();

    assert_eq!(
        inputs,
        vec![
            splitter_port(
                splitter_nodes[0].id,
                TransportPortRole::Input,
                TilePos::new(0, 0),
                Direction::West,
                0,
                top_input,
            ),
            splitter_port(
                splitter_nodes[0].id,
                TransportPortRole::Input,
                TilePos::new(0, 0),
                Direction::West,
                1,
                top_input,
            ),
            splitter_port(
                splitter_nodes[0].id,
                TransportPortRole::Input,
                TilePos::new(0, 1),
                Direction::West,
                0,
                bottom_input,
            ),
            splitter_port(
                splitter_nodes[0].id,
                TransportPortRole::Input,
                TilePos::new(0, 1),
                Direction::West,
                1,
                bottom_input,
            ),
        ]
    );
    assert_eq!(
        outputs,
        vec![
            splitter_port(
                splitter_nodes[0].id,
                TransportPortRole::Output,
                TilePos::new(1, 0),
                Direction::East,
                0,
                top_output,
            ),
            splitter_port(
                splitter_nodes[0].id,
                TransportPortRole::Output,
                TilePos::new(1, 0),
                Direction::East,
                1,
                top_output,
            ),
            splitter_port(
                splitter_nodes[0].id,
                TransportPortRole::Output,
                TilePos::new(1, 1),
                Direction::East,
                0,
                bottom_output,
            ),
            splitter_port(
                splitter_nodes[0].id,
                TransportPortRole::Output,
                TilePos::new(1, 1),
                Direction::East,
                1,
                bottom_output,
            ),
        ]
    );
}

#[test]
fn connected_splitter_accepts_output_belt_that_turns_from_splitter() {
    let mut world = SimWorld::with_catalog(catalog_for_tests());
    place_catalog_belt(
        &mut world,
        "basic_belt",
        TilePos::new(-1, 0),
        Direction::East,
    );
    place_catalog_belt(
        &mut world,
        "basic_belt",
        TilePos::new(-1, 1),
        Direction::East,
    );
    world
        .apply_core_command_for_tests(SimCommand::PlaceBelt {
            pos: TilePos::new(1, 0),
            direction: Direction::South,
            input_direction: Direction::East,
            speed: UnitsPerTick::new(4),
        })
        .unwrap();
    place_catalog_belt(
        &mut world,
        "basic_belt",
        TilePos::new(1, -1),
        Direction::South,
    );
    world
        .apply_core_command_for_tests(SimCommand::PlaceBuilding {
            def_id: "basic_splitter".to_string(),
            origin: TilePos::new(0, 0),
            direction: Direction::East,
            inserter_drop_direction: None,
        })
        .unwrap();

    world.rebuild_transport_lines();

    let turned_output = line_id_by_exact_path(&world, &[TilePos::new(1, 0), TilePos::new(1, -1)]);
    let splitter_nodes = world
        .transport
        .nodes_sorted()
        .filter(|node| node.kind == TransportNodeKind::Splitter2x1)
        .collect::<Vec<_>>();

    assert_eq!(splitter_nodes.len(), 1);
    assert!(
        splitter_nodes[0]
            .output_ports()
            .any(|port| port.tile == TilePos::new(1, 0) && port.line == turned_output)
    );

    world
        .apply_core_command_for_tests(SimCommand::DropItemOnBeltTile {
            pos: TilePos::new(-1, 0),
            lane: 0,
            distance_numerator: 128,
            distance_denominator: 128,
            item: TEST_IRON_ORE,
        })
        .unwrap();
    tick_splitter_until_egress(&mut world);

    assert_eq!(
        line_lane_items(&world, turned_output, 0),
        vec![TEST_IRON_ORE]
    );
}

#[test]
fn connected_splitter_accepts_single_input_channel_with_turning_output() {
    let mut world = SimWorld::with_catalog(catalog_for_tests());
    place_catalog_belt(
        &mut world,
        "basic_belt",
        TilePos::new(-1, 1),
        Direction::East,
    );
    world
        .apply_core_command_for_tests(SimCommand::PlaceBelt {
            pos: TilePos::new(1, 0),
            direction: Direction::South,
            input_direction: Direction::East,
            speed: UnitsPerTick::new(4),
        })
        .unwrap();
    place_catalog_belt(
        &mut world,
        "basic_belt",
        TilePos::new(1, -1),
        Direction::South,
    );
    world
        .apply_core_command_for_tests(SimCommand::PlaceBuilding {
            def_id: "basic_splitter".to_string(),
            origin: TilePos::new(0, 0),
            direction: Direction::East,
            inserter_drop_direction: None,
        })
        .unwrap();

    world.rebuild_transport_lines();

    assert!(
        world
            .transport
            .nodes_sorted()
            .any(|node| node.kind == TransportNodeKind::Splitter2x1)
    );

    let turned_output = line_id_by_exact_path(&world, &[TilePos::new(1, 0), TilePos::new(1, -1)]);
    world
        .apply_core_command_for_tests(SimCommand::DropItemOnBeltTile {
            pos: TilePos::new(-1, 1),
            lane: 0,
            distance_numerator: 128,
            distance_denominator: 128,
            item: TEST_IRON_ORE,
        })
        .unwrap();
    tick_splitter_until_egress(&mut world);

    assert_eq!(
        line_lane_items(&world, turned_output, 0),
        vec![TEST_IRON_ORE]
    );
}

#[test]
fn placing_splitter_after_belts_rebuilds_transport_immediately() {
    let mut world = SimWorld::with_catalog(catalog_for_tests());
    place_catalog_belt(
        &mut world,
        "basic_belt",
        TilePos::new(-1, 1),
        Direction::East,
    );
    world
        .apply_core_command_for_tests(SimCommand::PlaceBelt {
            pos: TilePos::new(1, 0),
            direction: Direction::North,
            input_direction: Direction::East,
            speed: UnitsPerTick::new(4),
        })
        .unwrap();
    place_catalog_belt(
        &mut world,
        "basic_belt",
        TilePos::new(1, 1),
        Direction::East,
    );
    world
        .apply_core_command_for_tests(SimCommand::PlaceBelt {
            pos: TilePos::new(2, 1),
            direction: Direction::South,
            input_direction: Direction::East,
            speed: UnitsPerTick::new(4),
        })
        .unwrap();

    world
        .apply_core_command_for_tests(SimCommand::PlaceBuilding {
            def_id: "basic_splitter".to_string(),
            origin: TilePos::new(0, 0),
            direction: Direction::East,
            inserter_drop_direction: None,
        })
        .unwrap();

    assert!(
        world
            .transport
            .nodes_sorted()
            .any(|node| node.kind == TransportNodeKind::Splitter2x1)
    );

    let top_output = line_id_by_exact_path(&world, &[TilePos::new(1, 0)]);
    let bottom_output = line_id_by_exact_path(&world, &[TilePos::new(1, 1), TilePos::new(2, 1)]);
    world
        .apply_core_command_for_tests(SimCommand::DropItemOnBeltTile {
            pos: TilePos::new(-1, 1),
            lane: 0,
            distance_numerator: 128,
            distance_denominator: 128,
            item: TEST_IRON_ORE,
        })
        .unwrap();
    tick_splitter_until_egress(&mut world);

    assert_eq!(
        line_lane_items(&world, top_output, 0)
            .into_iter()
            .chain(line_lane_items(&world, bottom_output, 0))
            .collect::<Vec<_>>(),
        vec![TEST_IRON_ORE]
    );
}

#[test]
fn splitter_output_catalog_belt_reports_turn_entry_for_rendering() {
    let mut world = SimWorld::with_catalog(catalog_for_tests());
    place_catalog_belt(
        &mut world,
        "basic_belt",
        TilePos::new(-1, 0),
        Direction::East,
    );
    place_catalog_belt(
        &mut world,
        "basic_belt",
        TilePos::new(-1, 1),
        Direction::East,
    );
    world
        .apply_core_command_for_tests(SimCommand::PlaceBuilding {
            def_id: "basic_splitter".to_string(),
            origin: TilePos::new(0, 0),
            direction: Direction::East,
            inserter_drop_direction: None,
        })
        .unwrap();
    place_catalog_belt(
        &mut world,
        "basic_belt",
        TilePos::new(1, 0),
        Direction::South,
    );
    place_catalog_belt(
        &mut world,
        "basic_belt",
        TilePos::new(1, -1),
        Direction::South,
    );

    world
        .apply_core_command_for_tests(SimCommand::DropItemOnBeltTile {
            pos: TilePos::new(-1, 0),
            lane: 0,
            distance_numerator: 128,
            distance_denominator: 128,
            item: TEST_IRON_ORE,
        })
        .unwrap();
    tick_splitter_until_egress(&mut world);

    let view = SimRenderView::extract(
        &world,
        VisibleTileBounds::new(TilePos::new(1, 0), TilePos::new(1, 0)),
    );
    let item = view
        .visible_items
        .iter()
        .find(|item| item.item == TEST_IRON_ORE)
        .expect("splitter output item should be visible on the first output belt tile");

    assert_eq!(item.entry_direction, Direction::East);
    assert_eq!(item.direction, Direction::South);
}

#[test]
fn connected_splitter_builds_transport_node_without_output_belts() {
    let mut world = SimWorld::with_catalog(catalog_for_tests());
    place_catalog_belt(
        &mut world,
        "basic_belt",
        TilePos::new(-1, 0),
        Direction::East,
    );
    place_catalog_belt(
        &mut world,
        "basic_belt",
        TilePos::new(-1, 1),
        Direction::East,
    );
    world
        .apply_core_command_for_tests(SimCommand::PlaceBuilding {
            def_id: "basic_splitter".to_string(),
            origin: TilePos::new(0, 0),
            direction: Direction::East,
            inserter_drop_direction: None,
        })
        .unwrap();

    world.rebuild_transport_lines();

    assert_eq!(world.transport.line_ids_sorted().count(), 2);
    let splitter_nodes = world
        .transport
        .nodes_sorted()
        .filter(|node| node.kind == TransportNodeKind::Splitter2x1)
        .collect::<Vec<_>>();

    assert_eq!(splitter_nodes.len(), 1);
    assert_eq!(splitter_nodes[0].input_ports().count(), 4);
    assert_eq!(splitter_nodes[0].output_ports().count(), 4);
    assert!(
        splitter_nodes[0]
            .output_ports()
            .all(|port| world.transport.line(port.line).is_none())
    );
}

#[test]
fn connected_splitter_keeps_transport_node_when_one_output_belt_is_removed() {
    let mut world = connected_splitter_world_for_tests();

    world
        .apply_core_command_for_tests(SimCommand::RemoveBuilding {
            pos: TilePos::new(1, 1),
        })
        .unwrap();
    world.rebuild_transport_lines();

    let splitter_nodes = world
        .transport
        .nodes_sorted()
        .filter(|node| node.kind == TransportNodeKind::Splitter2x1)
        .collect::<Vec<_>>();
    let existing_lines = splitter_nodes[0]
        .output_ports()
        .filter(|port| world.transport.line(port.line).is_some())
        .count();
    let missing_lines = splitter_nodes[0]
        .output_ports()
        .filter(|port| world.transport.line(port.line).is_none())
        .count();

    assert_eq!(splitter_nodes.len(), 1);
    assert_eq!(existing_lines, 2);
    assert_eq!(missing_lines, 2);
}

#[test]
fn splitter_rejects_output_lines_not_oriented_away_from_splitter() {
    let mut world = SimWorld::with_catalog(catalog_for_tests());
    place_catalog_belt(
        &mut world,
        "basic_belt",
        TilePos::new(-1, 0),
        Direction::East,
    );
    place_catalog_belt(
        &mut world,
        "basic_belt",
        TilePos::new(-1, 1),
        Direction::East,
    );
    place_catalog_belt(
        &mut world,
        "basic_belt",
        TilePos::new(1, 0),
        Direction::West,
    );
    place_catalog_belt(
        &mut world,
        "basic_belt",
        TilePos::new(1, 1),
        Direction::West,
    );
    world
        .apply_core_command_for_tests(SimCommand::PlaceBuilding {
            def_id: "basic_splitter".to_string(),
            origin: TilePos::new(0, 0),
            direction: Direction::East,
            inserter_drop_direction: None,
        })
        .unwrap();

    world.rebuild_transport_lines();

    assert_eq!(world.transport.line_ids_sorted().count(), 4);
    assert!(
        world
            .transport
            .nodes_sorted()
            .all(|node| node.kind != TransportNodeKind::Splitter2x1)
    );
}

#[test]
fn placing_splitter_reconnects_output_lines_with_wrong_input_side() {
    let mut world = SimWorld::with_catalog(catalog_for_tests());
    place_catalog_belt(
        &mut world,
        "basic_belt",
        TilePos::new(-1, 0),
        Direction::East,
    );
    place_catalog_belt(
        &mut world,
        "basic_belt",
        TilePos::new(-1, 1),
        Direction::East,
    );
    world
        .apply_core_command_for_tests(SimCommand::PlaceBelt {
            pos: TilePos::new(1, 0),
            direction: Direction::East,
            input_direction: Direction::North,
            speed: UnitsPerTick::new(4),
        })
        .unwrap();
    world
        .apply_core_command_for_tests(SimCommand::PlaceBelt {
            pos: TilePos::new(1, 1),
            direction: Direction::East,
            input_direction: Direction::North,
            speed: UnitsPerTick::new(4),
        })
        .unwrap();
    world
        .apply_core_command_for_tests(SimCommand::PlaceBuilding {
            def_id: "basic_splitter".to_string(),
            origin: TilePos::new(0, 0),
            direction: Direction::East,
            inserter_drop_direction: None,
        })
        .unwrap();

    world.rebuild_transport_lines();

    assert_eq!(
        world.topology_graph.belt(TilePos::new(1, 0)).unwrap(),
        BeltTile::turn(Direction::East, Direction::East)
    );
    assert_eq!(
        world.topology_graph.belt(TilePos::new(1, 1)).unwrap(),
        BeltTile::turn(Direction::East, Direction::East)
    );
    assert!(
        world
            .transport
            .nodes_sorted()
            .any(|node| node.kind == TransportNodeKind::Splitter2x1)
    );
}

#[test]
fn connected_splitter_inputs_do_not_keep_blocked_front_nodes() {
    let mut world = SimWorld::with_catalog(catalog_for_tests());
    place_catalog_belt(
        &mut world,
        "basic_belt",
        TilePos::new(-1, 0),
        Direction::East,
    );
    place_catalog_belt(
        &mut world,
        "basic_belt",
        TilePos::new(-1, 1),
        Direction::East,
    );
    place_catalog_belt(
        &mut world,
        "basic_belt",
        TilePos::new(1, 0),
        Direction::East,
    );
    place_catalog_belt(
        &mut world,
        "basic_belt",
        TilePos::new(1, 1),
        Direction::East,
    );
    world
        .apply_core_command_for_tests(SimCommand::PlaceBuilding {
            def_id: "basic_splitter".to_string(),
            origin: TilePos::new(0, 0),
            direction: Direction::East,
            inserter_drop_direction: None,
        })
        .unwrap();

    world.rebuild_transport_lines();

    let input_lines = [
        line_id_by_exact_path(&world, &[TilePos::new(-1, 0)]),
        line_id_by_exact_path(&world, &[TilePos::new(-1, 1)]),
    ];
    let blocked_input_lines = world
        .transport
        .nodes_sorted()
        .filter(|node| node.kind == TransportNodeKind::BlockedFront)
        .flat_map(|node| node.input_ports())
        .filter(|port| input_lines.contains(&port.line))
        .count();

    assert_eq!(blocked_input_lines, 0);
}

#[test]
fn isolated_splitter_does_not_build_transport_node_without_real_lines() {
    let mut world = SimWorld::with_catalog(catalog_for_tests());
    world
        .apply_core_command_for_tests(SimCommand::PlaceBuilding {
            def_id: "basic_splitter".to_string(),
            origin: TilePos::new(0, 0),
            direction: Direction::East,
            inserter_drop_direction: None,
        })
        .unwrap();

    world.rebuild_transport_lines();

    assert!(
        world
            .transport
            .nodes_sorted()
            .all(|node| node.kind != TransportNodeKind::Splitter2x1)
    );
}

#[test]
fn placed_building_stores_definition_id() {
    let mut world = SimWorld::with_catalog(catalog_for_tests());

    let result = world.apply_core_command_for_tests(SimCommand::PlaceBuilding {
        def_id: "basic_belt".to_string(),
        origin: TilePos::new(0, 0),
        direction: Direction::East,
        inserter_drop_direction: None,
    });

    assert_eq!(result, Ok(()));
    let building = world.building_at(TilePos::new(0, 0)).unwrap();
    assert_eq!(building.def_id, "basic_belt");
    assert_eq!(building.kind, CoreBuildingKind::Transport);
}

#[test]
fn placed_catalog_belts_use_definition_transport_speed() {
    let mut catalog = catalog_for_tests();
    let mut basic_belt = catalog.building_by_id("basic_belt").unwrap().clone();
    basic_belt.id = "basic_belt".to_string();
    basic_belt.behavior = CoreBuildingBehavior::transport(UnitsPerTick::new(4));
    let mut accelerated_belt = basic_belt.clone();
    accelerated_belt.id = "accelerated_belt".to_string();
    accelerated_belt.behavior = CoreBuildingBehavior::transport(UnitsPerTick::new(6));
    let mut fast_belt = basic_belt.clone();
    fast_belt.id = "fast_belt".to_string();
    fast_belt.behavior = CoreBuildingBehavior::transport(UnitsPerTick::new(8));
    catalog.buildings = vec![basic_belt, accelerated_belt, fast_belt];

    let mut basic_world = SimWorld::with_catalog(catalog.clone());
    basic_world
        .apply_core_command_for_tests(SimCommand::PlaceBuilding {
            def_id: "basic_belt".to_string(),
            origin: TilePos::new(0, 0),
            direction: Direction::East,
            inserter_drop_direction: None,
        })
        .unwrap();

    let mut accelerated_world = SimWorld::with_catalog(catalog.clone());
    accelerated_world
        .apply_core_command_for_tests(SimCommand::PlaceBuilding {
            def_id: "accelerated_belt".to_string(),
            origin: TilePos::new(0, 0),
            direction: Direction::East,
            inserter_drop_direction: None,
        })
        .unwrap();

    let mut fast_world = SimWorld::with_catalog(catalog);
    fast_world
        .apply_core_command_for_tests(SimCommand::PlaceBuilding {
            def_id: "fast_belt".to_string(),
            origin: TilePos::new(0, 0),
            direction: Direction::East,
            inserter_drop_direction: None,
        })
        .unwrap();

    assert_eq!(
        only_transport_line_speed_for_tests(&basic_world),
        UnitsPerTick::new(4)
    );
    assert_eq!(
        only_transport_line_speed_for_tests(&accelerated_world),
        UnitsPerTick::new(6)
    );
    assert_eq!(
        only_transport_line_speed_for_tests(&fast_world),
        UnitsPerTick::new(8)
    );
}

#[test]
fn assembler_processes_intermediate_recipes() {
    let cases = [
        ("iron_gear", TEST_IRON_PLATE, 2, TEST_IRON_GEAR, 1),
        ("copper_cable", TEST_COPPER_PLATE, 1, TEST_COPPER_CABLE, 2),
        ("iron_stick", TEST_IRON_PLATE, 1, TEST_IRON_STICK, 2),
    ];

    for (recipe, input_kind, input_amount, output_kind, output_amount) in cases {
        let mut world = SimWorld::with_catalog(catalog_for_tests());
        world
            .apply_core_command_for_tests(SimCommand::PlaceBuilding {
                def_id: "basic_assembler".to_string(),
                origin: TilePos::new(0, 0),
                direction: Direction::East,
                inserter_drop_direction: None,
            })
            .unwrap();
        let assembler = world
            .building_id_at_origin_for_tests(TilePos::new(0, 0))
            .unwrap();
        set_machine_recipe_for_tests(&mut world, assembler, Some(recipe.to_string()));
        world
            .apply_core_command_for_tests(SimCommand::InsertIntoInventory {
                building: assembler,
                role: CoreInventoryRole::Input,
                stack: CoreItemStack {
                    kind: input_kind,
                    amount: input_amount,
                },
            })
            .unwrap();

        for _ in 0..30 {
            tick_world_for_tests(&mut world);
        }

        assert_eq!(
            output_stack_for_tests(&world, assembler),
            Some(CoreItemStack {
                kind: output_kind,
                amount: output_amount,
            }),
            "{recipe} should produce the expected intermediate"
        );
    }
}

#[test]
fn mixed_speed_catalog_belts_accept_direct_drops_on_each_tile() {
    let mut world = SimWorld::with_catalog(catalog_for_tests());
    place_catalog_belt(
        &mut world,
        "basic_belt",
        TilePos::new(0, 0),
        Direction::East,
    );
    place_catalog_belt(
        &mut world,
        "accelerated_belt",
        TilePos::new(1, 0),
        Direction::East,
    );
    place_catalog_belt(&mut world, "fast_belt", TilePos::new(2, 0), Direction::East);

    assert_eq!(world.transport.line_ids_sorted().count(), 3);
    for (pos, item) in [
        (TilePos::new(0, 0), ItemKindId(11)),
        (TilePos::new(1, 0), ItemKindId(12)),
        (TilePos::new(2, 0), ItemKindId(13)),
    ] {
        world
            .apply_core_command_for_tests(SimCommand::DropItemOnBeltTile {
                pos,
                lane: 0,
                distance_numerator: 64,
                distance_denominator: 128,
                item,
            })
            .unwrap();
    }

    let visible = world
        .visible_items_for_bounds(VisibleTileBounds::new(
            TilePos::new(0, 0),
            TilePos::new(2, 0),
        ))
        .collect::<Vec<_>>();

    assert_eq!(visible.len(), 3, "{visible:?}");
    assert!(visible.iter().any(|item| item.tile == TilePos::new(0, 0)));
    assert!(visible.iter().any(|item| item.tile == TilePos::new(1, 0)));
    assert!(visible.iter().any(|item| item.tile == TilePos::new(2, 0)));
}

#[test]
fn mixed_speed_straight_catalog_belts_transfer_across_speed_boundaries() {
    let mut world = SimWorld::with_catalog(catalog_for_tests());
    place_catalog_belt(
        &mut world,
        "basic_belt",
        TilePos::new(0, 0),
        Direction::East,
    );
    place_catalog_belt(
        &mut world,
        "accelerated_belt",
        TilePos::new(1, 0),
        Direction::East,
    );
    place_catalog_belt(&mut world, "fast_belt", TilePos::new(2, 0), Direction::East);

    world
        .apply_core_command_for_tests(SimCommand::DropItemOnBeltTile {
            pos: TilePos::new(0, 0),
            lane: 0,
            distance_numerator: 64,
            distance_denominator: 128,
            item: TEST_IRON_ORE,
        })
        .unwrap();

    for _ in 0..150 {
        tick_world_for_tests(&mut world);
    }

    let visible = world
        .visible_items_for_bounds(VisibleTileBounds::new(
            TilePos::new(0, 0),
            TilePos::new(2, 0),
        ))
        .collect::<Vec<_>>();

    assert_eq!(visible.len(), 1, "{visible:?}");
    assert!(
        visible[0].tile.x >= 1,
        "item should transfer off the basic belt into a faster belt: {visible:?}"
    );
    assert!(world.removed_item_drops_for_tests().is_empty());
}

#[test]
fn straight_end_transfer_uses_transport_node_route() {
    let mut world = SimWorld::default();
    world
        .apply_core_command_for_tests(SimCommand::PlaceBelt {
            pos: TilePos::new(0, 0),
            direction: Direction::East,
            input_direction: Direction::East,
            speed: UnitsPerTick::new(4),
        })
        .unwrap();
    world
        .apply_core_command_for_tests(SimCommand::PlaceBelt {
            pos: TilePos::new(1, 0),
            direction: Direction::East,
            input_direction: Direction::East,
            speed: UnitsPerTick::new(8),
        })
        .unwrap();
    world
        .apply_core_command_for_tests(SimCommand::DropItemOnBeltTile {
            pos: TilePos::new(0, 0),
            lane: 0,
            distance_numerator: 128,
            distance_denominator: 128,
            item: TEST_IRON_ORE,
        })
        .unwrap();

    for _ in 0..40 {
        tick_world_for_tests(&mut world);
    }

    assert!(tile_has_item_in_line_window(&world, TilePos::new(1, 0)));
    assert!(
        world
            .transport
            .nodes_sorted()
            .any(|node| { node.kind == crate::transport::node::TransportNodeKind::EndTransfer })
    );
}

#[test]
fn side_load_uses_transport_node_route() {
    let mut world = SimWorld::with_catalog(catalog_for_tests());
    for (origin, direction) in [
        (TilePos::new(0, 0), Direction::East),
        (TilePos::new(1, 0), Direction::East),
        (TilePos::new(0, -1), Direction::North),
        (TilePos::new(0, 1), Direction::South),
    ] {
        place_catalog_belt(&mut world, "basic_belt", origin, direction);
    }

    assert!(world.transport.nodes_sorted().any(|node| {
        matches!(
            node.kind,
            crate::transport::node::TransportNodeKind::SideLoad { near_lane: 0 | 1 }
        )
    }));
}

#[test]
fn mixed_speed_t_junction_side_loads_into_target_belt() {
    let mut world = SimWorld::with_catalog(catalog_for_tests());
    place_catalog_belt(
        &mut world,
        "basic_belt",
        TilePos::new(-1, 0),
        Direction::East,
    );
    place_catalog_belt(
        &mut world,
        "accelerated_belt",
        TilePos::new(0, 1),
        Direction::South,
    );
    place_catalog_belt(&mut world, "fast_belt", TilePos::new(0, 0), Direction::East);
    place_catalog_belt(
        &mut world,
        "basic_belt",
        TilePos::new(1, 0),
        Direction::East,
    );

    let source_line = world
        .line_window_for_tile(TilePos::new(0, 1))
        .map(|(line_id, _, _, _)| line_id)
        .unwrap();
    let target_line = world
        .line_window_for_tile(TilePos::new(0, 0))
        .map(|(line_id, _, _, _)| line_id)
        .unwrap();
    assert_ne!(source_line, target_line);
    assert!(world.transport.interactions_sorted().any(|interaction| {
        interaction.source_line() == source_line
            && interaction.target_line() == Some(target_line)
            && interaction.target_tile() == Some(TilePos::new(0, 0))
            && interaction.kind() == BeltInteractionKind::SideLoad { near_lane: 0 }
    }));

    world
        .apply_core_command_for_tests(SimCommand::DropItemOnBeltTile {
            pos: TilePos::new(0, 1),
            lane: 0,
            distance_numerator: 128,
            distance_denominator: 128,
            item: TEST_IRON_ORE,
        })
        .unwrap();

    for _ in 0..60 {
        tick_world_for_tests(&mut world);
    }

    let target_items = world
        .visible_items_for_bounds(VisibleTileBounds::new(
            TilePos::new(0, 0),
            TilePos::new(1, 0),
        ))
        .collect::<Vec<_>>();
    assert!(
        target_items.iter().any(|item| item.item == TEST_IRON_ORE),
        "item should side-load from accelerated source onto mixed-speed target: {target_items:?}"
    );
}

#[test]
fn mixed_speed_compact_loop_and_turns_keep_items_moving() {
    let mut world = SimWorld::with_catalog(catalog_for_tests());
    for (def_id, origin, direction) in [
        ("accelerated_belt", TilePos::new(0, 1), Direction::South),
        ("fast_belt", TilePos::new(0, 0), Direction::East),
        ("basic_belt", TilePos::new(1, 0), Direction::South),
        ("accelerated_belt", TilePos::new(1, -1), Direction::West),
        ("fast_belt", TilePos::new(0, -1), Direction::North),
    ] {
        place_catalog_belt(&mut world, def_id, origin, direction);
    }

    for (pos, item) in [
        (TilePos::new(0, 1), ItemKindId(21)),
        (TilePos::new(0, -1), ItemKindId(22)),
    ] {
        world
            .apply_core_command_for_tests(SimCommand::DropItemOnBeltTile {
                pos,
                lane: 0,
                distance_numerator: 128,
                distance_denominator: 128,
                item,
            })
            .unwrap();
    }

    let mut visited_turn = false;
    let mut visited_front = false;
    for _ in 0..160 {
        tick_world_for_tests(&mut world);
        let visible = world
            .visible_items_for_bounds(VisibleTileBounds::new(
                TilePos::new(0, -1),
                TilePos::new(1, 1),
            ))
            .collect::<Vec<_>>();
        visited_turn |= visible.iter().any(|item| {
            item.item == ItemKindId(21)
                && (item.tile == TilePos::new(1, 0) || item.tile == TilePos::new(1, -1))
        });
        visited_front |= visible
            .iter()
            .any(|item| item.item == ItemKindId(22) && item.tile == TilePos::new(0, 0));
    }

    assert!(visited_turn, "top item should traverse mixed-speed turns");
    assert!(visited_front, "loop item should revisit the front belt");
    assert!(world.removed_item_drops_for_tests().is_empty());
}

#[test]
fn overlapping_buildings_are_rejected() {
    let mut world = SimWorld::with_catalog(catalog_for_tests());
    world
        .apply_core_command_for_tests(SimCommand::PlaceBuilding {
            def_id: "wooden_chest".to_string(),
            origin: TilePos::new(0, 0),
            direction: Direction::East,
            inserter_drop_direction: None,
        })
        .unwrap();

    assert_eq!(
        world.apply_core_command_for_tests(SimCommand::PlaceBuilding {
            def_id: "basic_belt".to_string(),
            origin: TilePos::new(1, 1),
            direction: Direction::East,
            inserter_drop_direction: None,
        }),
        Err(SimCommandError::OccupiedTile {
            pos: TilePos::new(1, 1)
        })
    );
}

#[test]
fn removing_core_chest_clears_snapshot_and_footprint_occupancy() {
    let mut world = SimWorld::with_catalog(catalog_for_tests());
    world
        .apply_core_command_for_tests(SimCommand::PlaceBuilding {
            def_id: "wooden_chest".to_string(),
            origin: TilePos::new(0, 0),
            direction: Direction::East,
            inserter_drop_direction: None,
        })
        .unwrap();

    world
        .apply_core_command_for_tests(SimCommand::RemoveBuilding {
            pos: TilePos::new(1, 1),
        })
        .unwrap();

    assert!(world.building_snapshots().is_empty());
    assert!(!world.is_occupied_for_tests(TilePos::new(0, 0)));
    assert!(!world.is_occupied_for_tests(TilePos::new(1, 1)));
    world
        .apply_core_command_for_tests(SimCommand::PlaceBuilding {
            def_id: "basic_belt".to_string(),
            origin: TilePos::new(1, 1),
            direction: Direction::East,
            inserter_drop_direction: None,
        })
        .unwrap();
}

#[test]
fn removing_core_belt_clears_transport_and_building_state_then_allows_replacement() {
    let mut world = SimWorld::with_catalog(catalog_for_tests());
    world
        .apply_core_command_for_tests(SimCommand::PlaceBuilding {
            def_id: "basic_belt".to_string(),
            origin: TilePos::new(0, 0),
            direction: Direction::East,
            inserter_drop_direction: None,
        })
        .unwrap();
    assert_eq!(world.active_line_ids_for_tests().len(), 1);

    apply_command_with_behavior_for_tests(
        &mut world,
        SimCommand::RemoveBuilding {
            pos: TilePos::new(0, 0),
        },
    )
    .unwrap();

    assert!(world.building_snapshots().is_empty());
    assert!(!world.is_occupied_for_tests(TilePos::new(0, 0)));
    assert_eq!(world.active_line_ids_for_tests().len(), 0);
    world
        .apply_core_command_for_tests(SimCommand::PlaceBuilding {
            def_id: "wooden_chest".to_string(),
            origin: TilePos::new(0, 0),
            direction: Direction::East,
            inserter_drop_direction: None,
        })
        .unwrap();
}

#[test]
fn core_belt_buildings_recompute_turn_input_when_neighbor_is_added() {
    let mut world = SimWorld::with_catalog(catalog_for_tests());
    world
        .apply_core_command_for_tests(SimCommand::PlaceBuilding {
            def_id: "basic_belt".to_string(),
            origin: TilePos::new(0, 0),
            direction: Direction::East,
            inserter_drop_direction: None,
        })
        .unwrap();
    world
        .apply_core_command_for_tests(SimCommand::PlaceBuilding {
            def_id: "basic_belt".to_string(),
            origin: TilePos::new(0, -1),
            direction: Direction::North,
            inserter_drop_direction: None,
        })
        .unwrap();
    world
        .apply_core_command_for_tests(SimCommand::DropItemOnBeltTile {
            pos: TilePos::new(0, 0),
            lane: 0,
            distance_numerator: 32,
            distance_denominator: 128,
            item: ItemKindId(3),
        })
        .unwrap();

    let visible = world
        .visible_items_for_bounds(VisibleTileBounds::new(
            TilePos::new(0, 0),
            TilePos::new(0, 1),
        ))
        .collect::<Vec<_>>();

    assert_eq!(visible.len(), 1);
    assert_eq!(visible[0].entry_direction, Direction::North);
    assert_eq!(visible[0].direction, Direction::East);
}

#[test]
fn core_belt_buildings_keep_t_junction_target_straight() {
    let mut world = SimWorld::with_catalog(catalog_for_tests());
    for (origin, direction) in [
        (TilePos::new(0, 0), Direction::East),
        (TilePos::new(1, 0), Direction::East),
        (TilePos::new(0, -1), Direction::North),
        (TilePos::new(0, 1), Direction::South),
    ] {
        world
            .apply_core_command_for_tests(SimCommand::PlaceBuilding {
                def_id: "basic_belt".to_string(),
                origin,
                direction,
                inserter_drop_direction: None,
            })
            .unwrap();
    }

    let target = world.topology_graph.belt(TilePos::new(0, 0)).unwrap();
    assert_eq!(target.direction, Direction::East);
    assert_eq!(target.input_direction, Direction::East);
}

#[test]
fn core_belt_t_junction_transfers_bottom_input_onto_straight_target() {
    let mut world = SimWorld::with_catalog(catalog_for_tests());
    for (origin, direction) in [
        (TilePos::new(0, 0), Direction::East),
        (TilePos::new(1, 0), Direction::East),
        (TilePos::new(0, -1), Direction::North),
        (TilePos::new(0, 1), Direction::South),
    ] {
        world
            .apply_core_command_for_tests(SimCommand::PlaceBuilding {
                def_id: "basic_belt".to_string(),
                origin,
                direction,
                inserter_drop_direction: None,
            })
            .unwrap();
    }
    world
        .apply_core_command_for_tests(SimCommand::DropItemOnBeltTile {
            pos: TilePos::new(0, -1),
            lane: 1,
            distance_numerator: 128,
            distance_denominator: 128,
            item: ItemKindId(3),
        })
        .unwrap();

    tick_world_for_tests(&mut world);

    let visible = world
        .visible_items_for_bounds(VisibleTileBounds::new(
            TilePos::new(0, 0),
            TilePos::new(1, 0),
        ))
        .collect::<Vec<_>>();

    assert!(
        visible
            .iter()
            .any(|item| item.item == ItemKindId(3) && item.direction == Direction::East),
        "bottom side input should move onto straight east target: {visible:?}"
    );
}

#[test]
fn core_belt_t_junction_transfers_top_input_onto_straight_target() {
    let mut world = SimWorld::with_catalog(catalog_for_tests());
    for (origin, direction) in [
        (TilePos::new(0, 0), Direction::East),
        (TilePos::new(1, 0), Direction::East),
        (TilePos::new(0, -1), Direction::North),
        (TilePos::new(0, 1), Direction::South),
    ] {
        world
            .apply_core_command_for_tests(SimCommand::PlaceBuilding {
                def_id: "basic_belt".to_string(),
                origin,
                direction,
                inserter_drop_direction: None,
            })
            .unwrap();
    }
    world
        .apply_core_command_for_tests(SimCommand::DropItemOnBeltTile {
            pos: TilePos::new(0, 1),
            lane: 0,
            distance_numerator: 128,
            distance_denominator: 128,
            item: ItemKindId(4),
        })
        .unwrap();

    tick_world_for_tests(&mut world);

    let visible = world
        .visible_items_for_bounds(VisibleTileBounds::new(
            TilePos::new(0, 0),
            TilePos::new(1, 0),
        ))
        .collect::<Vec<_>>();

    assert!(
        visible
            .iter()
            .any(|item| item.item == ItemKindId(4) && item.direction == Direction::East),
        "top side input should move onto straight east target: {visible:?}"
    );
}

#[test]
fn core_belt_t_junction_transfers_opposite_inputs_to_separate_target_lanes() {
    let mut world = SimWorld::with_catalog(catalog_for_tests());
    for (origin, direction) in [
        (TilePos::new(0, 0), Direction::East),
        (TilePos::new(1, 0), Direction::East),
        (TilePos::new(0, -1), Direction::North),
        (TilePos::new(0, 1), Direction::South),
    ] {
        world
            .apply_core_command_for_tests(SimCommand::PlaceBuilding {
                def_id: "basic_belt".to_string(),
                origin,
                direction,
                inserter_drop_direction: None,
            })
            .unwrap();
    }
    world
        .apply_core_command_for_tests(SimCommand::DropItemOnBeltTile {
            pos: TilePos::new(0, -1),
            lane: 1,
            distance_numerator: 128,
            distance_denominator: 128,
            item: ItemKindId(3),
        })
        .unwrap();
    world
        .apply_core_command_for_tests(SimCommand::DropItemOnBeltTile {
            pos: TilePos::new(0, 1),
            lane: 0,
            distance_numerator: 128,
            distance_denominator: 128,
            item: ItemKindId(4),
        })
        .unwrap();

    tick_world_for_tests(&mut world);

    let visible = world
        .visible_items_for_bounds(VisibleTileBounds::new(
            TilePos::new(0, 0),
            TilePos::new(1, 0),
        ))
        .collect::<Vec<_>>();

    assert!(
        visible
            .iter()
            .any(|item| item.item == ItemKindId(3) && item.lane == 1),
        "bottom input should move to lower target lane: {visible:?}"
    );
    assert!(
        visible
            .iter()
            .any(|item| item.item == ItemKindId(4) && item.lane == 0),
        "top input should move to upper target lane: {visible:?}"
    );
}

#[test]
fn core_belt_t_junction_places_right_side_input_near_right_target_edge() {
    let mut world = SimWorld::with_catalog(catalog_for_tests());
    for (origin, direction) in [
        (TilePos::new(0, 0), Direction::East),
        (TilePos::new(1, 0), Direction::East),
        (TilePos::new(0, -1), Direction::North),
        (TilePos::new(0, 1), Direction::South),
    ] {
        world
            .apply_core_command_for_tests(SimCommand::PlaceBuilding {
                def_id: "basic_belt".to_string(),
                origin,
                direction,
                inserter_drop_direction: None,
            })
            .unwrap();
    }
    world
        .apply_core_command_for_tests(SimCommand::DropItemOnBeltTile {
            pos: TilePos::new(0, -1),
            lane: 1,
            distance_numerator: 128,
            distance_denominator: 128,
            item: ItemKindId(3),
        })
        .unwrap();

    tick_world_for_tests(&mut world);

    let visible = world
        .visible_items_for_bounds(VisibleTileBounds::new(
            TilePos::new(0, 0),
            TilePos::new(0, 0),
        ))
        .collect::<Vec<_>>();
    let item = visible
        .iter()
        .find(|item| item.item == ItemKindId(3))
        .expect("side-loaded item should be visible on target tile");

    assert_eq!(item.direction, Direction::East);
    assert_eq!(item.lane, 1);
    assert!(
        item.progress_numerator >= 88,
        "right-side input should land near the right target edge instead of the center: {item:?}"
    );
}

#[test]
fn core_belt_t_junction_places_top_down_lane_zero_near_exit_edge() {
    let mut world = SimWorld::with_catalog(catalog_for_tests());
    for (origin, direction) in [
        (TilePos::new(0, 0), Direction::East),
        (TilePos::new(1, 0), Direction::East),
        (TilePos::new(0, -1), Direction::North),
        (TilePos::new(0, 1), Direction::South),
    ] {
        world
            .apply_core_command_for_tests(SimCommand::PlaceBuilding {
                def_id: "basic_belt".to_string(),
                origin,
                direction,
                inserter_drop_direction: None,
            })
            .unwrap();
    }
    world
        .apply_core_command_for_tests(SimCommand::DropItemOnBeltTile {
            pos: TilePos::new(0, 1),
            lane: 0,
            distance_numerator: 128,
            distance_denominator: 128,
            item: ItemKindId(4),
        })
        .unwrap();

    tick_world_for_tests(&mut world);

    let visible = world
        .visible_items_for_bounds(VisibleTileBounds::new(
            TilePos::new(0, 0),
            TilePos::new(0, 0),
        ))
        .collect::<Vec<_>>();
    let item = visible
        .iter()
        .find(|item| item.item == ItemKindId(4))
        .expect("side-loaded item should be visible on target tile");

    assert_eq!(item.direction, Direction::East);
    assert_eq!(item.lane, 0);
    assert!(
        item.progress_numerator >= 88,
        "top-down source lane 0 should land near the target exit edge: {item:?}"
    );
}

#[test]
fn core_belt_t_junction_drains_items_placed_on_both_side_lines() {
    let mut world = SimWorld::with_catalog(catalog_for_tests());
    for (origin, direction) in [
        (TilePos::new(0, 0), Direction::East),
        (TilePos::new(1, 0), Direction::East),
        (TilePos::new(2, 0), Direction::East),
        (TilePos::new(0, -3), Direction::North),
        (TilePos::new(0, -2), Direction::North),
        (TilePos::new(0, -1), Direction::North),
        (TilePos::new(0, 1), Direction::South),
        (TilePos::new(0, 2), Direction::South),
        (TilePos::new(0, 3), Direction::South),
    ] {
        world
            .apply_core_command_for_tests(SimCommand::PlaceBuilding {
                def_id: "basic_belt".to_string(),
                origin,
                direction,
                inserter_drop_direction: None,
            })
            .unwrap();
    }

    for lane in 0..2 {
        for _ in 0..4 {
            world
                .apply_core_command_for_tests(SimCommand::DropItemOnBeltTile {
                    pos: TilePos::new(0, -1),
                    lane,
                    distance_numerator: 64,
                    distance_denominator: 128,
                    item: ItemKindId(3),
                })
                .unwrap();
            world
                .apply_core_command_for_tests(SimCommand::DropItemOnBeltTile {
                    pos: TilePos::new(0, 1),
                    lane,
                    distance_numerator: 64,
                    distance_denominator: 128,
                    item: ItemKindId(4),
                })
                .unwrap();
        }
    }

    let initial_bottom_items = world
        .visible_items_for_bounds(VisibleTileBounds::new(
            TilePos::new(0, -3),
            TilePos::new(0, -1),
        ))
        .filter(|item| item.item == ItemKindId(3))
        .count();
    let initial_top_items = world
        .visible_items_for_bounds(VisibleTileBounds::new(
            TilePos::new(0, 1),
            TilePos::new(0, 3),
        ))
        .filter(|item| item.item == ItemKindId(4))
        .count();
    assert!(initial_bottom_items >= 4);
    assert!(initial_top_items >= 4);

    for _ in 0..120 {
        tick_world_for_tests(&mut world);
    }

    let remaining_bottom_items = world
        .visible_items_for_bounds(VisibleTileBounds::new(
            TilePos::new(0, -3),
            TilePos::new(0, -1),
        ))
        .filter(|item| item.item == ItemKindId(3))
        .count();
    let remaining_top_items = world
        .visible_items_for_bounds(VisibleTileBounds::new(
            TilePos::new(0, 1),
            TilePos::new(0, 3),
        ))
        .filter(|item| item.item == ItemKindId(4))
        .count();
    let target_items = world
        .visible_items_for_bounds(VisibleTileBounds::new(
            TilePos::new(0, 0),
            TilePos::new(2, 0),
        ))
        .collect::<Vec<_>>();

    assert!(
        remaining_bottom_items < initial_bottom_items,
        "bottom side line should drain into the straight target"
    );
    assert!(
        remaining_top_items < initial_top_items,
        "top side line should drain into the straight target"
    );
    assert!(
        target_items
            .iter()
            .any(|item| item.item == ItemKindId(3) && item.direction == Direction::East),
        "bottom items should be visible on the east target: {target_items:?}"
    );
    assert!(
        target_items
            .iter()
            .any(|item| item.item == ItemKindId(4) && item.direction == Direction::East),
        "top items should be visible on the east target: {target_items:?}"
    );
}

#[test]
fn core_belt_t_junction_builds_two_side_load_interactions() {
    let mut world = SimWorld::with_catalog(catalog_for_tests());
    for (origin, direction) in [
        (TilePos::new(0, 0), Direction::East),
        (TilePos::new(1, 0), Direction::East),
        (TilePos::new(0, -1), Direction::North),
        (TilePos::new(0, 1), Direction::South),
    ] {
        world
            .apply_core_command_for_tests(SimCommand::PlaceBuilding {
                def_id: "basic_belt".to_string(),
                origin,
                direction,
                inserter_drop_direction: None,
            })
            .unwrap();
    }

    let side_load_lanes = world
        .transport
        .interactions_sorted()
        .filter(|interaction| interaction.target_tile() == Some(TilePos::new(0, 0)))
        .filter_map(|interaction| match interaction.kind() {
            BeltInteractionKind::SideLoad { near_lane } => Some(near_lane),
            _ => None,
        })
        .collect::<BTreeSet<_>>();

    assert_eq!(side_load_lanes, BTreeSet::from([0, 1]));
}

#[test]
fn core_belt_t_junction_stays_straight_regardless_of_side_input_order() {
    for belts in [
        [
            (TilePos::new(0, -1), Direction::North),
            (TilePos::new(0, 0), Direction::East),
            (TilePos::new(0, 1), Direction::South),
            (TilePos::new(1, 0), Direction::East),
        ],
        [
            (TilePos::new(0, 1), Direction::South),
            (TilePos::new(0, 0), Direction::East),
            (TilePos::new(0, -1), Direction::North),
            (TilePos::new(1, 0), Direction::East),
        ],
        [
            (TilePos::new(0, 0), Direction::East),
            (TilePos::new(1, 0), Direction::East),
            (TilePos::new(0, -1), Direction::North),
            (TilePos::new(0, 1), Direction::South),
        ],
    ] {
        let mut world = SimWorld::with_catalog(catalog_for_tests());
        for (origin, direction) in belts {
            world
                .apply_core_command_for_tests(SimCommand::PlaceBuilding {
                    def_id: "basic_belt".to_string(),
                    origin,
                    direction,
                    inserter_drop_direction: None,
                })
                .unwrap();
        }

        let target = world.topology_graph.belt(TilePos::new(0, 0)).unwrap();
        assert_eq!(target.direction, Direction::East);
        assert_eq!(target.input_direction, Direction::East);
    }
}

#[test]
fn place_inserter_preserves_pickup_and_drop_directions() {
    let mut world = SimWorld::with_catalog(catalog_for_tests());

    world
        .apply_core_command_for_tests(SimCommand::PlaceBuilding {
            def_id: "basic_inserter".to_string(),
            origin: TilePos::new(0, 0),
            direction: Direction::South,
            inserter_drop_direction: Some(Direction::East),
        })
        .unwrap();

    let snapshots = world.building_snapshots();
    assert_eq!(snapshots.len(), 1);
    assert_eq!(
        snapshots[0].state,
        SimBuildingState::Inserter(InserterRuntime {
            pickup_direction: Direction::South,
            drop_direction: Direction::East,
            cooldown_remaining_ticks: 0,
            carried: None,
        })
    );
}

#[test]
fn digest_differs_when_core_building_exists() {
    let empty = SimWorld::with_catalog(catalog_for_tests());
    let mut with_chest = SimWorld::with_catalog(catalog_for_tests());
    with_chest
        .apply_core_command_for_tests(SimCommand::PlaceBuilding {
            def_id: "wooden_chest".to_string(),
            origin: TilePos::new(0, 0),
            direction: Direction::East,
            inserter_drop_direction: None,
        })
        .unwrap();

    assert_ne!(empty.digest(), with_chest.digest());
}

#[test]
fn digest_differs_for_different_core_building_inventory_contents() {
    let mut empty_chest = SimWorld::with_catalog(catalog_for_tests());
    empty_chest
        .apply_core_command_for_tests(SimCommand::PlaceBuilding {
            def_id: "wooden_chest".to_string(),
            origin: TilePos::new(0, 0),
            direction: Direction::East,
            inserter_drop_direction: None,
        })
        .unwrap();

    let mut filled_chest = SimWorld::with_catalog(catalog_for_tests());
    filled_chest
        .apply_core_command_for_tests(SimCommand::PlaceBuilding {
            def_id: "wooden_chest".to_string(),
            origin: TilePos::new(0, 0),
            direction: Direction::East,
            inserter_drop_direction: None,
        })
        .unwrap();
    let building = filled_chest.building_snapshots()[0].id;
    filled_chest
        .apply_core_command_for_tests(SimCommand::InsertIntoInventory {
            building,
            role: CoreInventoryRole::Storage,
            stack: CoreItemStack {
                kind: TEST_IRON_ORE,
                amount: 3,
            },
        })
        .unwrap();

    assert_ne!(empty_chest.digest(), filled_chest.digest());
}

#[test]
fn digest_differs_for_same_technical_kind_with_different_definition_ids() {
    fn storage_def(id: &str) -> CoreBuildingDef {
        CoreBuildingDef {
            id: id.to_string(),
            kind: CoreBuildingKind::Passive,
            footprint: vec![(0, 0)],
            rotate_footprint: false,
            inputs: Vec::new(),
            outputs: Vec::new(),
            inventories: Vec::new(),
            inserter_deposit_limits: Vec::new(),
            behavior: CoreBuildingBehavior::noop("test:storage"),
            power: PowerDef::none(),
        }
    }

    fn build_world(def_id: &str) -> SimWorld {
        let catalog = CoreCatalog::new(
            Vec::new(),
            vec![test_terrain()],
            vec![storage_def("small_box"), storage_def("big_box")],
        );
        let mut world = SimWorld::with_catalog(catalog);
        world
            .apply_core_command_for_tests(SimCommand::PlaceBuilding {
                def_id: def_id.to_string(),
                origin: TilePos::new(0, 0),
                direction: Direction::East,
                inserter_drop_direction: None,
            })
            .unwrap();
        world
    }

    let small_box = build_world("small_box");
    let big_box = build_world("big_box");

    assert_ne!(small_box.digest(), big_box.digest());
}

#[test]
fn inserter_runtime_uses_placed_definition_cooldown() {
    fn inserter_def(id: &str, cooldown_ticks: u32) -> CoreBuildingDef {
        CoreBuildingDef {
            id: id.to_string(),
            kind: CoreBuildingKind::Inserter,
            footprint: vec![(0, 0)],
            rotate_footprint: true,
            inputs: Vec::new(),
            outputs: Vec::new(),
            inventories: Vec::new(),
            inserter_deposit_limits: Vec::new(),
            behavior: CoreBuildingBehavior::inserter(cooldown_ticks),
            power: PowerDef::none(),
        }
    }

    let catalog = CoreCatalog::new(
        Vec::new(),
        vec![test_terrain()],
        vec![
            inserter_def("fast_inserter", 3),
            inserter_def("slow_inserter", 9),
            CoreBuildingDef {
                id: "source_box".to_string(),
                kind: CoreBuildingKind::Passive,
                footprint: vec![(0, 0)],
                rotate_footprint: false,
                inputs: Vec::new(),
                outputs: Vec::new(),
                inventories: vec![CoreInventoryDef {
                    role: CoreInventoryRole::Storage,
                    slots: 1,
                    max_stack: 100,
                    stack_limits: Vec::new(),
                    comfortable_weight_limit_grams: None,
                    hard_weight_limit_grams: None,
                    accepts: Vec::new(),
                    ..CoreInventoryDef::new(CoreInventoryRole::Storage, 1, 100)
                }],
                inserter_deposit_limits: Vec::new(),
                behavior: CoreBuildingBehavior::noop("test:storage"),
                power: PowerDef::none(),
            },
        ],
    );
    let mut world = SimWorld::with_catalog(catalog);
    world
        .apply_core_command_for_tests(SimCommand::PlaceBuilding {
            def_id: "source_box".to_string(),
            origin: TilePos::new(0, 0),
            direction: Direction::East,
            inserter_drop_direction: None,
        })
        .unwrap();
    let source = world
        .building_id_at_origin_for_tests(TilePos::new(0, 0))
        .unwrap();
    world
        .apply_core_command_for_tests(SimCommand::InsertIntoInventory {
            building: source,
            role: CoreInventoryRole::Storage,
            stack: CoreItemStack {
                kind: TEST_IRON_ORE,
                amount: 1,
            },
        })
        .unwrap();
    world
        .apply_core_command_for_tests(SimCommand::PlaceBuilding {
            def_id: "slow_inserter".to_string(),
            origin: TilePos::new(1, 0),
            direction: Direction::West,
            inserter_drop_direction: Some(Direction::East),
        })
        .unwrap();

    tick_world_for_tests(&mut world);

    let inserter = world
        .building_snapshots()
        .into_iter()
        .find(|snapshot| snapshot.def_id == "slow_inserter")
        .unwrap();
    assert!(matches!(
        inserter.state,
        SimBuildingState::Inserter(InserterRuntime {
            carried: Some(CoreItemStack {
                kind: TEST_IRON_ORE,
                amount: 1,
            }),
            cooldown_remaining_ticks: 9,
            ..
        })
    ));
}

#[test]
fn miner_to_furnace_chain_produces_iron_plate() {
    let mut world = SimWorld::with_catalog(catalog_for_tests());
    world.seed_resource_for_tests(TilePos::new(2, 2), TEST_IRON_ORE, 10);

    world
        .apply_core_command_for_tests(SimCommand::PlaceBuilding {
            def_id: "basic_miner".to_string(),
            origin: TilePos::new(0, 0),
            direction: Direction::East,
            inserter_drop_direction: None,
        })
        .unwrap();
    world
        .apply_core_command_for_tests(SimCommand::PlaceBuilding {
            def_id: "basic_inserter".to_string(),
            origin: TilePos::new(5, 2),
            direction: Direction::West,
            inserter_drop_direction: Some(Direction::East),
        })
        .unwrap();
    world
        .apply_core_command_for_tests(SimCommand::PlaceBuilding {
            def_id: "wooden_chest".to_string(),
            origin: TilePos::new(6, 1),
            direction: Direction::East,
            inserter_drop_direction: None,
        })
        .unwrap();
    world
        .apply_core_command_for_tests(SimCommand::PlaceBuilding {
            def_id: "basic_inserter".to_string(),
            origin: TilePos::new(8, 2),
            direction: Direction::West,
            inserter_drop_direction: Some(Direction::East),
        })
        .unwrap();
    world
        .apply_core_command_for_tests(SimCommand::PlaceBuilding {
            def_id: "stone_furnace".to_string(),
            origin: TilePos::new(9, 1),
            direction: Direction::East,
            inserter_drop_direction: None,
        })
        .unwrap();

    let miner = world
        .building_id_at_origin_for_tests(TilePos::new(0, 0))
        .unwrap();
    let furnace = world
        .building_id_at_origin_for_tests(TilePos::new(9, 1))
        .unwrap();
    select_iron_plate_recipe(&mut world, furnace);
    world
        .insert_into_inventory_for_tests(
            miner,
            CoreInventoryRole::Fuel,
            CoreItemStack {
                kind: TEST_COAL,
                amount: 1,
            },
        )
        .unwrap();
    world
        .insert_into_inventory_for_tests(
            furnace,
            CoreInventoryRole::Fuel,
            CoreItemStack {
                kind: TEST_COAL,
                amount: 1,
            },
        )
        .unwrap();

    for _ in 0..360 {
        tick_world_for_tests(&mut world);
    }

    let furnace = world
        .building_snapshots()
        .into_iter()
        .find(|snapshot| {
            snapshot.kind == CoreBuildingKind::Machine && snapshot.def_id == "stone_furnace"
        })
        .unwrap();
    let output = furnace
        .inventories
        .iter()
        .find(|inventory| inventory.role == CoreInventoryRole::Output)
        .unwrap();
    assert!(matches!(
        output.slots[0],
        Some(CoreItemStack {
            kind: TEST_IRON_PLATE,
            amount
        }) if amount >= 1
    ));
}

#[test]
fn inserter_drop_into_furnace_input_stops_at_configured_cap() {
    let mut catalog = catalog_for_tests();
    let furnace = catalog
        .buildings
        .iter_mut()
        .find(|building| building.id == "stone_furnace")
        .unwrap();
    furnace.inserter_deposit_limits = vec![CoreInserterDepositLimit {
        role: CoreInventoryRole::Input,
        item: TEST_IRON_ORE,
        max_amount: 5,
    }];
    let mut world = SimWorld::with_catalog(catalog);
    world
        .apply_core_command_for_tests(SimCommand::PlaceBuilding {
            def_id: "stone_furnace".to_string(),
            origin: TilePos::new(0, 0),
            direction: Direction::East,
            inserter_drop_direction: None,
        })
        .unwrap();
    let furnace = world
        .building_id_at_origin_for_tests(TilePos::new(0, 0))
        .unwrap();
    world
        .insert_into_inventory_for_tests(
            furnace,
            CoreInventoryRole::Input,
            CoreItemStack {
                kind: TEST_IRON_ORE,
                amount: 5,
            },
        )
        .unwrap();
    world
        .apply_core_command_for_tests(SimCommand::PlaceBuilding {
            def_id: "basic_inserter".to_string(),
            origin: TilePos::new(-1, 1),
            direction: Direction::East,
            inserter_drop_direction: Some(Direction::East),
        })
        .unwrap();
    let inserter_id = world
        .building_snapshots()
        .into_iter()
        .find(|snapshot| snapshot.kind == CoreBuildingKind::Inserter)
        .unwrap()
        .id;
    let inserter_building = world.buildings.get(&inserter_id).unwrap().clone();
    let SimBuildingState::Inserter(inserter) = inserter_building.state.clone() else {
        panic!("expected inserter");
    };

    let accepted = world.try_inserter_drop(
        &inserter_building,
        &inserter,
        CoreItemStack {
            kind: TEST_IRON_ORE,
            amount: 1,
        },
        &mut SimDiff::default(),
    );

    assert!(!accepted);
    let snapshot = world
        .building_snapshots()
        .into_iter()
        .find(|snapshot| snapshot.id == furnace)
        .unwrap();
    let input = snapshot
        .inventories
        .iter()
        .find(|inventory| inventory.role == CoreInventoryRole::Input)
        .unwrap();
    assert_eq!(
        input.slots[0],
        Some(CoreItemStack {
            kind: TEST_IRON_ORE,
            amount: 5,
        })
    );
}

#[test]
fn manual_insert_can_exceed_inserter_deposit_cap_up_to_machine_capacity() {
    let mut catalog = catalog_for_tests();
    let furnace = catalog
        .buildings
        .iter_mut()
        .find(|building| building.id == "stone_furnace")
        .unwrap();
    furnace.inserter_deposit_limits = vec![CoreInserterDepositLimit {
        role: CoreInventoryRole::Input,
        item: TEST_IRON_ORE,
        max_amount: 5,
    }];
    let mut world = SimWorld::with_catalog(catalog);
    world
        .apply_core_command_for_tests(SimCommand::PlaceBuilding {
            def_id: "stone_furnace".to_string(),
            origin: TilePos::new(0, 0),
            direction: Direction::East,
            inserter_drop_direction: None,
        })
        .unwrap();
    let furnace = world
        .building_id_at_origin_for_tests(TilePos::new(0, 0))
        .unwrap();

    world
        .insert_into_inventory_atomic(
            furnace,
            CoreInventoryRole::Input,
            CoreItemStack {
                kind: TEST_IRON_ORE,
                amount: 100,
            },
        )
        .unwrap();

    let snapshot = world
        .building_snapshots()
        .into_iter()
        .find(|snapshot| snapshot.id == furnace)
        .unwrap();
    let input = snapshot
        .inventories
        .iter()
        .find(|inventory| inventory.role == CoreInventoryRole::Input)
        .unwrap();
    assert_eq!(
        input.slots[0],
        Some(CoreItemStack {
            kind: TEST_IRON_ORE,
            amount: 100,
        })
    );
}

#[test]
fn inserter_fuel_cap_keeps_carried_item_instead_of_spilling_to_surface() {
    let mut catalog = catalog_for_tests();
    let furnace = catalog
        .buildings
        .iter_mut()
        .find(|building| building.id == "stone_furnace")
        .unwrap();
    furnace.inserter_deposit_limits = vec![CoreInserterDepositLimit {
        role: CoreInventoryRole::Fuel,
        item: TEST_COAL,
        max_amount: 15,
    }];
    let mut world = SimWorld::with_catalog(catalog);
    world
        .apply_core_command_for_tests(SimCommand::PlaceBuilding {
            def_id: "stone_furnace".to_string(),
            origin: TilePos::new(0, 0),
            direction: Direction::East,
            inserter_drop_direction: None,
        })
        .unwrap();
    let furnace = world
        .building_id_at_origin_for_tests(TilePos::new(0, 0))
        .unwrap();
    world
        .insert_into_inventory_for_tests(
            furnace,
            CoreInventoryRole::Fuel,
            CoreItemStack {
                kind: TEST_COAL,
                amount: 15,
            },
        )
        .unwrap();
    world
        .apply_core_command_for_tests(SimCommand::PlaceBuilding {
            def_id: "basic_inserter".to_string(),
            origin: TilePos::new(-2, 1),
            direction: Direction::East,
            inserter_drop_direction: Some(Direction::East),
        })
        .unwrap();
    let inserter_id = world
        .building_snapshots()
        .into_iter()
        .find(|snapshot| snapshot.kind == CoreBuildingKind::Inserter)
        .unwrap()
        .id;
    let inserter_building = world.buildings.get(&inserter_id).unwrap().clone();
    let SimBuildingState::Inserter(inserter) = inserter_building.state.clone() else {
        panic!("expected inserter");
    };

    let accepted = world.try_inserter_drop(
        &inserter_building,
        &inserter,
        CoreItemStack {
            kind: TEST_COAL,
            amount: 1,
        },
        &mut SimDiff::default(),
    );

    assert!(!accepted);
    let (_removed_drops, surface_drops) = world.take_pending_item_drops();
    assert!(surface_drops.is_empty());
}

fn build_complete_factory_loop_world(fuel_amount: u32) -> (SimWorld, crate::ids::BuildingId) {
    let mut world = SimWorld::with_catalog(catalog_for_tests());
    world.seed_resource_for_tests(TilePos::new(2, 2), TEST_IRON_ORE, 10);

    world
        .apply_core_command_for_tests(SimCommand::PlaceBuilding {
            def_id: "basic_miner".to_string(),
            origin: TilePos::new(0, 0),
            direction: Direction::East,
            inserter_drop_direction: None,
        })
        .unwrap();
    world
        .apply_core_command_for_tests(SimCommand::PlaceBuilding {
            def_id: "basic_inserter".to_string(),
            origin: TilePos::new(5, 2),
            direction: Direction::West,
            inserter_drop_direction: Some(Direction::East),
        })
        .unwrap();
    world
        .apply_core_command_for_tests(SimCommand::PlaceBuilding {
            def_id: "wooden_chest".to_string(),
            origin: TilePos::new(6, 1),
            direction: Direction::East,
            inserter_drop_direction: None,
        })
        .unwrap();
    world
        .apply_core_command_for_tests(SimCommand::PlaceBuilding {
            def_id: "basic_inserter".to_string(),
            origin: TilePos::new(8, 2),
            direction: Direction::West,
            inserter_drop_direction: Some(Direction::East),
        })
        .unwrap();
    world
        .apply_core_command_for_tests(SimCommand::PlaceBuilding {
            def_id: "stone_furnace".to_string(),
            origin: TilePos::new(9, 1),
            direction: Direction::East,
            inserter_drop_direction: None,
        })
        .unwrap();
    world
        .apply_core_command_for_tests(SimCommand::PlaceBuilding {
            def_id: "basic_inserter".to_string(),
            origin: TilePos::new(12, 2),
            direction: Direction::West,
            inserter_drop_direction: Some(Direction::East),
        })
        .unwrap();
    world
        .apply_core_command_for_tests(SimCommand::PlaceBuilding {
            def_id: "basic_assembler".to_string(),
            origin: TilePos::new(13, 1),
            direction: Direction::East,
            inserter_drop_direction: None,
        })
        .unwrap();

    let miner = world
        .building_id_at_origin_for_tests(TilePos::new(0, 0))
        .unwrap();
    let furnace = world
        .building_id_at_origin_for_tests(TilePos::new(9, 1))
        .unwrap();
    let assembler = world
        .building_id_at_origin_for_tests(TilePos::new(13, 1))
        .unwrap();
    select_iron_plate_recipe(&mut world, furnace);
    set_machine_recipe_for_tests(&mut world, assembler, Some("iron_gear".to_string()));
    world
        .insert_into_inventory_for_tests(
            miner,
            CoreInventoryRole::Fuel,
            CoreItemStack {
                kind: TEST_COAL,
                amount: fuel_amount,
            },
        )
        .unwrap();
    world
        .insert_into_inventory_for_tests(
            furnace,
            CoreInventoryRole::Fuel,
            CoreItemStack {
                kind: TEST_COAL,
                amount: fuel_amount,
            },
        )
        .unwrap();

    (world, assembler)
}

#[test]
fn complete_factory_loop_produces_assembler_intermediate() {
    let (mut world, assembler) = build_complete_factory_loop_world(1);

    for _ in 0..900 {
        tick_world_for_tests(&mut world);
    }

    assert!(matches!(
        output_stack_for_tests(&world, assembler),
        Some(CoreItemStack {
            kind: TEST_IRON_GEAR,
            amount
        }) if amount >= 1
    ));
}

#[cfg(any())]
#[test]
fn complete_factory_loop_survives_save_load() {
    let resource_tile = TilePos::new(2, 2);
    let (mut world, assembler) = build_complete_factory_loop_world(4);

    for _ in 0..520 {
        tick_world_for_tests(&mut world);
    }
    let before = output_amount(&world, assembler, TEST_IRON_GEAR);
    assert!(before >= 1, "factory should produce gear before save");
    let before_tick = world.snapshot().tick;
    let resource_before = world.resource_amount_for_tests(resource_tile).unwrap();

    let catalog = catalog_for_tests();
    let save = crate::save::codec::save_from_world(123, &world);
    let encoded = crate::save::codec::encode_save(&save).unwrap();
    let decoded = crate::save::codec::decode_save(&encoded).unwrap();
    let mut restored = crate::save::codec::world_from_save(decoded, catalog).unwrap();

    assert_eq!(restored.snapshot().tick, before_tick);
    for _ in 0..260 {
        tick_world_for_tests(&mut restored);
    }

    let after = output_amount(&restored, assembler, TEST_IRON_GEAR);
    assert!(
        after > before,
        "factory should keep producing after load: before={before}, after={after}"
    );
    let resource_after = restored.resource_amount_for_tests(resource_tile).unwrap();
    assert!(
        resource_after < resource_before,
        "restored factory should keep mining after load: before={resource_before}, after={resource_after}"
    );
    assert!(restored.snapshot().tick > before_tick);
}

#[test]
fn miner_distributes_extraction_across_matching_resource_tiles() {
    let mut world = SimWorld::with_catalog(catalog_for_tests());
    let resources = [TilePos::new(1, 1), TilePos::new(2, 2), TilePos::new(3, 3)];
    for pos in resources {
        world.seed_resource_for_tests(pos, TEST_IRON_ORE, 10);
    }
    world
        .apply_core_command_for_tests(SimCommand::PlaceBuilding {
            def_id: "basic_miner".to_string(),
            origin: TilePos::new(0, 0),
            direction: Direction::East,
            inserter_drop_direction: None,
        })
        .unwrap();
    let miner = world.building_snapshots()[0].id;
    world
        .insert_into_inventory_for_tests(
            miner,
            CoreInventoryRole::Fuel,
            CoreItemStack {
                kind: TEST_COAL,
                amount: 4,
            },
        )
        .unwrap();

    for _ in 0..30 {
        for _ in 0..60 {
            tick_world_for_tests(&mut world);
        }
        drain_output_for_tests(&mut world, miner);
    }

    let remaining = resources
        .map(|pos| world.resource_amount_for_tests(pos).unwrap())
        .to_vec();
    assert!(
        remaining.iter().all(|amount| *amount < 10),
        "expected every resource tile to be mined at least once, got {remaining:?}"
    );
    let mined = remaining
        .iter()
        .map(|amount| 10 - amount)
        .collect::<Vec<_>>();
    let min = mined.iter().copied().min().unwrap();
    let max = mined.iter().copied().max().unwrap();
    assert!(
        max - min <= 8,
        "expected pseudo-random mining to avoid starving a tile, mined {mined:?}"
    );
}

#[test]
fn miner_continues_with_other_resource_tiles_after_one_is_depleted() {
    let mut world = SimWorld::with_catalog(catalog_for_tests());
    let depleted = TilePos::new(1, 1);
    let remaining_a = TilePos::new(2, 2);
    let remaining_b = TilePos::new(3, 3);
    world.seed_resource_for_tests(depleted, TEST_IRON_ORE, 0);
    world.seed_resource_for_tests(remaining_a, TEST_IRON_ORE, 5);
    world.seed_resource_for_tests(remaining_b, TEST_IRON_ORE, 5);
    world
        .apply_core_command_for_tests(SimCommand::PlaceBuilding {
            def_id: "basic_miner".to_string(),
            origin: TilePos::new(0, 0),
            direction: Direction::East,
            inserter_drop_direction: None,
        })
        .unwrap();
    let miner = world.building_snapshots()[0].id;
    world
        .insert_into_inventory_for_tests(
            miner,
            CoreInventoryRole::Fuel,
            CoreItemStack {
                kind: TEST_COAL,
                amount: 1,
            },
        )
        .unwrap();

    for _ in 0..60 {
        tick_world_for_tests(&mut world);
    }

    let snapshot = world
        .building_snapshots()
        .into_iter()
        .find(|snapshot| snapshot.id == miner)
        .unwrap();
    assert_ne!(
        machine_runtime_for_tests(&snapshot).status,
        MachineStatus::NoMatchingResource
    );
    assert_eq!(world.resource_amount_for_tests(depleted), Some(0));
    assert!(
        world.resource_amount_for_tests(remaining_a).unwrap() < 5
            || world.resource_amount_for_tests(remaining_b).unwrap() < 5
    );
}

#[test]
fn miner_distributes_extraction_across_mixed_resource_types() {
    let mut world = SimWorld::with_catalog(catalog_for_tests());
    let resources = [
        (TilePos::new(1, 1), TEST_IRON_ORE),
        (TilePos::new(2, 2), TEST_COPPER_ORE),
        (TilePos::new(3, 3), TEST_COAL),
    ];
    for (pos, kind) in resources {
        world.seed_resource_for_tests(pos, kind, 10);
    }
    world
        .apply_core_command_for_tests(SimCommand::PlaceBuilding {
            def_id: "basic_miner".to_string(),
            origin: TilePos::new(0, 0),
            direction: Direction::East,
            inserter_drop_direction: None,
        })
        .unwrap();
    let miner = world.building_snapshots()[0].id;
    world
        .insert_into_inventory_for_tests(
            miner,
            CoreInventoryRole::Fuel,
            CoreItemStack {
                kind: TEST_COAL,
                amount: 4,
            },
        )
        .unwrap();

    for _ in 0..30 {
        for _ in 0..60 {
            tick_world_for_tests(&mut world);
        }
        drain_output_for_tests(&mut world, miner);
    }

    let remaining = resources
        .map(|(pos, _)| world.resource_amount_for_tests(pos).unwrap())
        .to_vec();
    assert!(
        remaining.iter().all(|amount| *amount < 10),
        "expected miner to extract every resource type, got remaining {remaining:?}"
    );
    let mined = remaining
        .iter()
        .map(|amount| 10 - amount)
        .collect::<Vec<_>>();
    let min = mined.iter().copied().min().unwrap();
    let max = mined.iter().copied().max().unwrap();
    assert!(
        max - min <= 8,
        "expected mixed resource extraction to stay roughly distributed, mined {mined:?}"
    );
}

#[test]
fn fuel_starved_miner_keeps_selected_resource_recipe_stable() {
    let mut world = SimWorld::with_catalog(catalog_for_tests());
    world.seed_resource_for_tests(TilePos::new(1, 1), TEST_IRON_ORE, 10);
    world.seed_resource_for_tests(TilePos::new(2, 2), TEST_COPPER_ORE, 10);
    world.seed_resource_for_tests(TilePos::new(3, 3), TEST_COAL, 10);
    world
        .apply_core_command_for_tests(SimCommand::PlaceBuilding {
            def_id: "basic_miner".to_string(),
            origin: TilePos::new(0, 0),
            direction: Direction::East,
            inserter_drop_direction: None,
        })
        .unwrap();
    let miner = world.building_snapshots()[0].id;

    let mut recipes = Vec::new();
    for _ in 0..12 {
        tick_world_for_tests(&mut world);
        let snapshot = world
            .building_snapshots()
            .into_iter()
            .find(|snapshot| snapshot.id == miner)
            .unwrap();
        let machine = machine_runtime_for_tests(&snapshot);
        assert_eq!(machine.status, MachineStatus::MissingFuel);
        recipes.push(machine.active_recipe);
    }

    assert!(
        recipes.windows(2).all(|pair| pair[0] == pair[1]),
        "expected fuel-starved miner recipe to stay stable, got {recipes:?}"
    );
}

#[test]
fn miner_switches_mixed_resource_type_when_current_recipe_resource_is_depleted() {
    let mut world = SimWorld::with_catalog(catalog_for_tests());
    let depleted_iron = TilePos::new(1, 1);
    let copper = TilePos::new(2, 2);
    let coal = TilePos::new(3, 3);
    world.seed_resource_for_tests(depleted_iron, TEST_IRON_ORE, 0);
    world.seed_resource_for_tests(copper, TEST_COPPER_ORE, 3);
    world.seed_resource_for_tests(coal, TEST_COAL, 3);
    world
        .apply_core_command_for_tests(SimCommand::PlaceBuilding {
            def_id: "basic_miner".to_string(),
            origin: TilePos::new(0, 0),
            direction: Direction::East,
            inserter_drop_direction: None,
        })
        .unwrap();
    let miner = world.building_snapshots()[0].id;
    world
        .insert_into_inventory_for_tests(
            miner,
            CoreInventoryRole::Fuel,
            CoreItemStack {
                kind: TEST_COAL,
                amount: 1,
            },
        )
        .unwrap();

    for _ in 0..60 {
        tick_world_for_tests(&mut world);
    }

    let snapshot = world
        .building_snapshots()
        .into_iter()
        .find(|snapshot| snapshot.id == miner)
        .unwrap();
    assert_ne!(
        machine_runtime_for_tests(&snapshot).status,
        MachineStatus::NoMatchingResource
    );
    assert_eq!(world.resource_amount_for_tests(depleted_iron), Some(0));
    assert!(
        world.resource_amount_for_tests(copper).unwrap() < 3
            || world.resource_amount_for_tests(coal).unwrap() < 3
    );
}

#[test]
fn miner_blocks_when_single_output_slot_is_full() {
    let mut world = SimWorld::with_catalog(catalog_for_tests());
    let iron = TilePos::new(1, 1);
    world.seed_resource_for_tests(iron, TEST_IRON_ORE, 10);
    world
        .apply_core_command_for_tests(SimCommand::PlaceBuilding {
            def_id: "basic_miner".to_string(),
            origin: TilePos::new(0, 0),
            direction: Direction::East,
            inserter_drop_direction: None,
        })
        .unwrap();
    let miner = world.building_snapshots()[0].id;
    world
        .insert_into_inventory_for_tests(
            miner,
            CoreInventoryRole::Fuel,
            CoreItemStack {
                kind: TEST_COAL,
                amount: 1,
            },
        )
        .unwrap();

    for _ in 0..60 {
        tick_world_for_tests(&mut world);
    }
    assert_eq!(
        output_stack_for_tests(&world, miner),
        Some(CoreItemStack {
            kind: TEST_IRON_ORE,
            amount: 1,
        })
    );
    let remaining_after_first = world.resource_amount_for_tests(iron).unwrap();

    tick_world_for_tests(&mut world);

    let snapshot = world
        .building_snapshots()
        .into_iter()
        .find(|snapshot| snapshot.id == miner)
        .unwrap();
    assert_eq!(
        machine_runtime_for_tests(&snapshot).status,
        MachineStatus::OutputBlocked
    );
    assert_eq!(
        world.resource_amount_for_tests(iron),
        Some(remaining_after_first)
    );
}

#[test]
fn miner_does_not_switch_resource_type_while_output_contains_previous_type() {
    let mut world = SimWorld::with_catalog(catalog_for_tests());
    let iron = TilePos::new(1, 1);
    let copper = TilePos::new(2, 2);
    world.seed_resource_for_tests(iron, TEST_IRON_ORE, 1);
    world
        .apply_core_command_for_tests(SimCommand::PlaceBuilding {
            def_id: "basic_miner".to_string(),
            origin: TilePos::new(0, 0),
            direction: Direction::East,
            inserter_drop_direction: None,
        })
        .unwrap();
    let miner = world.building_snapshots()[0].id;
    world
        .insert_into_inventory_for_tests(
            miner,
            CoreInventoryRole::Fuel,
            CoreItemStack {
                kind: TEST_COAL,
                amount: 1,
            },
        )
        .unwrap();

    for _ in 0..60 {
        tick_world_for_tests(&mut world);
    }
    assert_eq!(
        output_stack_for_tests(&world, miner),
        Some(CoreItemStack {
            kind: TEST_IRON_ORE,
            amount: 1,
        })
    );
    world.seed_resource_for_tests(copper, TEST_COPPER_ORE, 3);

    tick_world_for_tests(&mut world);

    let snapshot = world
        .building_snapshots()
        .into_iter()
        .find(|snapshot| snapshot.id == miner)
        .unwrap();
    assert_eq!(
        machine_runtime_for_tests(&snapshot).status,
        MachineStatus::OutputBlocked
    );
    assert_eq!(world.resource_amount_for_tests(copper), Some(3));
}

#[test]
fn miner_can_switch_resource_type_after_output_is_drained() {
    let mut world = SimWorld::with_catalog(catalog_for_tests());
    let iron = TilePos::new(1, 1);
    let copper = TilePos::new(2, 2);
    world.seed_resource_for_tests(iron, TEST_IRON_ORE, 1);
    world
        .apply_core_command_for_tests(SimCommand::PlaceBuilding {
            def_id: "basic_miner".to_string(),
            origin: TilePos::new(0, 0),
            direction: Direction::East,
            inserter_drop_direction: None,
        })
        .unwrap();
    let miner = world.building_snapshots()[0].id;
    world
        .insert_into_inventory_for_tests(
            miner,
            CoreInventoryRole::Fuel,
            CoreItemStack {
                kind: TEST_COAL,
                amount: 1,
            },
        )
        .unwrap();

    for _ in 0..60 {
        tick_world_for_tests(&mut world);
    }
    world.seed_resource_for_tests(copper, TEST_COPPER_ORE, 3);
    drain_output_for_tests(&mut world, miner);
    for _ in 0..60 {
        tick_world_for_tests(&mut world);
    }

    assert_eq!(world.resource_amount_for_tests(iron), Some(0));
    assert_eq!(world.resource_amount_for_tests(copper), Some(2));
    assert_eq!(
        output_stack_for_tests(&world, miner),
        Some(CoreItemStack {
            kind: TEST_COPPER_ORE,
            amount: 1,
        })
    );
}

#[test]
fn same_factory_commands_produce_same_digest() {
    fn build_world() -> SimWorld {
        let mut world = SimWorld::with_catalog(catalog_for_tests());
        world
            .apply_core_command_for_tests(SimCommand::PlaceBuilding {
                def_id: "wooden_chest".to_string(),
                origin: TilePos::new(0, 0),
                direction: Direction::East,
                inserter_drop_direction: None,
            })
            .unwrap();
        world
    }

    let mut left = build_world();
    let mut right = build_world();
    for _ in 0..32 {
        tick_world_for_tests(&mut left);
        tick_world_for_tests(&mut right);
    }

    assert_eq!(left.digest(), right.digest());
}

#[test]
fn furnace_consumes_ore_and_coal_to_produce_plate() {
    let mut world = SimWorld::with_catalog(catalog_for_tests());
    world
        .apply_core_command_for_tests(SimCommand::PlaceBuilding {
            def_id: "stone_furnace".to_string(),
            origin: TilePos::new(0, 0),
            direction: Direction::East,
            inserter_drop_direction: None,
        })
        .unwrap();
    let furnace = world.building_snapshots()[0].id;
    select_iron_plate_recipe(&mut world, furnace);
    world
        .insert_into_inventory_for_tests(
            furnace,
            CoreInventoryRole::Input,
            CoreItemStack {
                kind: TEST_IRON_ORE,
                amount: 1,
            },
        )
        .unwrap();
    world
        .insert_into_inventory_for_tests(
            furnace,
            CoreInventoryRole::Fuel,
            CoreItemStack {
                kind: TEST_COAL,
                amount: 1,
            },
        )
        .unwrap();

    for _ in 0..121 {
        tick_world_for_tests(&mut world);
    }

    let snapshot = world.building_snapshots().remove(0);
    let output = snapshot
        .inventories
        .iter()
        .find(|inventory| inventory.role == CoreInventoryRole::Output)
        .unwrap();
    assert_eq!(
        output.slots[0],
        Some(CoreItemStack {
            kind: TEST_IRON_PLATE,
            amount: 1,
        })
    );
}

#[test]
fn furnace_with_full_output_reports_blocked_without_consuming_input_or_fuel() {
    let mut world = SimWorld::with_catalog(catalog_for_tests());
    world
        .apply_core_command_for_tests(SimCommand::PlaceBuilding {
            def_id: "stone_furnace".to_string(),
            origin: TilePos::new(0, 0),
            direction: Direction::East,
            inserter_drop_direction: None,
        })
        .unwrap();
    let furnace = world.building_snapshots()[0].id;
    select_iron_plate_recipe(&mut world, furnace);
    world
        .insert_into_inventory_for_tests(
            furnace,
            CoreInventoryRole::Output,
            CoreItemStack {
                kind: TEST_IRON_PLATE,
                amount: 100,
            },
        )
        .unwrap();
    world
        .insert_into_inventory_for_tests(
            furnace,
            CoreInventoryRole::Input,
            CoreItemStack {
                kind: TEST_IRON_ORE,
                amount: 1,
            },
        )
        .unwrap();
    world
        .insert_into_inventory_for_tests(
            furnace,
            CoreInventoryRole::Fuel,
            CoreItemStack {
                kind: TEST_COAL,
                amount: 1,
            },
        )
        .unwrap();

    let output = tick_world_for_tests(&mut world);

    let snapshot = world.building_snapshots().remove(0);
    let machine = machine_runtime_for_tests(&snapshot);
    assert_eq!(machine.status, MachineStatus::OutputBlocked);
    assert_eq!(machine.progress_ticks, 0);
    assert_eq!(output.metrics.active_behaviors, 0);
    let input = snapshot
        .inventories
        .iter()
        .find(|inventory| inventory.role == CoreInventoryRole::Input)
        .unwrap();
    let fuel = snapshot
        .inventories
        .iter()
        .find(|inventory| inventory.role == CoreInventoryRole::Fuel)
        .unwrap();
    let output = snapshot
        .inventories
        .iter()
        .find(|inventory| inventory.role == CoreInventoryRole::Output)
        .unwrap();
    assert_eq!(
        input.slots[0],
        Some(CoreItemStack {
            kind: TEST_IRON_ORE,
            amount: 1,
        })
    );
    assert_eq!(
        fuel.slots[0],
        Some(CoreItemStack {
            kind: TEST_COAL,
            amount: 1,
        })
    );
    assert_eq!(
        output.slots[0],
        Some(CoreItemStack {
            kind: TEST_IRON_PLATE,
            amount: 100,
        })
    );
}

#[test]
fn inserter_moves_one_item_from_chest_to_furnace_input() {
    let mut world = SimWorld::with_catalog(catalog_for_tests());
    world
        .apply_core_command_for_tests(SimCommand::PlaceBuilding {
            def_id: "wooden_chest".to_string(),
            origin: TilePos::new(0, 0),
            direction: Direction::East,
            inserter_drop_direction: None,
        })
        .unwrap();
    world
        .apply_core_command_for_tests(SimCommand::PlaceBuilding {
            def_id: "basic_inserter".to_string(),
            origin: TilePos::new(2, 0),
            direction: Direction::West,
            inserter_drop_direction: Some(Direction::East),
        })
        .unwrap();
    world
        .apply_core_command_for_tests(SimCommand::PlaceBuilding {
            def_id: "stone_furnace".to_string(),
            origin: TilePos::new(3, 0),
            direction: Direction::East,
            inserter_drop_direction: None,
        })
        .unwrap();
    let chest = world
        .building_snapshots()
        .into_iter()
        .find(|snapshot| {
            snapshot.kind == CoreBuildingKind::Passive && snapshot.def_id == "wooden_chest"
        })
        .unwrap()
        .id;
    let furnace = world
        .building_snapshots()
        .into_iter()
        .find(|snapshot| {
            snapshot.kind == CoreBuildingKind::Machine && snapshot.def_id == "stone_furnace"
        })
        .unwrap()
        .id;
    select_iron_plate_recipe(&mut world, furnace);
    world
        .insert_into_inventory_for_tests(
            chest,
            CoreInventoryRole::Storage,
            CoreItemStack {
                kind: TEST_IRON_ORE,
                amount: 1,
            },
        )
        .unwrap();

    for _ in 0..60 {
        tick_world_for_tests(&mut world);
    }

    let furnace = world
        .building_snapshots()
        .into_iter()
        .find(|snapshot| {
            snapshot.kind == CoreBuildingKind::Machine && snapshot.def_id == "stone_furnace"
        })
        .unwrap();
    let input = furnace
        .inventories
        .iter()
        .find(|inventory| inventory.role == CoreInventoryRole::Input)
        .unwrap();
    assert_eq!(
        input.slots[0],
        Some(CoreItemStack {
            kind: TEST_IRON_ORE,
            amount: 1,
        })
    );
}

#[test]
fn inserter_moves_one_item_from_miner_output_to_chest_storage() {
    let mut world = SimWorld::with_catalog(catalog_for_tests());
    world.seed_resource_for_tests(TilePos::new(2, 2), TEST_IRON_ORE, 4);
    world
        .apply_core_command_for_tests(SimCommand::PlaceBuilding {
            def_id: "basic_miner".to_string(),
            origin: TilePos::new(0, 0),
            direction: Direction::East,
            inserter_drop_direction: None,
        })
        .unwrap();
    world
        .apply_core_command_for_tests(SimCommand::PlaceBuilding {
            def_id: "basic_inserter".to_string(),
            origin: TilePos::new(5, 2),
            direction: Direction::West,
            inserter_drop_direction: Some(Direction::East),
        })
        .unwrap();
    world
        .apply_core_command_for_tests(SimCommand::PlaceBuilding {
            def_id: "wooden_chest".to_string(),
            origin: TilePos::new(6, 1),
            direction: Direction::East,
            inserter_drop_direction: None,
        })
        .unwrap();
    let miner = world
        .building_snapshots()
        .into_iter()
        .find(|snapshot| {
            snapshot.kind == CoreBuildingKind::Machine && snapshot.def_id == "basic_miner"
        })
        .unwrap()
        .id;
    world
        .insert_into_inventory_for_tests(
            miner,
            CoreInventoryRole::Fuel,
            CoreItemStack {
                kind: TEST_COAL,
                amount: 1,
            },
        )
        .unwrap();

    for _ in 0..120 {
        tick_world_for_tests(&mut world);
    }

    let chest = world
        .building_snapshots()
        .into_iter()
        .find(|snapshot| {
            snapshot.kind == CoreBuildingKind::Passive && snapshot.def_id == "wooden_chest"
        })
        .unwrap();
    let storage = chest
        .inventories
        .iter()
        .find(|inventory| inventory.role == CoreInventoryRole::Storage)
        .unwrap();
    assert_eq!(
        storage.slots[0],
        Some(CoreItemStack {
            kind: TEST_IRON_ORE,
            amount: 1,
        })
    );
}

#[test]
fn inserter_tick_pulls_resource_from_miner_output() {
    let mut world = SimWorld::with_catalog(catalog_for_tests());
    world
        .apply_core_command_for_tests(SimCommand::PlaceBuilding {
            def_id: "basic_miner".to_string(),
            origin: TilePos::new(0, 0),
            direction: Direction::East,
            inserter_drop_direction: None,
        })
        .unwrap();
    let miner = world.building_snapshots()[0].id;
    world
        .insert_into_inventory_for_tests(
            miner,
            CoreInventoryRole::Output,
            CoreItemStack {
                kind: TEST_COPPER_ORE,
                amount: 1,
            },
        )
        .unwrap();
    world
        .apply_core_command_for_tests(SimCommand::PlaceBuilding {
            def_id: "basic_inserter".to_string(),
            origin: TilePos::new(5, 2),
            direction: Direction::West,
            inserter_drop_direction: Some(Direction::East),
        })
        .unwrap();
    let inserter_id = world.building_snapshots()[1].id;

    tick_world_for_tests(&mut world);

    assert_eq!(output_stack_for_tests(&world, miner), None);
    let inserter_snapshot = world
        .building_snapshots()
        .into_iter()
        .find(|snapshot| snapshot.id == inserter_id)
        .unwrap();
    assert!(matches!(
        inserter_snapshot.state,
        SimBuildingState::Inserter(InserterRuntime {
            carried: Some(CoreItemStack {
                kind: TEST_COPPER_ORE,
                amount: 1
            }),
            ..
        })
    ));
}

#[test]
fn inserter_picks_up_miner_output_from_configured_footprint_port_matrix() {
    let cases = [
        (
            "east output",
            Direction::East,
            TilePos::new(5, 2),
            Direction::West,
        ),
        (
            "west output",
            Direction::West,
            TilePos::new(-1, 2),
            Direction::East,
        ),
        (
            "north output",
            Direction::North,
            TilePos::new(2, 5),
            Direction::South,
        ),
        (
            "south output",
            Direction::South,
            TilePos::new(2, -1),
            Direction::North,
        ),
        (
            "east top output",
            Direction::East,
            TilePos::new(5, 4),
            Direction::West,
        ),
        (
            "east bottom output",
            Direction::East,
            TilePos::new(5, 0),
            Direction::West,
        ),
        (
            "north west output",
            Direction::East,
            TilePos::new(0, 5),
            Direction::South,
        ),
        (
            "south east output",
            Direction::East,
            TilePos::new(4, -1),
            Direction::North,
        ),
    ];

    for (label, miner_direction, inserter_origin, pickup_direction) in cases {
        let mut world = SimWorld::with_catalog(catalog_for_tests());
        world
            .apply_core_command_for_tests(SimCommand::PlaceBuilding {
                def_id: "basic_miner".to_string(),
                origin: TilePos::new(0, 0),
                direction: miner_direction,
                inserter_drop_direction: None,
            })
            .unwrap();
        let miner = world.building_snapshots()[0].id;
        world
            .insert_into_inventory_for_tests(
                miner,
                CoreInventoryRole::Output,
                CoreItemStack {
                    kind: TEST_IRON_ORE,
                    amount: 1,
                },
            )
            .unwrap();
        world
            .apply_core_command_for_tests(SimCommand::PlaceBuilding {
                def_id: "basic_inserter".to_string(),
                origin: inserter_origin,
                direction: pickup_direction,
                inserter_drop_direction: Some(pickup_direction.opposite()),
            })
            .unwrap();
        let inserter_id = world.building_snapshots()[1].id;
        let inserter_building = world.buildings.get(&inserter_id).unwrap().clone();
        let SimBuildingState::Inserter(inserter) = inserter_building.state.clone() else {
            panic!("expected inserter");
        };

        assert_eq!(
            world.try_inserter_pickup(&inserter_building, &inserter, &mut SimDiff::default(),),
            Some(CoreItemStack {
                kind: TEST_IRON_ORE,
                amount: 1,
            }),
            "{label}",
        );
    }
}

#[test]
fn inserter_does_not_pick_up_miner_output_from_non_port_tile() {
    let mut world = SimWorld::with_catalog(catalog_for_tests());
    world
        .apply_core_command_for_tests(SimCommand::PlaceBuilding {
            def_id: "basic_miner".to_string(),
            origin: TilePos::new(0, 0),
            direction: Direction::East,
            inserter_drop_direction: None,
        })
        .unwrap();
    let miner = world.building_snapshots()[0].id;
    world
        .insert_into_inventory_for_tests(
            miner,
            CoreInventoryRole::Output,
            CoreItemStack {
                kind: TEST_IRON_ORE,
                amount: 1,
            },
        )
        .unwrap();
    world
        .apply_core_command_for_tests(SimCommand::PlaceBuilding {
            def_id: "basic_inserter".to_string(),
            origin: TilePos::new(6, 6),
            direction: Direction::West,
            inserter_drop_direction: Some(Direction::East),
        })
        .unwrap();
    let inserter_id = world.building_snapshots()[1].id;
    let inserter_building = world.buildings.get(&inserter_id).unwrap().clone();
    let SimBuildingState::Inserter(inserter) = inserter_building.state.clone() else {
        panic!("expected inserter");
    };

    assert_eq!(
        world.try_inserter_pickup(&inserter_building, &inserter, &mut SimDiff::default()),
        None
    );
}

#[test]
fn unknown_belt_item_pickup_does_not_remove_item() {
    let mut world = SimWorld::with_catalog(catalog_for_tests());
    world
        .apply_core_command_for_tests(SimCommand::PlaceBelt {
            pos: TilePos::new(0, 0),
            direction: Direction::East,
            input_direction: Direction::East,
            speed: UnitsPerTick::new(8),
        })
        .unwrap();
    world
        .apply_core_command_for_tests(SimCommand::PlaceBuilding {
            def_id: "basic_inserter".to_string(),
            origin: TilePos::new(1, 0),
            direction: Direction::West,
            inserter_drop_direction: Some(Direction::East),
        })
        .unwrap();
    world
        .apply_core_command_for_tests(SimCommand::InsertItemAtLineStart {
            line_index: 0,
            lane: 0,
            item: ItemKindId(999),
        })
        .unwrap();

    let output = tick_world_for_tests(&mut world);

    assert_eq!(output.metrics.inserter_pickups, 0);
    assert_eq!(output.metrics.simulated_items, 1);
    let inserter = world
        .building_snapshots()
        .into_iter()
        .find(|snapshot| snapshot.kind == CoreBuildingKind::Inserter)
        .unwrap();
    assert!(matches!(
        inserter.state,
        SimBuildingState::Inserter(InserterRuntime { carried: None, .. })
    ));
    assert_eq!(
        world
            .visible_items_for_bounds(VisibleTileBounds::new(
                TilePos::new(0, 0),
                TilePos::new(0, 0),
            ))
            .collect::<Vec<_>>()
            .len(),
        1
    );
}

#[test]
fn inserter_belt_mutation_metrics_match_final_state() {
    let mut world = SimWorld::with_catalog(catalog_for_tests());
    world
        .apply_core_command_for_tests(SimCommand::PlaceBuilding {
            def_id: "basic_belt".to_string(),
            origin: TilePos::new(0, 0),
            direction: Direction::East,
            inserter_drop_direction: None,
        })
        .unwrap();
    world
        .apply_core_command_for_tests(SimCommand::PlaceBuilding {
            def_id: "basic_inserter".to_string(),
            origin: TilePos::new(1, 0),
            direction: Direction::West,
            inserter_drop_direction: Some(Direction::East),
        })
        .unwrap();
    world
        .apply_core_command_for_tests(SimCommand::InsertItemAtLineStart {
            line_index: 0,
            lane: 0,
            item: TEST_IRON_ORE,
        })
        .unwrap();

    let output = tick_world_for_tests(&mut world);

    assert_eq!(output.metrics.inserter_pickups, 1);
    assert_eq!(output.metrics.simulated_items, 0);
    assert_eq!(world.last_metrics().simulated_items, 0);
    assert!(
        world
            .visible_items_for_bounds(VisibleTileBounds::new(
                TilePos::new(0, 0),
                TilePos::new(0, 0),
            ))
            .collect::<Vec<_>>()
            .is_empty()
    );
}

#[test]
fn inserter_can_pickup_from_adjacent_belt_tile() {
    let mut world = SimWorld::with_catalog(catalog_for_tests());
    world
        .apply_core_command_for_tests(SimCommand::PlaceBuilding {
            def_id: "basic_belt".to_string(),
            origin: TilePos::new(0, 0),
            direction: Direction::East,
            inserter_drop_direction: None,
        })
        .unwrap();
    world
        .apply_core_command_for_tests(SimCommand::InsertItemAtLineStart {
            line_index: 0,
            lane: 0,
            item: TEST_IRON_ORE,
        })
        .unwrap();
    world
        .apply_core_command_for_tests(SimCommand::PlaceBuilding {
            def_id: "basic_inserter".to_string(),
            origin: TilePos::new(1, 0),
            direction: Direction::West,
            inserter_drop_direction: Some(Direction::East),
        })
        .unwrap();
    let inserter_id = world.building_snapshots()[1].id;
    let inserter_building = world.buildings.get(&inserter_id).unwrap().clone();
    let SimBuildingState::Inserter(inserter) = inserter_building.state.clone() else {
        panic!("expected inserter");
    };

    assert_eq!(
        world.try_inserter_pickup(&inserter_building, &inserter, &mut SimDiff::default(),),
        Some(CoreItemStack {
            kind: TEST_IRON_ORE,
            amount: 1,
        })
    );
}

#[test]
fn inserter_can_pickup_from_adjacent_splitter_internal_item() {
    let mut world = connected_splitter_world_without_outputs_for_tests();
    world
        .apply_core_command_for_tests(SimCommand::DropItemOnBeltTile {
            pos: TilePos::new(-1, 0),
            lane: 0,
            distance_numerator: 128,
            distance_denominator: 128,
            item: TEST_IRON_ORE,
        })
        .unwrap();
    tick_world_for_tests(&mut world);
    assert_eq!(production_splitter_runtime(&world).ingress_items.len(), 1);

    world
        .apply_core_command_for_tests(SimCommand::PlaceBuilding {
            def_id: "basic_inserter".to_string(),
            origin: TilePos::new(0, -1),
            direction: Direction::North,
            inserter_drop_direction: Some(Direction::South),
        })
        .unwrap();
    let inserter_id = world
        .building_snapshots()
        .into_iter()
        .find(|snapshot| snapshot.def_id == "basic_inserter")
        .unwrap()
        .id;
    let inserter_building = world.buildings.get(&inserter_id).unwrap().clone();
    let SimBuildingState::Inserter(inserter) = inserter_building.state.clone() else {
        panic!("expected inserter");
    };

    assert_eq!(
        world.try_inserter_pickup(&inserter_building, &inserter, &mut SimDiff::default()),
        Some(CoreItemStack {
            kind: TEST_IRON_ORE,
            amount: 1,
        })
    );
    assert_eq!(
        production_splitter_runtime(&world).ingress_items,
        Vec::new()
    );
}

#[test]
fn inserter_can_pickup_from_visible_underground_endpoint_item() {
    let mut world = SimWorld::with_catalog(catalog_for_tests());
    world
        .apply_core_command_for_tests(SimCommand::PlaceUndergroundBelt {
            def_id: "basic_underground_belt".to_string(),
            entrance: TilePos::new(0, 0),
            exit: TilePos::new(3, 0),
            direction: Direction::East,
        })
        .unwrap();
    world.rebuild_transport_lines();
    push_visible_underground_entrance_item(&mut world, TEST_IRON_ORE, 0);
    assert_eq!(underground_runtime_items(&world).len(), 1);

    world
        .apply_core_command_for_tests(SimCommand::PlaceBuilding {
            def_id: "basic_inserter".to_string(),
            origin: TilePos::new(0, -1),
            direction: Direction::North,
            inserter_drop_direction: Some(Direction::South),
        })
        .unwrap();
    let inserter_id = world
        .building_snapshots()
        .into_iter()
        .find(|snapshot| snapshot.def_id == "basic_inserter")
        .unwrap()
        .id;
    let inserter_building = world.buildings.get(&inserter_id).unwrap().clone();
    let SimBuildingState::Inserter(inserter) = inserter_building.state.clone() else {
        panic!("expected inserter");
    };

    assert_eq!(
        world.try_inserter_pickup(&inserter_building, &inserter, &mut SimDiff::default()),
        Some(CoreItemStack {
            kind: TEST_IRON_ORE,
            amount: 1,
        })
    );
    assert_eq!(underground_runtime_items(&world), Vec::new());
}

#[test]
fn inserter_pickup_matches_player_order_for_underground_and_surface_item() {
    let mut world = SimWorld::with_catalog(catalog_for_tests());
    world
        .apply_core_command_for_tests(SimCommand::PlaceUndergroundBelt {
            def_id: "basic_underground_belt".to_string(),
            entrance: TilePos::new(0, 0),
            exit: TilePos::new(3, 0),
            direction: Direction::East,
        })
        .unwrap();
    world.rebuild_transport_lines();
    world
        .apply_core_command_for_tests(SimCommand::DropItemOnBeltTile {
            pos: TilePos::new(0, 0),
            lane: 0,
            distance_numerator: 64,
            distance_denominator: 128,
            item: TEST_COPPER_ORE,
        })
        .unwrap();

    let node_id = underground_runtime_node_id(&world);
    world
        .transport
        .underground_runtime_mut(node_id)
        .unwrap()
        .items
        .push(UndergroundTransportItem {
            item: TEST_IRON_ORE,
            lane: 1,
            progress: DistanceUnits::new(1),
        });

    world
        .apply_core_command_for_tests(SimCommand::PlaceBuilding {
            def_id: "basic_inserter".to_string(),
            origin: TilePos::new(0, -1),
            direction: Direction::North,
            inserter_drop_direction: Some(Direction::South),
        })
        .unwrap();
    let inserter_id = world
        .building_snapshots()
        .into_iter()
        .find(|snapshot| snapshot.def_id == "basic_inserter")
        .unwrap()
        .id;
    let inserter_building = world.buildings.get(&inserter_id).unwrap().clone();
    let SimBuildingState::Inserter(inserter) = inserter_building.state.clone() else {
        panic!("expected inserter");
    };
    let preview = world
        .first_item_stack_on_belt_tile(TilePos::new(0, 0))
        .unwrap();

    let picked = world
        .try_inserter_pickup(&inserter_building, &inserter, &mut SimDiff::default())
        .unwrap();

    assert_eq!(
        preview,
        CoreItemStack {
            kind: TEST_IRON_ORE,
            amount: 1,
        }
    );
    assert_eq!(picked, preview);
    assert_eq!(underground_runtime_items(&world), Vec::new());
    assert_eq!(
        world.first_item_stack_on_belt_tile(TilePos::new(0, 0)),
        Some(CoreItemStack {
            kind: TEST_COPPER_ORE,
            amount: 1,
        })
    );
}

#[test]
fn inserter_does_not_pickup_from_belt_with_one_tile_gap() {
    let mut world = SimWorld::with_catalog(catalog_for_tests());
    world
        .apply_core_command_for_tests(SimCommand::PlaceBuilding {
            def_id: "basic_belt".to_string(),
            origin: TilePos::new(0, 0),
            direction: Direction::East,
            inserter_drop_direction: None,
        })
        .unwrap();
    world
        .apply_core_command_for_tests(SimCommand::InsertItemAtLineStart {
            line_index: 0,
            lane: 0,
            item: TEST_IRON_ORE,
        })
        .unwrap();
    world
        .apply_core_command_for_tests(SimCommand::PlaceBuilding {
            def_id: "basic_inserter".to_string(),
            origin: TilePos::new(2, 0),
            direction: Direction::West,
            inserter_drop_direction: Some(Direction::East),
        })
        .unwrap();
    let inserter_id = world.building_snapshots()[1].id;
    let inserter_building = world.buildings.get(&inserter_id).unwrap().clone();
    let SimBuildingState::Inserter(inserter) = inserter_building.state.clone() else {
        panic!("expected inserter");
    };

    assert_eq!(
        world.try_inserter_pickup(&inserter_building, &inserter, &mut SimDiff::default(),),
        None
    );
}

#[test]
fn inserter_can_drop_to_adjacent_belt_tile() {
    let mut world = SimWorld::with_catalog(catalog_for_tests());
    world
        .apply_core_command_for_tests(SimCommand::PlaceBuilding {
            def_id: "basic_belt".to_string(),
            origin: TilePos::new(0, 0),
            direction: Direction::East,
            inserter_drop_direction: None,
        })
        .unwrap();
    world
        .apply_core_command_for_tests(SimCommand::PlaceBuilding {
            def_id: "basic_inserter".to_string(),
            origin: TilePos::new(1, 0),
            direction: Direction::East,
            inserter_drop_direction: Some(Direction::West),
        })
        .unwrap();
    let inserter_id = world.building_snapshots()[1].id;
    let inserter_building = world.buildings.get(&inserter_id).unwrap().clone();
    let SimBuildingState::Inserter(inserter) = inserter_building.state.clone() else {
        panic!("expected inserter");
    };

    assert!(world.try_inserter_drop(
        &inserter_building,
        &inserter,
        CoreItemStack {
            kind: TEST_IRON_ORE,
            amount: 1,
        },
        &mut SimDiff::default(),
    ));
    assert_eq!(
        world
            .visible_items_for_bounds(VisibleTileBounds::new(
                TilePos::new(0, 0),
                TilePos::new(0, 0),
            ))
            .collect::<Vec<_>>()
            .len(),
        1
    );
}

#[test]
fn inserter_drops_to_nearest_free_belt_slot_when_center_is_occupied() {
    let mut world = SimWorld::with_catalog(catalog_for_tests());
    world
        .apply_core_command_for_tests(SimCommand::PlaceBuilding {
            def_id: "basic_belt".to_string(),
            origin: TilePos::new(0, 0),
            direction: Direction::East,
            inserter_drop_direction: None,
        })
        .unwrap();
    world
        .apply_core_command_for_tests(SimCommand::DropItemOnBeltTile {
            pos: TilePos::new(0, 0),
            lane: 0,
            distance_numerator: 64,
            distance_denominator: 128,
            item: TEST_WOOD,
        })
        .unwrap();
    world
        .apply_core_command_for_tests(SimCommand::PlaceBuilding {
            def_id: "basic_inserter".to_string(),
            origin: TilePos::new(1, 0),
            direction: Direction::East,
            inserter_drop_direction: Some(Direction::West),
        })
        .unwrap();
    let inserter_id = world.building_snapshots()[1].id;
    let inserter_building = world.buildings.get(&inserter_id).unwrap().clone();
    let SimBuildingState::Inserter(inserter) = inserter_building.state.clone() else {
        panic!("expected inserter");
    };

    assert!(world.try_inserter_drop(
        &inserter_building,
        &inserter,
        CoreItemStack {
            kind: TEST_IRON_ORE,
            amount: 1,
        },
        &mut SimDiff::default(),
    ));

    let visible = world
        .visible_items_for_bounds(VisibleTileBounds::new(
            TilePos::new(0, 0),
            TilePos::new(0, 0),
        ))
        .collect::<Vec<_>>();
    assert_eq!(visible.len(), 2);
    assert!(visible.iter().any(|item| item.item == TEST_WOOD));
    assert!(visible.iter().any(|item| item.item == TEST_IRON_ORE));
}

#[test]
fn inserter_drop_prefers_belt_half_nearest_the_inserter_side() {
    let mut world = SimWorld::with_catalog(catalog_for_tests());
    world
        .apply_core_command_for_tests(SimCommand::PlaceBuilding {
            def_id: "basic_belt".to_string(),
            origin: TilePos::new(0, 0),
            direction: Direction::East,
            inserter_drop_direction: None,
        })
        .unwrap();
    world
        .apply_core_command_for_tests(SimCommand::PlaceBuilding {
            def_id: "basic_inserter".to_string(),
            origin: TilePos::new(1, 0),
            direction: Direction::East,
            inserter_drop_direction: Some(Direction::West),
        })
        .unwrap();
    let inserter_id = world.building_snapshots()[1].id;
    let inserter_building = world.buildings.get(&inserter_id).unwrap().clone();
    let SimBuildingState::Inserter(inserter) = inserter_building.state.clone() else {
        panic!("expected inserter");
    };

    assert!(world.try_inserter_drop(
        &inserter_building,
        &inserter,
        CoreItemStack {
            kind: TEST_IRON_ORE,
            amount: 1,
        },
        &mut SimDiff::default(),
    ));

    let visible = world
        .visible_items_for_bounds(VisibleTileBounds::new(
            TilePos::new(0, 0),
            TilePos::new(0, 0),
        ))
        .collect::<Vec<_>>();
    assert_eq!(visible.len(), 1);
    assert_eq!(visible[0].item, TEST_IRON_ORE);
    assert!(
        visible[0].progress_numerator >= 96,
        "east-side inserter should place near the east/front half: {visible:?}"
    );
}

#[test]
fn inserter_drop_prefers_belt_lane_nearest_the_inserter_side() {
    let mut world = SimWorld::with_catalog(catalog_for_tests());
    world
        .apply_core_command_for_tests(SimCommand::PlaceBuilding {
            def_id: "basic_belt".to_string(),
            origin: TilePos::new(0, 0),
            direction: Direction::East,
            inserter_drop_direction: None,
        })
        .unwrap();
    world
        .apply_core_command_for_tests(SimCommand::PlaceBuilding {
            def_id: "basic_inserter".to_string(),
            origin: TilePos::new(0, -1),
            direction: Direction::South,
            inserter_drop_direction: Some(Direction::North),
        })
        .unwrap();
    let inserter_id = world.building_snapshots()[1].id;
    let inserter_building = world.buildings.get(&inserter_id).unwrap().clone();
    let SimBuildingState::Inserter(inserter) = inserter_building.state.clone() else {
        panic!("expected inserter");
    };

    assert!(world.try_inserter_drop(
        &inserter_building,
        &inserter,
        CoreItemStack {
            kind: TEST_IRON_ORE,
            amount: 1,
        },
        &mut SimDiff::default(),
    ));

    let visible = world
        .visible_items_for_bounds(VisibleTileBounds::new(
            TilePos::new(0, 0),
            TilePos::new(0, 0),
        ))
        .collect::<Vec<_>>();
    assert_eq!(visible.len(), 1);
    assert_eq!(visible[0].item, TEST_IRON_ORE);
    assert_eq!(visible[0].lane, 1);
}

#[test]
fn inserter_pickup_wakes_belt_line_after_freeing_space() {
    let mut world = SimWorld::with_catalog(catalog_for_tests());
    world
        .apply_core_command_for_tests(SimCommand::PlaceBuilding {
            def_id: "basic_belt".to_string(),
            origin: TilePos::new(0, 0),
            direction: Direction::East,
            inserter_drop_direction: None,
        })
        .unwrap();
    world
        .apply_core_command_for_tests(SimCommand::InsertItemAtLineStart {
            line_index: 0,
            lane: 0,
            item: TEST_IRON_ORE,
        })
        .unwrap();
    world
        .apply_core_command_for_tests(SimCommand::PlaceBuilding {
            def_id: "basic_inserter".to_string(),
            origin: TilePos::new(1, 0),
            direction: Direction::West,
            inserter_drop_direction: Some(Direction::East),
        })
        .unwrap();
    let line_id = world.transport.line_ids_sorted().next().unwrap();
    world.activation.sleep_line(line_id);
    assert!(world.activation.active_lines().next().is_none());

    let inserter_id = world.building_snapshots()[1].id;
    let inserter_building = world.buildings.get(&inserter_id).unwrap().clone();
    let SimBuildingState::Inserter(inserter) = inserter_building.state.clone() else {
        panic!("expected inserter");
    };

    assert_eq!(
        world.try_inserter_pickup(&inserter_building, &inserter, &mut SimDiff::default()),
        Some(CoreItemStack {
            kind: TEST_IRON_ORE,
            amount: 1,
        })
    );
    assert_eq!(
        world.activation.active_lines().collect::<Vec<_>>(),
        vec![line_id]
    );
}

#[test]
fn inserter_with_one_tile_gap_drops_to_ground_not_belt() {
    let mut world = SimWorld::with_catalog(catalog_for_tests());
    world
        .apply_core_command_for_tests(SimCommand::PlaceBuilding {
            def_id: "basic_belt".to_string(),
            origin: TilePos::new(0, 0),
            direction: Direction::East,
            inserter_drop_direction: None,
        })
        .unwrap();
    world
        .apply_core_command_for_tests(SimCommand::PlaceBuilding {
            def_id: "basic_inserter".to_string(),
            origin: TilePos::new(2, 0),
            direction: Direction::East,
            inserter_drop_direction: Some(Direction::West),
        })
        .unwrap();
    let inserter_id = world.building_snapshots()[1].id;
    let inserter_building = world.buildings.get(&inserter_id).unwrap().clone();
    let SimBuildingState::Inserter(inserter) = inserter_building.state.clone() else {
        panic!("expected inserter");
    };
    assert!(world.try_inserter_drop(
        &inserter_building,
        &inserter,
        CoreItemStack {
            kind: TEST_IRON_ORE,
            amount: 1,
        },
        &mut SimDiff::default(),
    ));
    assert_eq!(
        world.surface_item_drops,
        vec![CoreSurfaceDrop {
            origin: TilePos::new(1, 0),
            stack: CoreItemStack {
                kind: TEST_IRON_ORE,
                amount: 1,
            },
            instance: None,
        }]
    );
    assert_eq!(
        world
            .visible_items_for_bounds(VisibleTileBounds::new(
                TilePos::new(0, 0),
                TilePos::new(0, 0),
            ))
            .collect::<Vec<_>>()
            .len(),
        0
    );
}

#[test]
fn inserter_moves_one_item_from_chest_to_adjacent_belt() {
    let mut world = SimWorld::with_catalog(catalog_for_tests());
    world
        .apply_core_command_for_tests(SimCommand::PlaceBuilding {
            def_id: "wooden_chest".to_string(),
            origin: TilePos::new(0, 0),
            direction: Direction::East,
            inserter_drop_direction: None,
        })
        .unwrap();
    world
        .apply_core_command_for_tests(SimCommand::PlaceBuilding {
            def_id: "basic_inserter".to_string(),
            origin: TilePos::new(2, 0),
            direction: Direction::West,
            inserter_drop_direction: Some(Direction::East),
        })
        .unwrap();
    world
        .apply_core_command_for_tests(SimCommand::PlaceBuilding {
            def_id: "basic_belt".to_string(),
            origin: TilePos::new(3, 0),
            direction: Direction::East,
            inserter_drop_direction: None,
        })
        .unwrap();
    let chest = world
        .building_snapshots()
        .into_iter()
        .find(|snapshot| {
            snapshot.kind == CoreBuildingKind::Passive && snapshot.def_id == "wooden_chest"
        })
        .unwrap()
        .id;
    world
        .insert_into_inventory_for_tests(
            chest,
            CoreInventoryRole::Storage,
            CoreItemStack {
                kind: TEST_IRON_ORE,
                amount: 1,
            },
        )
        .unwrap();

    for _ in 0..60 {
        tick_world_for_tests(&mut world);
    }

    let visible = world
        .visible_items_for_bounds(VisibleTileBounds::new(
            TilePos::new(3, 0),
            TilePos::new(3, 0),
        ))
        .collect::<Vec<_>>();
    assert_eq!(visible.len(), 1);
    assert_eq!(visible[0].item, TEST_IRON_ORE);
}

#[test]
fn inserter_moves_one_item_from_adjacent_belt_to_chest() {
    let mut world = SimWorld::with_catalog(catalog_for_tests());
    world
        .apply_core_command_for_tests(SimCommand::PlaceBuilding {
            def_id: "basic_belt".to_string(),
            origin: TilePos::new(0, 0),
            direction: Direction::East,
            inserter_drop_direction: None,
        })
        .unwrap();
    world
        .apply_core_command_for_tests(SimCommand::PlaceBuilding {
            def_id: "basic_inserter".to_string(),
            origin: TilePos::new(1, 0),
            direction: Direction::West,
            inserter_drop_direction: Some(Direction::East),
        })
        .unwrap();
    world
        .apply_core_command_for_tests(SimCommand::PlaceBuilding {
            def_id: "wooden_chest".to_string(),
            origin: TilePos::new(2, 0),
            direction: Direction::East,
            inserter_drop_direction: None,
        })
        .unwrap();
    world
        .apply_core_command_for_tests(SimCommand::InsertItemAtLineStart {
            line_index: 0,
            lane: 0,
            item: TEST_IRON_ORE,
        })
        .unwrap();

    for _ in 0..60 {
        tick_world_for_tests(&mut world);
    }

    let chest = world
        .building_snapshots()
        .into_iter()
        .find(|snapshot| {
            snapshot.kind == CoreBuildingKind::Passive && snapshot.def_id == "wooden_chest"
        })
        .unwrap();
    let storage = chest
        .inventories
        .iter()
        .find(|inventory| inventory.role == CoreInventoryRole::Storage)
        .unwrap();
    assert_eq!(
        storage.slots[0],
        Some(CoreItemStack {
            kind: TEST_IRON_ORE,
            amount: 1,
        })
    );
}

#[test]
fn inserter_with_one_tile_gap_before_chest_drops_to_ground_not_storage() {
    let mut world = SimWorld::with_catalog(catalog_for_tests());
    world
        .apply_core_command_for_tests(SimCommand::PlaceBuilding {
            def_id: "basic_inserter".to_string(),
            origin: TilePos::new(2, -2),
            direction: Direction::West,
            inserter_drop_direction: Some(Direction::East),
        })
        .unwrap();
    world
        .apply_core_command_for_tests(SimCommand::PlaceBuilding {
            def_id: "wooden_chest".to_string(),
            origin: TilePos::new(4, -3),
            direction: Direction::East,
            inserter_drop_direction: None,
        })
        .unwrap();
    let inserter_id = world.building_snapshots()[0].id;
    world.replace_inserter_state(
        inserter_id,
        InserterRuntime {
            pickup_direction: Direction::West,
            drop_direction: Direction::East,
            cooldown_remaining_ticks: 0,
            carried: Some(CoreItemStack {
                kind: TEST_WOOD,
                amount: 1,
            }),
        },
    );

    let output = tick_world_for_tests(&mut world);

    assert_eq!(output.metrics.inserter_drops, 1);
    assert_eq!(
        output.surface_drops,
        vec![CoreSurfaceDrop {
            origin: TilePos::new(3, -2),
            stack: CoreItemStack {
                kind: TEST_WOOD,
                amount: 1,
            },
            instance: None,
        }]
    );
    let chest = world
        .building_snapshots()
        .into_iter()
        .find(|snapshot| {
            snapshot.kind == CoreBuildingKind::Passive && snapshot.def_id == "wooden_chest"
        })
        .unwrap();
    let storage = chest
        .inventories
        .iter()
        .find(|inventory| inventory.role == CoreInventoryRole::Storage)
        .unwrap();
    assert!(storage.slots.iter().all(Option::is_none));
}

#[test]
fn inserter_storage_footprint_drop_respects_vector_matrix() {
    let cases = [
        (
            "west edge",
            TilePos::new(0, 0),
            Direction::East,
            TilePos::new(1, 0),
        ),
        (
            "east edge",
            TilePos::new(2, 0),
            Direction::West,
            TilePos::new(0, 0),
        ),
        (
            "south edge",
            TilePos::new(0, 0),
            Direction::North,
            TilePos::new(0, 1),
        ),
        (
            "north edge",
            TilePos::new(0, 0),
            Direction::South,
            TilePos::new(0, -2),
        ),
    ];

    for (label, inserter_origin, drop_direction, chest_origin) in cases {
        let mut world = SimWorld::with_catalog(catalog_for_tests());
        world
            .apply_core_command_for_tests(SimCommand::PlaceBuilding {
                def_id: "basic_inserter".to_string(),
                origin: inserter_origin,
                direction: drop_direction.opposite(),
                inserter_drop_direction: Some(drop_direction),
            })
            .unwrap();
        world
            .apply_core_command_for_tests(SimCommand::PlaceBuilding {
                def_id: "wooden_chest".to_string(),
                origin: chest_origin,
                direction: Direction::East,
                inserter_drop_direction: None,
            })
            .unwrap();
        let inserter_id = world.building_snapshots()[0].id;
        world.replace_inserter_state(
            inserter_id,
            InserterRuntime {
                pickup_direction: drop_direction.opposite(),
                drop_direction,
                cooldown_remaining_ticks: 0,
                carried: Some(CoreItemStack {
                    kind: TEST_WOOD,
                    amount: 1,
                }),
            },
        );

        let output = tick_world_for_tests(&mut world);

        assert_eq!(output.metrics.inserter_drops, 1, "{label}");
        assert!(output.surface_drops.is_empty(), "{label}");
        let chest = world
            .building_snapshots()
            .into_iter()
            .find(|snapshot| {
                snapshot.kind == CoreBuildingKind::Passive && snapshot.def_id == "wooden_chest"
            })
            .unwrap();
        let storage = chest
            .inventories
            .iter()
            .find(|inventory| inventory.role == CoreInventoryRole::Storage)
            .unwrap();
        assert_eq!(
            storage.slots[0],
            Some(CoreItemStack {
                kind: TEST_WOOD,
                amount: 1,
            }),
            "{label}",
        );
    }
}

#[test]
fn inserter_does_not_use_storage_external_port_matrix() {
    let cases = [
        (
            "east vector into south port gap",
            TilePos::new(0, 0),
            Direction::East,
            TilePos::new(1, 1),
            TilePos::new(1, 0),
        ),
        (
            "west vector into south port gap",
            TilePos::new(0, 0),
            Direction::West,
            TilePos::new(-2, 1),
            TilePos::new(-1, 0),
        ),
        (
            "north vector into west port gap",
            TilePos::new(0, 0),
            Direction::North,
            TilePos::new(1, 1),
            TilePos::new(0, 1),
        ),
        (
            "south vector into west port gap",
            TilePos::new(0, 0),
            Direction::South,
            TilePos::new(1, -2),
            TilePos::new(0, -1),
        ),
    ];

    for (label, inserter_origin, drop_direction, chest_origin, ground_drop) in cases {
        let mut world = SimWorld::with_catalog(catalog_for_tests());
        world
            .apply_core_command_for_tests(SimCommand::PlaceBuilding {
                def_id: "basic_inserter".to_string(),
                origin: inserter_origin,
                direction: drop_direction.opposite(),
                inserter_drop_direction: Some(drop_direction),
            })
            .unwrap();
        world
            .apply_core_command_for_tests(SimCommand::PlaceBuilding {
                def_id: "wooden_chest".to_string(),
                origin: chest_origin,
                direction: Direction::East,
                inserter_drop_direction: None,
            })
            .unwrap();
        let inserter_id = world.building_snapshots()[0].id;
        world.replace_inserter_state(
            inserter_id,
            InserterRuntime {
                pickup_direction: drop_direction.opposite(),
                drop_direction,
                cooldown_remaining_ticks: 0,
                carried: Some(CoreItemStack {
                    kind: TEST_WOOD,
                    amount: 1,
                }),
            },
        );

        let output = tick_world_for_tests(&mut world);

        assert_eq!(output.metrics.inserter_drops, 1, "{label}");
        assert_eq!(
            output.surface_drops,
            vec![CoreSurfaceDrop {
                origin: ground_drop,
                stack: CoreItemStack {
                    kind: TEST_WOOD,
                    amount: 1,
                },
                instance: None,
            }],
            "{label}",
        );
        let chest = world
            .building_snapshots()
            .into_iter()
            .find(|snapshot| {
                snapshot.kind == CoreBuildingKind::Passive && snapshot.def_id == "wooden_chest"
            })
            .unwrap();
        let storage = chest
            .inventories
            .iter()
            .find(|inventory| inventory.role == CoreInventoryRole::Storage)
            .unwrap();
        assert!(storage.slots.iter().all(Option::is_none), "{label}");
    }
}

#[test]
fn inserter_moves_carried_item_into_occupied_chest_footprint_tile() {
    let mut world = SimWorld::with_catalog(catalog_for_tests());
    world
        .apply_core_command_for_tests(SimCommand::PlaceBuilding {
            def_id: "basic_inserter".to_string(),
            origin: TilePos::new(0, 1),
            direction: Direction::West,
            inserter_drop_direction: Some(Direction::South),
        })
        .unwrap();
    world
        .apply_core_command_for_tests(SimCommand::PlaceBuilding {
            def_id: "wooden_chest".to_string(),
            origin: TilePos::new(0, -1),
            direction: Direction::East,
            inserter_drop_direction: None,
        })
        .unwrap();
    let inserter_id = world.building_snapshots()[0].id;
    world.replace_inserter_state(
        inserter_id,
        InserterRuntime {
            pickup_direction: Direction::West,
            drop_direction: Direction::South,
            cooldown_remaining_ticks: 0,
            carried: Some(CoreItemStack {
                kind: TEST_WOOD,
                amount: 1,
            }),
        },
    );

    let output = tick_world_for_tests(&mut world);

    assert_eq!(output.metrics.inserter_drops, 1);
    assert!(output.surface_drops.is_empty());
    let chest = world
        .building_snapshots()
        .into_iter()
        .find(|snapshot| {
            snapshot.kind == CoreBuildingKind::Passive && snapshot.def_id == "wooden_chest"
        })
        .unwrap();
    let storage = chest
        .inventories
        .iter()
        .find(|inventory| inventory.role == CoreInventoryRole::Storage)
        .unwrap();
    assert_eq!(
        storage.slots[0],
        Some(CoreItemStack {
            kind: TEST_WOOD,
            amount: 1,
        })
    );
}

#[test]
fn inserter_picks_up_storage_item_from_occupied_chest_footprint_tile() {
    let mut world = SimWorld::with_catalog(catalog_for_tests());
    world
        .apply_core_command_for_tests(SimCommand::PlaceBuilding {
            def_id: "wooden_chest".to_string(),
            origin: TilePos::new(0, -1),
            direction: Direction::East,
            inserter_drop_direction: None,
        })
        .unwrap();
    world
        .apply_core_command_for_tests(SimCommand::PlaceBuilding {
            def_id: "basic_inserter".to_string(),
            origin: TilePos::new(0, 1),
            direction: Direction::South,
            inserter_drop_direction: Some(Direction::East),
        })
        .unwrap();
    let chest_id = world
        .building_snapshots()
        .into_iter()
        .find(|snapshot| {
            snapshot.kind == CoreBuildingKind::Passive && snapshot.def_id == "wooden_chest"
        })
        .unwrap()
        .id;
    world
        .insert_into_inventory_for_tests(
            chest_id,
            CoreInventoryRole::Storage,
            CoreItemStack {
                kind: TEST_WOOD,
                amount: 1,
            },
        )
        .unwrap();
    let inserter_id = world
        .building_snapshots()
        .into_iter()
        .find(|snapshot| snapshot.kind == CoreBuildingKind::Inserter)
        .unwrap()
        .id;
    let inserter_building = world.buildings.get(&inserter_id).unwrap().clone();
    let SimBuildingState::Inserter(inserter) = inserter_building.state.clone() else {
        panic!("expected inserter");
    };

    assert_eq!(
        world.try_inserter_pickup(&inserter_building, &inserter, &mut SimDiff::default()),
        Some(CoreItemStack {
            kind: TEST_WOOD,
            amount: 1,
        })
    );
}

#[test]
fn inserter_drops_carried_item_to_empty_ground_tile() {
    let mut world = SimWorld::with_catalog(catalog_for_tests());
    world
        .apply_core_command_for_tests(SimCommand::PlaceBuilding {
            def_id: "basic_inserter".to_string(),
            origin: TilePos::new(0, 0),
            direction: Direction::West,
            inserter_drop_direction: Some(Direction::East),
        })
        .unwrap();
    let inserter_id = world.building_snapshots()[0].id;
    world.replace_inserter_state(
        inserter_id,
        InserterRuntime {
            pickup_direction: Direction::West,
            drop_direction: Direction::East,
            cooldown_remaining_ticks: 0,
            carried: Some(CoreItemStack {
                kind: TEST_WOOD,
                amount: 1,
            }),
        },
    );

    let output = tick_world_for_tests(&mut world);

    assert_eq!(output.metrics.inserter_drops, 1);
    assert_eq!(
        output.surface_drops,
        vec![CoreSurfaceDrop {
            origin: TilePos::new(1, 0),
            stack: CoreItemStack {
                kind: TEST_WOOD,
                amount: 1,
            },
            instance: None,
        }]
    );
    let inserter = world
        .building_snapshots()
        .into_iter()
        .find(|snapshot| snapshot.kind == CoreBuildingKind::Inserter)
        .unwrap();
    assert!(matches!(
        inserter.state,
        SimBuildingState::Inserter(InserterRuntime { carried: None, .. })
    ));
}

#[test]
fn inserter_does_not_drop_to_ground_when_drop_tile_is_occupied() {
    let mut world = SimWorld::with_catalog(catalog_for_tests());
    world
        .apply_core_command_for_tests(SimCommand::PlaceBuilding {
            def_id: "basic_inserter".to_string(),
            origin: TilePos::new(0, 0),
            direction: Direction::West,
            inserter_drop_direction: Some(Direction::East),
        })
        .unwrap();
    world
        .apply_core_command_for_tests(SimCommand::PlaceBuilding {
            def_id: "stone_furnace".to_string(),
            origin: TilePos::new(1, 0),
            direction: Direction::East,
            inserter_drop_direction: None,
        })
        .unwrap();
    let inserter_id = world.building_snapshots()[0].id;
    world.replace_inserter_state(
        inserter_id,
        InserterRuntime {
            pickup_direction: Direction::West,
            drop_direction: Direction::East,
            cooldown_remaining_ticks: 0,
            carried: Some(CoreItemStack {
                kind: TEST_WOOD,
                amount: 1,
            }),
        },
    );

    let output = tick_world_for_tests(&mut world);

    assert_eq!(output.metrics.inserter_drops, 0);
    assert!(output.surface_drops.is_empty());
}

#[test]
fn same_commands_produce_same_digest() {
    let commands = vec![
        SimCommand::PlaceBelt {
            pos: TilePos::new(0, 0),
            direction: Direction::East,
            input_direction: Direction::East,
            speed: UnitsPerTick::new(8),
        },
        SimCommand::PlaceBelt {
            pos: TilePos::new(1, 0),
            direction: Direction::East,
            input_direction: Direction::East,
            speed: UnitsPerTick::new(8),
        },
        SimCommand::CreateSource {
            pos: TilePos::new(0, 0),
            item: ItemKindId(1),
            interval_ticks: 1,
        },
        SimCommand::CreateSink {
            pos: TilePos::new(1, 0),
        },
    ];

    let mut left = SimWorld::default();
    let mut right = SimWorld::default();
    for command in commands.clone() {
        left.apply_core_command_for_tests(command).unwrap();
    }
    for command in commands {
        right.apply_core_command_for_tests(command).unwrap();
    }
    for _ in 0..8 {
        tick_world_for_tests(&mut left);
        tick_world_for_tests(&mut right);
    }

    assert_eq!(left.digest(), right.digest());
}

#[test]
fn tick_reports_changed_lines_and_metrics() {
    let mut world = SimWorld::default();
    world
        .apply_core_command_for_tests(SimCommand::PlaceBelt {
            pos: TilePos::new(0, 0),
            direction: Direction::East,
            input_direction: Direction::East,
            speed: UnitsPerTick::new(8),
        })
        .unwrap();
    world
        .apply_core_command_for_tests(SimCommand::InsertItemAtLineStart {
            line_index: 0,
            lane: 0,
            item: ItemKindId(1),
        })
        .unwrap();

    let output = tick_world_for_tests(&mut world);

    assert_eq!(output.tick.raw(), 1);
    assert_eq!(output.metrics.simulated_items, 1);
    assert_eq!(output.metrics.active_lines, 1);
    assert!(!output.diff.changed_lines.is_empty());
}

#[test]
fn dirty_chunks_deduplicates_multiple_tiles_in_same_chunk() {
    let mut world = SimWorld::default();
    world
        .apply_core_command_for_tests(SimCommand::PlaceBelt {
            pos: TilePos::new(0, 0),
            direction: Direction::East,
            input_direction: Direction::East,
            speed: UnitsPerTick::new(8),
        })
        .unwrap();
    world
        .apply_core_command_for_tests(SimCommand::PlaceBelt {
            pos: TilePos::new(1, 0),
            direction: Direction::East,
            input_direction: Direction::East,
            speed: UnitsPerTick::new(8),
        })
        .unwrap();

    let output = tick_world_for_tests(&mut world);

    assert_eq!(output.metrics.dirty_chunks, 1);
}

#[test]
fn t_junction_world_keeps_vertical_target_as_one_line() {
    let mut world = SimWorld::default();
    for (pos, direction) in [
        (TilePos::new(0, -1), Direction::North),
        (TilePos::new(0, 0), Direction::North),
        (TilePos::new(0, 1), Direction::North),
        (TilePos::new(-1, 0), Direction::East),
        (TilePos::new(1, 0), Direction::West),
    ] {
        world
            .apply_core_command_for_tests(SimCommand::PlaceBelt {
                pos,
                direction,
                input_direction: direction,
                speed: UnitsPerTick::new(8),
            })
            .unwrap();
    }

    let line_tiles = world
        .transport
        .line_ids_sorted()
        .filter_map(|line_id| world.transport.line(line_id))
        .map(|line| {
            line.path()
                .tiles()
                .iter()
                .map(|tile| tile.pos)
                .collect::<Vec<_>>()
        })
        .collect::<Vec<_>>();

    assert!(line_tiles.contains(&vec![
        TilePos::new(0, -1),
        TilePos::new(0, 0),
        TilePos::new(0, 1),
    ]));
}

#[test]
fn opposing_belts_create_two_blocked_interactions() {
    let mut world = SimWorld::default();
    for (pos, direction) in [
        (TilePos::new(0, 0), Direction::East),
        (TilePos::new(1, 0), Direction::West),
    ] {
        world
            .apply_core_command_for_tests(SimCommand::PlaceBelt {
                pos,
                direction,
                input_direction: direction,
                speed: UnitsPerTick::new(8),
            })
            .unwrap();
    }

    let blocked = world
        .transport
        .interactions_sorted()
        .filter(|interaction| {
            interaction.kind() == crate::transport::interaction::BeltInteractionKind::BlockedFront
        })
        .count();

    assert_eq!(blocked, 2);
}

#[test]
fn items_on_opposing_belts_stop_at_shared_front() {
    let mut world = SimWorld::default();
    for (pos, direction) in [
        (TilePos::new(0, 0), Direction::East),
        (TilePos::new(1, 0), Direction::West),
    ] {
        world
            .apply_core_command_for_tests(SimCommand::PlaceBelt {
                pos,
                direction,
                input_direction: direction,
                speed: UnitsPerTick::new(8),
            })
            .unwrap();
    }
    world
        .apply_core_command_for_tests(SimCommand::DropItemOnBeltTile {
            pos: TilePos::new(0, 0),
            lane: 0,
            distance_numerator: 64,
            distance_denominator: 128,
            item: ItemKindId(3),
        })
        .unwrap();
    world
        .apply_core_command_for_tests(SimCommand::DropItemOnBeltTile {
            pos: TilePos::new(1, 0),
            lane: 0,
            distance_numerator: 64,
            distance_denominator: 128,
            item: ItemKindId(4),
        })
        .unwrap();

    for _ in 0..80 {
        tick_world_for_tests(&mut world);
    }

    let visible = world
        .visible_items_for_bounds(VisibleTileBounds::new(
            TilePos::new(0, 0),
            TilePos::new(1, 0),
        ))
        .collect::<Vec<_>>();

    assert_eq!(visible.len(), 2);
    assert!(
        visible
            .iter()
            .any(|item| item.tile == TilePos::new(0, 0) && item.item == ItemKindId(3))
    );
    assert!(
        visible
            .iter()
            .any(|item| item.tile == TilePos::new(1, 0) && item.item == ItemKindId(4))
    );
    let east_item = visible
        .iter()
        .find(|item| item.tile == TilePos::new(0, 0) && item.item == ItemKindId(3))
        .unwrap();
    let west_item = visible
        .iter()
        .find(|item| item.tile == TilePos::new(1, 0) && item.item == ItemKindId(4))
        .unwrap();
    assert_eq!(east_item.direction, Direction::East);
    assert_eq!(west_item.direction, Direction::West);
    assert_eq!(east_item.progress_denominator, 128);
    assert_eq!(west_item.progress_denominator, 128);
    assert_eq!(east_item.progress_numerator, 128);
    assert_eq!(west_item.progress_numerator, 128);
}

#[test]
fn blocked_sideload_source_compresses_and_sleeps() {
    let mut world = SimWorld::default();
    for (pos, direction) in [
        (TilePos::new(0, -1), Direction::North),
        (TilePos::new(0, 0), Direction::North),
        (TilePos::new(-1, 0), Direction::East),
    ] {
        world
            .apply_core_command_for_tests(SimCommand::PlaceBelt {
                pos,
                direction,
                input_direction: direction,
                speed: UnitsPerTick::new(8),
            })
            .unwrap();
    }
    for _ in 0..8 {
        world
            .apply_core_command_for_tests(SimCommand::DropItemOnBeltTile {
                pos: TilePos::new(0, 0),
                lane: 0,
                distance_numerator: 64,
                distance_denominator: 128,
                item: ItemKindId(4),
            })
            .unwrap();
    }
    for _ in 0..4 {
        world
            .apply_core_command_for_tests(SimCommand::DropItemOnBeltTile {
                pos: TilePos::new(-1, 0),
                lane: 0,
                distance_numerator: 64,
                distance_denominator: 128,
                item: ItemKindId(3),
            })
            .unwrap();
    }

    for _ in 0..120 {
        tick_world_for_tests(&mut world);
    }

    let source_line = world
        .line_window_for_tile(TilePos::new(-1, 0))
        .map(|(line_id, _, _, _)| line_id)
        .unwrap();
    assert!(!world.active_line_ids_for_tests().contains(&source_line));
    let source_items = world
        .visible_items_for_bounds(VisibleTileBounds::new(
            TilePos::new(-1, 0),
            TilePos::new(-1, 0),
        ))
        .count();
    assert!(source_items > 0);
}

#[test]
fn side_input_to_target_first_tile_without_straight_predecessor_builds_sideload() {
    let mut world = SimWorld::default();
    for (pos, direction) in [
        (TilePos::new(-1, 0), Direction::East),
        (TilePos::new(0, 0), Direction::North),
        (TilePos::new(0, 1), Direction::North),
    ] {
        world
            .apply_core_command_for_tests(SimCommand::PlaceBelt {
                pos,
                direction,
                input_direction: direction,
                speed: UnitsPerTick::new(8),
            })
            .unwrap();
    }

    let line_paths = world
        .transport
        .line_ids_sorted()
        .filter_map(|line_id| {
            world.transport.line(line_id).map(|line| {
                (
                    line_id,
                    line.path()
                        .tiles()
                        .iter()
                        .map(|tile| tile.pos)
                        .collect::<Vec<_>>(),
                )
            })
        })
        .collect::<Vec<_>>();
    let source_line = line_paths
        .iter()
        .find_map(|(line_id, tiles)| (*tiles == vec![TilePos::new(-1, 0)]).then_some(*line_id))
        .expect("side source line should be single tile");
    let target_line = line_paths
        .iter()
        .find_map(|(line_id, tiles)| {
            (*tiles == vec![TilePos::new(0, 0), TilePos::new(0, 1)]).then_some(*line_id)
        })
        .expect("target line should start at target first tile");

    assert_eq!(line_paths.len(), 2, "{line_paths:?}");

    let interactions = world.transport.interactions_sorted().collect::<Vec<_>>();
    assert!(interactions.iter().any(|interaction| {
        interaction.source_line() == source_line
            && interaction.target_line() == Some(target_line)
            && interaction.target_tile() == Some(TilePos::new(0, 0))
            && interaction.kind() == BeltInteractionKind::SideLoad { near_lane: 0 }
    }));
    assert!(!interactions.iter().any(|interaction| {
        interaction.source_line() == source_line
            && interaction.kind() == BeltInteractionKind::EndTransfer
    }));
}

#[test]
fn multi_tile_side_input_to_target_first_tile_builds_sideload() {
    let mut world = SimWorld::default();
    for (pos, direction) in [
        (TilePos::new(-2, 0), Direction::East),
        (TilePos::new(-1, 0), Direction::East),
        (TilePos::new(0, 0), Direction::North),
        (TilePos::new(0, 1), Direction::North),
    ] {
        world
            .apply_core_command_for_tests(SimCommand::PlaceBelt {
                pos,
                direction,
                input_direction: direction,
                speed: UnitsPerTick::new(8),
            })
            .unwrap();
    }

    let interactions = world.transport.interactions_sorted().collect::<Vec<_>>();

    assert!(interactions.iter().any(|interaction| {
        interaction.target_tile() == Some(TilePos::new(0, 0))
            && interaction.kind() == BeltInteractionKind::SideLoad { near_lane: 0 }
    }));
    assert!(!interactions.iter().any(|interaction| {
        interaction.target_tile() == Some(TilePos::new(0, 0))
            && interaction.kind() == BeltInteractionKind::EndTransfer
    }));
}

#[test]
fn side_input_to_closed_loop_first_tile_builds_sideload_not_end_transfer() {
    let mut world = SimWorld::default();
    for (pos, belt) in rectangular_cycle_for_tests() {
        world.topology_graph.set_belt(pos, belt);
    }
    let loop_first_tile = TilePos::new(0, -2);
    let target_belt = world.topology_graph.belt(loop_first_tile).unwrap();
    let source_side = target_belt.direction.left();
    let source_pos = source_side.output_pos(loop_first_tile);
    world
        .topology_graph
        .set_belt(source_pos, BeltTile::new(source_side.opposite()));
    let source_line = LineId(1);
    let target_line = LineId(2);
    let records = [
        BuiltLineRecord {
            line_id: source_line,
            tiles: vec![crate::topology::builder::BuiltPathTile::surface(source_pos)],
            closed: false,
            front_output: Some(loop_first_tile),
        },
        BuiltLineRecord {
            line_id: target_line,
            tiles: [
                TilePos::new(0, -2),
                TilePos::new(0, -1),
                TilePos::new(0, 0),
                TilePos::new(1, 0),
                TilePos::new(2, 0),
                TilePos::new(2, -1),
                TilePos::new(2, -2),
                TilePos::new(1, -2),
            ]
            .into_iter()
            .map(crate::topology::builder::BuiltPathTile::surface)
            .collect(),
            closed: true,
            front_output: None,
        },
    ];
    let mut transport = TransportStorage::default();
    world.insert_belt_interactions(&mut transport, &records, &BTreeSet::new());

    let interactions = transport.interactions_sorted().collect::<Vec<_>>();
    assert!(interactions.iter().any(|interaction| {
        interaction.source_line() == source_line
            && interaction.target_line() == Some(target_line)
            && interaction.target_tile() == Some(loop_first_tile)
            && interaction.kind() == BeltInteractionKind::SideLoad { near_lane: 0 }
    }));
    assert!(!interactions.iter().any(|interaction| {
        interaction.source_line() == source_line
            && interaction.target_line() == Some(target_line)
            && interaction.kind() == BeltInteractionKind::EndTransfer
    }));
}

#[test]
fn side_input_to_closed_loop_straight_segment_drains_both_source_lanes() {
    let mut world = SimWorld::with_catalog(catalog_for_tests());
    for (origin, direction) in [
        (TilePos::new(0, 1), Direction::East),
        (TilePos::new(1, 1), Direction::East),
        (TilePos::new(2, 1), Direction::South),
        (TilePos::new(2, 0), Direction::South),
        (TilePos::new(2, -1), Direction::West),
        (TilePos::new(1, -1), Direction::West),
        (TilePos::new(0, -1), Direction::North),
        (TilePos::new(0, 0), Direction::North),
        (TilePos::new(1, 0), Direction::North),
    ] {
        world
            .apply_core_command_for_tests(SimCommand::PlaceBuilding {
                def_id: "basic_belt".to_string(),
                origin,
                direction,
                inserter_drop_direction: None,
            })
            .unwrap();
    }

    let target = world.topology_graph.belt(TilePos::new(1, 1)).unwrap();
    assert_eq!(target.direction, Direction::East);
    assert_eq!(
        target.input_direction,
        Direction::East,
        "closed-loop target segment should stay straight instead of becoming a turn"
    );

    let source_line = world
        .line_window_for_tile(TilePos::new(1, 0))
        .map(|(line_id, _, _, _)| line_id)
        .unwrap();
    let target_line = world
        .line_window_for_tile(TilePos::new(1, 1))
        .map(|(line_id, _, _, _)| line_id)
        .unwrap();
    assert_ne!(source_line, target_line);
    assert!(world.transport.interactions_sorted().any(|interaction| {
        interaction.source_line() == source_line
            && interaction.target_line() == Some(target_line)
            && interaction.target_tile() == Some(TilePos::new(1, 1))
            && interaction.kind() == BeltInteractionKind::SideLoad { near_lane: 1 }
    }));

    for (lane, item) in [(0, ItemKindId(3)), (1, ItemKindId(4))] {
        world
            .apply_core_command_for_tests(SimCommand::DropItemOnBeltTile {
                pos: TilePos::new(1, 0),
                lane,
                distance_numerator: 128,
                distance_denominator: 128,
                item,
            })
            .unwrap();
    }

    for _ in 0..120 {
        tick_world_for_tests(&mut world);
    }

    let source_items = world
        .visible_items_for_bounds(VisibleTileBounds::new(
            TilePos::new(1, 0),
            TilePos::new(1, 0),
        ))
        .collect::<Vec<_>>();
    assert!(
        source_items.is_empty(),
        "side input should drain into the closed loop: {source_items:?}"
    );

    let loop_items = world
        .visible_items_for_bounds(VisibleTileBounds::new(
            TilePos::new(0, -1),
            TilePos::new(2, 1),
        ))
        .collect::<Vec<_>>();
    assert!(
        loop_items
            .iter()
            .any(|item| item.item == ItemKindId(3) && item.tile != TilePos::new(1, 0)),
        "source lane 0 item should be on the closed loop: {loop_items:?}"
    );
    assert!(
        loop_items
            .iter()
            .any(|item| item.item == ItemKindId(4) && item.tile != TilePos::new(1, 0)),
        "source lane 1 item should be on the closed loop: {loop_items:?}"
    );
}

#[test]
fn loop_shaped_side_input_transfers_to_central_straight_segment() {
    let mut world = SimWorld::with_catalog(catalog_for_tests());
    for (origin, direction) in [
        (TilePos::new(-1, 0), Direction::East),
        (TilePos::new(0, 0), Direction::East),
        (TilePos::new(1, 0), Direction::East),
        (TilePos::new(2, 0), Direction::South),
        (TilePos::new(2, -1), Direction::South),
        (TilePos::new(2, -2), Direction::West),
        (TilePos::new(1, -2), Direction::West),
        (TilePos::new(0, -2), Direction::North),
        (TilePos::new(0, -1), Direction::North),
        (TilePos::new(0, 1), Direction::South),
    ] {
        world
            .apply_core_command_for_tests(SimCommand::PlaceBuilding {
                def_id: "basic_belt".to_string(),
                origin,
                direction,
                inserter_drop_direction: None,
            })
            .unwrap();
    }

    let target = world.topology_graph.belt(TilePos::new(0, 0)).unwrap();
    assert_eq!(target.direction, Direction::East);
    assert_eq!(target.input_direction, Direction::East);

    world
        .apply_core_command_for_tests(SimCommand::DropItemOnBeltTile {
            pos: TilePos::new(0, -1),
            lane: 1,
            distance_numerator: 128,
            distance_denominator: 128,
            item: ItemKindId(3),
        })
        .unwrap();

    let mut visited_central = false;
    for _ in 0..120 {
        tick_world_for_tests(&mut world);
        visited_central |= world
            .visible_items_for_bounds(VisibleTileBounds::new(
                TilePos::new(0, 0),
                TilePos::new(2, 0),
            ))
            .any(|item| item.item == ItemKindId(3) && item.direction == Direction::East);
    }

    assert!(
        visited_central,
        "item should visit the central east segment after side-loading from the loop"
    );
}

#[test]
fn screenshot_compact_loop_t_junction_side_loads_both_inputs_to_front_belt_edges() {
    let mut world = SimWorld::with_catalog(catalog_for_tests());
    for (origin, direction) in [
        (TilePos::new(0, 1), Direction::South),
        (TilePos::new(0, 0), Direction::East),
        (TilePos::new(1, 0), Direction::South),
        (TilePos::new(1, -1), Direction::West),
        (TilePos::new(0, -1), Direction::North),
    ] {
        world
            .apply_core_command_for_tests(SimCommand::PlaceBuilding {
                def_id: "basic_belt".to_string(),
                origin,
                direction,
                inserter_drop_direction: None,
            })
            .unwrap();
    }

    for (pos, direction, input_direction) in [
        (TilePos::new(0, 1), Direction::South, Direction::South),
        (TilePos::new(0, 0), Direction::East, Direction::East),
        (TilePos::new(1, 0), Direction::South, Direction::East),
        (TilePos::new(1, -1), Direction::West, Direction::South),
        (TilePos::new(0, -1), Direction::North, Direction::West),
    ] {
        let belt = world.topology_graph.belt(pos).unwrap();
        assert_eq!(
            belt.direction, direction,
            "wrong output direction at {pos:?}"
        );
        assert_eq!(
            belt.input_direction, input_direction,
            "wrong input direction at {pos:?}"
        );
    }

    let target_line = world
        .line_window_for_tile(TilePos::new(0, 0))
        .map(|(line_id, _, _, _)| line_id)
        .unwrap();
    let top_source_line = world
        .line_window_for_tile(TilePos::new(0, 1))
        .map(|(line_id, _, _, _)| line_id)
        .unwrap();
    let loop_source_line = world
        .line_window_for_tile(TilePos::new(0, -1))
        .map(|(line_id, _, _, _)| line_id)
        .unwrap();

    assert_ne!(
        top_source_line, target_line,
        "top belt should side-load into the central straight target"
    );
    assert_eq!(
        loop_source_line, target_line,
        "bottom-left corner is part of the same compact loop line as the target"
    );

    let interactions = world.transport.interactions_sorted().collect::<Vec<_>>();
    assert!(
        interactions.iter().any(|interaction| {
            interaction.source_line() == top_source_line
                && interaction.target_line() == Some(target_line)
                && interaction.target_tile() == Some(TilePos::new(0, 0))
                && interaction.target_sort_tile() == TilePos::new(0, 1)
                && interaction.kind() == BeltInteractionKind::SideLoad { near_lane: 0 }
        }),
        "top input should build a lane-0 side-load into the central straight belt: {interactions:?}"
    );
    assert!(
        interactions.iter().any(|interaction| {
            interaction.source_line() == loop_source_line
                && interaction.target_line() == Some(target_line)
                && interaction.target_tile() == Some(TilePos::new(0, 0))
                && interaction.target_sort_tile() == TilePos::new(0, -1)
                && interaction.kind() == BeltInteractionKind::SideLoad { near_lane: 1 }
        }),
        "loop input should build a self side-load into lane 1 instead of fighting for the same point: {interactions:?}"
    );

    world
        .apply_core_command_for_tests(SimCommand::DropItemOnBeltTile {
            pos: TilePos::new(0, 1),
            lane: 0,
            distance_numerator: 128,
            distance_denominator: 128,
            item: ItemKindId(4),
        })
        .unwrap();
    world
        .apply_core_command_for_tests(SimCommand::DropItemOnBeltTile {
            pos: TilePos::new(0, -1),
            lane: 1,
            distance_numerator: 128,
            distance_denominator: 128,
            item: ItemKindId(3),
        })
        .unwrap();

    tick_world_for_tests(&mut world);

    let visible = world
        .visible_items_for_bounds(VisibleTileBounds::new(
            TilePos::new(0, 0),
            TilePos::new(1, 0),
        ))
        .collect::<Vec<_>>();
    let top_item = visible
        .iter()
        .find(|item| item.item == ItemKindId(4))
        .expect("top input item should be visible on the front belt");
    let loop_item = visible
        .iter()
        .find(|item| item.item == ItemKindId(3))
        .expect("loop input item should be visible on the front belt");

    assert_eq!(top_item.tile, TilePos::new(0, 0));
    assert_eq!(top_item.direction, Direction::East);
    assert_eq!(top_item.lane, 0);
    assert!(
        top_item.progress_numerator >= 88,
        "top input lane 0 should visually land near the target exit edge: {top_item:?}"
    );

    assert_eq!(loop_item.tile, TilePos::new(0, 0));
    assert_eq!(loop_item.direction, Direction::East);
    assert_eq!(loop_item.lane, 1);
    assert!(
        loop_item.progress_numerator >= 88,
        "loop input should visually land near the right edge, not the same point as the top input: {loop_item:?}"
    );
}

#[test]
fn screenshot_compact_loop_t_junction_keeps_working_after_manual_drop_on_target() {
    let mut world = SimWorld::with_catalog(catalog_for_tests());
    for (origin, direction) in [
        (TilePos::new(0, 1), Direction::South),
        (TilePos::new(0, 0), Direction::East),
        (TilePos::new(1, 0), Direction::South),
        (TilePos::new(1, -1), Direction::West),
        (TilePos::new(0, -1), Direction::North),
    ] {
        world
            .apply_core_command_for_tests(SimCommand::PlaceBuilding {
                def_id: "basic_belt".to_string(),
                origin,
                direction,
                inserter_drop_direction: None,
            })
            .unwrap();
    }

    world
        .apply_core_command_for_tests(SimCommand::DropItemOnBeltTile {
            pos: TilePos::new(0, 0),
            lane: 0,
            distance_numerator: 64,
            distance_denominator: 128,
            item: ItemKindId(5),
        })
        .unwrap();
    world
        .apply_core_command_for_tests(SimCommand::DropItemOnBeltTile {
            pos: TilePos::new(0, 1),
            lane: 0,
            distance_numerator: 128,
            distance_denominator: 128,
            item: ItemKindId(4),
        })
        .unwrap();
    world
        .apply_core_command_for_tests(SimCommand::DropItemOnBeltTile {
            pos: TilePos::new(0, -1),
            lane: 0,
            distance_numerator: 128,
            distance_denominator: 128,
            item: ItemKindId(3),
        })
        .unwrap();

    let mut top_item_reached_front = false;
    let mut loop_item_reached_front = false;
    let mut manual_item_kept_moving = false;
    let mut last_visible = Vec::new();
    for _ in 0..80 {
        tick_world_for_tests(&mut world);
        last_visible = world
            .visible_items_for_bounds(VisibleTileBounds::new(
                TilePos::new(0, -1),
                TilePos::new(1, 1),
            ))
            .collect::<Vec<_>>();
        top_item_reached_front |= last_visible.iter().any(|item| {
            item.item == ItemKindId(4)
                && item.tile == TilePos::new(0, 0)
                && item.direction == Direction::East
                && item.lane == 0
                && item.progress_numerator >= 88
        });
        loop_item_reached_front |= last_visible.iter().any(|item| {
            item.item == ItemKindId(3)
                && item.tile == TilePos::new(0, 0)
                && item.direction == Direction::East
                && item.lane == 1
                && item.progress_numerator >= 88
        });
        manual_item_kept_moving |= last_visible.iter().any(|item| {
            item.item == ItemKindId(5)
                && (item.tile != TilePos::new(0, 0) || item.progress_numerator != 64)
        });
    }

    assert!(
        manual_item_kept_moving,
        "manually dropped item on the T target should continue moving around the loop: {last_visible:?}"
    );
    assert!(
        top_item_reached_front,
        "top side input should still enter the T target exit half after manual drop on it: {last_visible:?}"
    );
    assert!(
        loop_item_reached_front,
        "loop side input should still enter the other T lane after manual drop on it: {last_visible:?}"
    );
}

#[test]
fn topology_rebuild_replaces_active_line_schedule() {
    let mut world = SimWorld::default();
    world
        .apply_core_command_for_tests(SimCommand::PlaceBelt {
            pos: TilePos::new(0, 0),
            direction: Direction::East,
            input_direction: Direction::East,
            speed: UnitsPerTick::new(8),
        })
        .unwrap();

    let first_ids = world.active_line_ids_for_tests();
    assert_eq!(first_ids.len(), 1);

    world
        .apply_core_command_for_tests(SimCommand::PlaceBelt {
            pos: TilePos::new(1, 0),
            direction: Direction::East,
            input_direction: Direction::East,
            speed: UnitsPerTick::new(8),
        })
        .unwrap();

    let second_ids = world.active_line_ids_for_tests();
    assert_eq!(second_ids.len(), 1);
    assert!(first_ids.iter().all(|id| !second_ids.contains(id)));

    let output = tick_world_for_tests(&mut world);

    assert_eq!(output.metrics.active_lines, 1);
    assert_eq!(output.diff.changed_lines.len(), 0);

    let sleeping_output = tick_world_for_tests(&mut world);
    assert_eq!(sleeping_output.metrics.active_lines, 0);
    assert_eq!(sleeping_output.diff.changed_lines.len(), 0);
}

#[test]
fn placing_unrelated_belt_preserves_items_on_existing_line() {
    let mut world = SimWorld::default();
    world
        .apply_core_command_for_tests(SimCommand::PlaceBelt {
            pos: TilePos::new(0, 0),
            direction: Direction::East,
            input_direction: Direction::East,
            speed: UnitsPerTick::new(8),
        })
        .unwrap();
    world
        .apply_core_command_for_tests(SimCommand::InsertItemAtLineStart {
            line_index: 0,
            lane: 0,
            item: ItemKindId(1),
        })
        .unwrap();

    world
        .apply_core_command_for_tests(SimCommand::PlaceBelt {
            pos: TilePos::new(10, 0),
            direction: Direction::East,
            input_direction: Direction::East,
            speed: UnitsPerTick::new(8),
        })
        .unwrap();

    assert_eq!(
        world
            .visible_items_for_bounds(VisibleTileBounds::new(
                TilePos::new(0, 0),
                TilePos::new(0, 0),
            ))
            .collect::<Vec<_>>()
            .len(),
        1
    );
}

#[test]
fn empty_belt_line_sleeps_without_changed_diffs() {
    let mut world = SimWorld::default();
    world
        .apply_core_command_for_tests(SimCommand::PlaceBelt {
            pos: TilePos::new(0, 0),
            direction: Direction::East,
            input_direction: Direction::East,
            speed: UnitsPerTick::new(8),
        })
        .unwrap();

    let first = tick_world_for_tests(&mut world);
    assert_eq!(first.metrics.active_lines, 1);
    assert_eq!(first.metrics.simulated_items, 0);
    assert_eq!(first.diff.changed_lines, Vec::new());

    let second = tick_world_for_tests(&mut world);
    assert_eq!(second.metrics.active_lines, 0);
    assert_eq!(second.diff.changed_lines, Vec::new());
}

#[test]
fn build_straight_belt_line_for_tests_builds_one_active_line() {
    let mut world = SimWorld::default();

    world
        .build_straight_belt_line_for_tests(
            TilePos::new(0, 0),
            4,
            Direction::East,
            UnitsPerTick::new(8),
        )
        .unwrap();

    assert_eq!(world.active_line_ids_for_tests().len(), 1);
    let output = tick_world_for_tests(&mut world);
    assert_eq!(output.metrics.active_lines, 1);
    assert_eq!(output.metrics.dirty_chunks, 1);
}

#[test]
fn placed_adjacent_catalog_belts_build_one_transport_line() {
    let mut world = SimWorld::with_catalog(catalog_for_tests());
    for y in 0..6 {
        world
            .apply_core_command_for_tests(SimCommand::PlaceBuilding {
                def_id: "basic_belt".to_string(),
                origin: TilePos::new(0, y),
                direction: Direction::South,
                inserter_drop_direction: None,
            })
            .unwrap();
    }

    assert_eq!(world.active_line_ids_for_tests().len(), 1);
}

#[test]
fn build_straight_belt_line_for_tests_rejects_occupied_tile() {
    let mut world = SimWorld::default();
    world
        .apply_core_command_for_tests(SimCommand::PlaceBelt {
            pos: TilePos::new(1, 0),
            direction: Direction::East,
            input_direction: Direction::East,
            speed: UnitsPerTick::new(8),
        })
        .unwrap();

    let error = world
        .build_straight_belt_line_for_tests(
            TilePos::new(0, 0),
            4,
            Direction::East,
            UnitsPerTick::new(8),
        )
        .unwrap_err();

    assert_eq!(
        error,
        SimCommandError::OccupiedTile {
            pos: TilePos::new(1, 0)
        }
    );
}

#[test]
fn build_straight_belt_line_rejects_unbuildable_terrain_without_partial_mutation() {
    let mut world = SimWorld::with_catalog(catalog_for_tests());
    world.set_terrain(TilePos::new(2, 0), "water").unwrap();

    let error = world
        .build_straight_belt_line_for_tests(
            TilePos::new(0, 0),
            4,
            Direction::East,
            UnitsPerTick::new(8),
        )
        .unwrap_err();

    assert_eq!(
        error,
        SimCommandError::UnbuildableTile {
            pos: TilePos::new(2, 0)
        }
    );
    assert_eq!(world.active_line_ids_for_tests().len(), 0);
    world
        .apply_core_command_for_tests(SimCommand::PlaceBelt {
            pos: TilePos::new(0, 0),
            direction: Direction::East,
            input_direction: Direction::East,
            speed: UnitsPerTick::new(8),
        })
        .unwrap();
}

#[test]
fn drop_item_on_belt_tile_inserts_into_requested_tile_window() {
    let mut world = SimWorld::default();
    world
        .build_straight_belt_line_for_tests(
            TilePos::new(0, 0),
            3,
            Direction::East,
            UnitsPerTick::new(8),
        )
        .unwrap();

    world
        .apply_core_command_for_tests(SimCommand::DropItemOnBeltTile {
            pos: TilePos::new(1, 0),
            lane: 1,
            distance_numerator: 64,
            distance_denominator: 128,
            item: ItemKindId(3),
        })
        .unwrap();

    let visible = world
        .visible_items_for_bounds(VisibleTileBounds::new(
            TilePos::new(1, 0),
            TilePos::new(1, 0),
        ))
        .collect::<Vec<_>>();
    assert_eq!(visible.len(), 1);
    assert_eq!(visible[0].tile, TilePos::new(1, 0));
    assert_eq!(visible[0].item, ItemKindId(3));
}

#[test]
fn player_can_take_item_from_belt_tile() {
    let mut world = SimWorld::with_catalog(catalog_for_tests());
    world
        .apply_core_command_for_tests(SimCommand::PlaceBuilding {
            def_id: "basic_belt".to_string(),
            origin: TilePos::new(0, 0),
            direction: Direction::East,
            inserter_drop_direction: None,
        })
        .unwrap();
    world
        .drop_item_on_belt_tile(TilePos::new(0, 0), 0, 64, 128, TEST_IRON_ORE)
        .unwrap();

    let stack = world.take_item_from_belt_tile(TilePos::new(0, 0)).unwrap();

    assert_eq!(stack.kind, TEST_IRON_ORE);
    assert_eq!(stack.amount, 1);
    assert!(
        world
            .visible_items_for_bounds(VisibleTileBounds::new(
                TilePos::new(-1, -1),
                TilePos::new(2, 2),
            ))
            .next()
            .is_none()
    );
}

#[test]
fn player_can_take_item_from_splitter_internal_lane() {
    let mut world = connected_splitter_world_without_outputs_for_tests();
    world
        .apply_core_command_for_tests(SimCommand::DropItemOnBeltTile {
            pos: TilePos::new(-1, 0),
            lane: 0,
            distance_numerator: 128,
            distance_denominator: 128,
            item: TEST_IRON_ORE,
        })
        .unwrap();
    tick_world_for_tests(&mut world);

    let stack = world.take_item_from_belt_tile(TilePos::new(0, 0)).unwrap();

    assert_eq!(stack.kind, TEST_IRON_ORE);
    assert_eq!(stack.amount, 1);
    assert_eq!(
        production_splitter_runtime(&world).ingress_items,
        Vec::new()
    );
    assert!(
        SimRenderView::extract(
            &world,
            VisibleTileBounds::new(TilePos::new(0, 0), TilePos::new(0, 1)),
        )
        .visible_splitter_items
        .is_empty()
    );
}

#[test]
fn player_can_take_item_from_splitter_egress_lane() {
    let mut world = connected_splitter_world_without_outputs_for_tests();
    world
        .apply_core_command_for_tests(SimCommand::DropItemOnBeltTile {
            pos: TilePos::new(-1, 0),
            lane: 0,
            distance_numerator: 128,
            distance_denominator: 128,
            item: TEST_IRON_ORE,
        })
        .unwrap();
    tick_splitter_until_center(&mut world);
    assert_eq!(production_splitter_runtime(&world).egress_items.len(), 1);

    let stack = world.take_item_from_belt_tile(TilePos::new(0, 0)).unwrap();

    assert_eq!(stack.kind, TEST_IRON_ORE);
    assert_eq!(stack.amount, 1);
    assert_eq!(production_splitter_runtime(&world).egress_items, Vec::new());
}

#[test]
fn player_takes_nearest_item_across_belt_lanes() {
    let mut world = SimWorld::with_catalog(catalog_for_tests());
    world
        .apply_core_command_for_tests(SimCommand::PlaceBuilding {
            def_id: "basic_belt".to_string(),
            origin: TilePos::new(0, 0),
            direction: Direction::East,
            inserter_drop_direction: None,
        })
        .unwrap();
    world
        .drop_item_on_belt_tile(TilePos::new(0, 0), 0, 32, 128, TEST_IRON_ORE)
        .unwrap();
    world
        .drop_item_on_belt_tile(TilePos::new(0, 0), 1, 96, 128, TEST_COPPER_ORE)
        .unwrap();

    let stack = world.take_item_from_belt_tile(TilePos::new(0, 0)).unwrap();

    assert_eq!(stack.kind, TEST_COPPER_ORE);
    assert_eq!(stack.amount, 1);
    let visible = world
        .visible_items_for_bounds(VisibleTileBounds::new(
            TilePos::new(0, 0),
            TilePos::new(0, 0),
        ))
        .collect::<Vec<_>>();
    assert_eq!(visible.len(), 1);
    assert_eq!(visible[0].item, TEST_IRON_ORE);
    assert_eq!(visible[0].lane, 0);
}

#[test]
fn drop_item_on_belt_tile_nudges_line_to_keep_cursor_position() {
    let mut world = SimWorld::default();
    world
        .build_straight_belt_line_for_tests(
            TilePos::new(0, 0),
            3,
            Direction::East,
            UnitsPerTick::new(8),
        )
        .unwrap();

    for _ in 0..12 {
        world
            .apply_core_command_for_tests(SimCommand::DropItemOnBeltTile {
                pos: TilePos::new(1, 0),
                lane: 0,
                distance_numerator: 64,
                distance_denominator: 128,
                item: ItemKindId(3),
            })
            .unwrap();
    }

    let error = world
        .apply_core_command_for_tests(SimCommand::DropItemOnBeltTile {
            pos: TilePos::new(1, 0),
            lane: 0,
            distance_numerator: 64,
            distance_denominator: 128,
            item: ItemKindId(3),
        })
        .unwrap_err();

    assert_eq!(error, SimCommandError::CapacityExceeded);
    let visible = world
        .visible_items_for_bounds(VisibleTileBounds::new(
            TilePos::new(0, 0),
            TilePos::new(2, 0),
        ))
        .collect::<Vec<_>>();
    assert_eq!(visible.len(), 12);
    assert_eq!(
        visible
            .iter()
            .filter(|item| item.tile == TilePos::new(1, 0)
                && item.lane == 0
                && item.progress_numerator == 64)
            .count(),
        1
    );
}

#[test]
fn drop_item_on_belt_tile_can_fill_full_lane_by_nudging_from_one_cursor_position() {
    let mut world = SimWorld::default();
    world
        .build_straight_belt_line_for_tests(
            TilePos::new(0, 0),
            5,
            Direction::East,
            UnitsPerTick::new(8),
        )
        .unwrap();

    for _ in 0..20 {
        world
            .apply_core_command_for_tests(SimCommand::DropItemOnBeltTile {
                pos: TilePos::new(2, 0),
                lane: 0,
                distance_numerator: 64,
                distance_denominator: 128,
                item: ItemKindId(3),
            })
            .unwrap();
    }

    let error = world
        .apply_core_command_for_tests(SimCommand::DropItemOnBeltTile {
            pos: TilePos::new(2, 0),
            lane: 0,
            distance_numerator: 64,
            distance_denominator: 128,
            item: ItemKindId(3),
        })
        .unwrap_err();

    assert_eq!(error, SimCommandError::CapacityExceeded);
    let visible = world
        .visible_items_for_bounds(VisibleTileBounds::new(
            TilePos::new(0, 0),
            TilePos::new(4, 0),
        ))
        .collect::<Vec<_>>();
    assert_eq!(visible.len(), 20);
    for tile_x in 0..5 {
        let mut progress = visible
            .iter()
            .filter(|item| item.tile == TilePos::new(tile_x, 0) && item.lane == 0)
            .map(|item| item.progress_numerator)
            .collect::<Vec<_>>();
        progress.sort_unstable();
        assert_eq!(progress, vec![32, 64, 96, 128]);
    }
}

#[test]
fn drop_item_on_belt_tile_snaps_edge_cursor_inside_line_capacity() {
    let mut world = SimWorld::default();
    world
        .build_straight_belt_line_for_tests(
            TilePos::new(0, 0),
            1,
            Direction::East,
            UnitsPerTick::new(8),
        )
        .unwrap();

    world
        .apply_core_command_for_tests(SimCommand::DropItemOnBeltTile {
            pos: TilePos::new(0, 0),
            lane: 0,
            distance_numerator: 0,
            distance_denominator: 128,
            item: ItemKindId(3),
        })
        .unwrap();

    let visible = world
        .visible_items_for_bounds(VisibleTileBounds::new(
            TilePos::new(0, 0),
            TilePos::new(0, 0),
        ))
        .collect::<Vec<_>>();
    assert_eq!(visible.len(), 1);
    assert_eq!(visible[0].progress_numerator, 32);
}

#[test]
fn belt_items_continue_through_corner_tiles() {
    let mut world = SimWorld::default();
    for (pos, direction) in [
        (TilePos::new(0, 0), Direction::East),
        (TilePos::new(1, 0), Direction::East),
        (TilePos::new(2, 0), Direction::South),
        (TilePos::new(2, -1), Direction::South),
    ] {
        world
            .apply_core_command_for_tests(SimCommand::PlaceBelt {
                pos,
                direction,
                input_direction: direction,
                speed: UnitsPerTick::new(8),
            })
            .unwrap();
    }

    world
        .apply_core_command_for_tests(SimCommand::DropItemOnBeltTile {
            pos: TilePos::new(0, 0),
            lane: 0,
            distance_numerator: 64,
            distance_denominator: 128,
            item: ItemKindId(3),
        })
        .unwrap();
    for _ in 0..70 {
        tick_world_for_tests(&mut world);
    }

    let visible = world
        .visible_items_for_bounds(VisibleTileBounds::new(
            TilePos::new(2, -1),
            TilePos::new(2, 0),
        ))
        .collect::<Vec<_>>();

    assert!(
        visible
            .iter()
            .any(|item| item.tile == TilePos::new(2, 0) || item.tile == TilePos::new(2, -1)),
        "item should move into or through the corner segment, got {visible:?}"
    );
}

#[test]
fn end_transfer_preserves_lane() {
    let mut world = world_with_end_transfer_for_tests(
        1,
        PackedItemStream::default(),
        PackedItemStream::default(),
    );

    tick_world_for_tests(&mut world);

    let visible = world
        .visible_items_for_bounds(VisibleTileBounds::new(
            TilePos::new(1, 0),
            TilePos::new(1, 0),
        ))
        .collect::<Vec<_>>();
    assert!(visible.iter().any(|item| item.lane == 1));
    assert!(!visible.iter().any(|item| item.lane == 0));
}

#[test]
fn end_transfer_blocks_when_same_lane_full_without_route_hint() {
    let blocking_lane = PackedItemStream::from_gaps(
        vec![ItemKindId(4)],
        DistanceUnits::new(96),
        vec![],
        DistanceUnits::new(32),
    );
    let mut world =
        world_with_end_transfer_for_tests(0, blocking_lane, PackedItemStream::default());

    let blocked_output = tick_world_for_tests(&mut world);
    assert_eq!(blocked_output.diff.route_hints, Vec::new());

    let source_items = world
        .visible_items_for_bounds(VisibleTileBounds::new(
            TilePos::new(0, 0),
            TilePos::new(0, 0),
        ))
        .collect::<Vec<_>>();
    assert!(source_items.iter().any(|item| item.item == ItemKindId(3)));

    let blocker_distance = world
        .transport
        .line(LineId(2))
        .unwrap()
        .first_in_window(0, DistanceUnits::ZERO, DistanceUnits::new(127))
        .unwrap()
        .distance;
    world
        .transport
        .line_mut(LineId(2))
        .unwrap()
        .remove_one_at_distance(0, blocker_distance)
        .unwrap();
    tick_world_for_tests(&mut world);

    let target_items = world
        .visible_items_for_bounds(VisibleTileBounds::new(
            TilePos::new(1, 0),
            TilePos::new(1, 0),
        ))
        .collect::<Vec<_>>();
    assert!(target_items.iter().any(|item| item.item == ItemKindId(3)));
    assert!(!target_items.iter().any(|item| item.lane == 1));
}

#[test]
fn successful_node_transfer_emits_route_hint() {
    let mut world = world_with_end_transfer_for_tests(
        1,
        PackedItemStream::default(),
        PackedItemStream::default(),
    );

    let output = tick_world_for_tests(&mut world);

    assert_eq!(output.diff.route_hints.len(), 1);
    let hint = output.diff.route_hints[0];
    assert_eq!(hint.item, ItemKindId(3));
    assert_eq!(hint.from.node, TransportNodeId(1));
    assert_eq!(hint.from.line, LineId(1));
    assert_eq!(hint.from.lane, 1);
    assert_eq!(hint.from.role, TransportPortRole::Input);
    assert_eq!(hint.to.node, TransportNodeId(1));
    assert_eq!(hint.to.line, LineId(2));
    assert_eq!(hint.to.lane, 1);
    assert_eq!(hint.to.role, TransportPortRole::Output);
    assert_eq!(hint.center, TilePos::new(1, 0));
    assert_eq!(hint.target_tile, TilePos::new(1, 0));
    assert_eq!(hint.progress_numerator, 0);
    assert_eq!(hint.progress_denominator, 128);
    assert_eq!(hint.start_tick, output.tick.raw());
    assert_eq!(hint.end_tick, output.tick.raw() + 1);
}

#[test]
fn successful_side_load_emits_route_hint_with_source_and_target_lanes() {
    let mut world = SimWorld::default();
    let source_line = LineId(1);
    let target_line = LineId(2);
    let source_tile = TilePos::new(-1, 0);
    let target_tile = TilePos::new(0, 0);
    let source_side = Direction::West;
    let target_direction = Direction::North;
    let near_lane = target_direction
        .near_lane_for_source_side(source_side)
        .expect("west input should side-load onto north belt");
    let mut source_lanes = [PackedItemStream::default(), PackedItemStream::default()];
    source_lanes[1] = PackedItemStream::from_gaps(
        vec![TEST_IRON_ORE],
        DistanceUnits::ZERO,
        vec![],
        DistanceUnits::new(128),
    );

    let mut transport = TransportStorage::default();
    transport.insert_line(TransportLine::new(
        source_line,
        GroupId(source_line.0),
        LinePath::new(vec![LineTile::new(source_tile)]),
        UnitsPerTick::new(0),
        source_lanes,
        LineEndpoint::Blocked,
        LineEndpoint::Open,
    ));
    transport.insert_line(TransportLine::new(
        target_line,
        GroupId(target_line.0),
        LinePath::new(vec![LineTile::new(target_tile)]),
        UnitsPerTick::new(0),
        [PackedItemStream::default(), PackedItemStream::default()],
        LineEndpoint::Open,
        LineEndpoint::Open,
    ));
    transport.insert_node(TransportNode::side_load_to(
        TransportNodeId(7),
        source_tile,
        target_tile,
        source_line,
        target_line,
        near_lane,
    ));

    world
        .topology_graph
        .set_belt(source_tile, BeltTile::new(Direction::East));
    world
        .topology_graph
        .set_belt(target_tile, BeltTile::new(target_direction));
    world
        .activation
        .replace_active_lines([source_line, target_line]);
    world.transport = transport;

    let output = tick_world_for_tests(&mut world);

    assert_eq!(output.diff.route_hints.len(), 1);
    let hint = output.diff.route_hints[0];
    assert_eq!(hint.item, TEST_IRON_ORE);
    assert_eq!(hint.from.node, TransportNodeId(7));
    assert_eq!(hint.from.line, source_line);
    assert_eq!(hint.from.lane, 1);
    assert_eq!(hint.from.role, TransportPortRole::Input);
    assert_eq!(hint.to.node, TransportNodeId(7));
    assert_eq!(hint.to.line, target_line);
    assert_eq!(hint.to.lane, near_lane);
    assert_eq!(hint.to.role, TransportPortRole::Output);
    assert_eq!(hint.center, target_tile);
    assert_eq!(hint.target_tile, target_tile);
    assert_eq!(hint.progress_numerator, 32);
    assert_eq!(hint.progress_denominator, 128);
    assert_eq!(hint.start_tick, output.tick.raw());
    assert_eq!(hint.end_tick, output.tick.raw() + 1);
}

#[test]
fn splitter_accepts_item_into_internal_route_without_output_teleport() {
    let mut world = world_with_splitter_for_tests();
    insert_splitter_source_item(&mut world, 0, TEST_IRON_ORE);

    let output = tick_world_for_tests(&mut world);

    assert_eq!(output.diff.route_hints, Vec::new());
    assert_eq!(line_lane_items(&world, LineId(1), 0), Vec::new());
    assert_eq!(line_lane_items(&world, LineId(3), 0), Vec::new());

    let runtime = splitter_runtime(&world);
    assert_eq!(runtime.next_output, 0);
    assert_eq!(
        runtime.ingress_items,
        vec![SplitterIngressItem {
            item: TEST_IRON_ORE,
            input_channel: 0,
            lane: 0,
            progress: DistanceUnits::ZERO,
        }]
    );
    assert_eq!(runtime.buffered_items, Vec::new());
    assert_eq!(runtime.egress_items, Vec::new());
}

#[test]
fn splitter_waits_for_source_item_to_reach_front_edge_before_ingress() {
    let mut world = world_with_splitter_for_tests();
    insert_line_item_at_distance(
        &mut world,
        LineId(1),
        0,
        TEST_IRON_ORE,
        DistanceUnits::new(1),
    );

    tick_world_for_tests(&mut world);

    assert_eq!(line_lane_items(&world, LineId(1), 0), vec![TEST_IRON_ORE]);
    assert_eq!(splitter_runtime(&world).ingress_items, Vec::new());
}

#[test]
fn splitter_internal_item_is_visible_in_render_view() {
    let mut world = world_with_splitter_for_tests();
    insert_splitter_source_item(&mut world, 0, TEST_IRON_ORE);

    tick_world_for_tests(&mut world);

    let view = SimRenderView::extract(
        &world,
        VisibleTileBounds::new(TilePos::new(0, 0), TilePos::new(0, 1)),
    );
    assert_eq!(
        view.visible_splitter_items,
        vec![VisibleSplitterItem {
            origin: TilePos::new(0, 0),
            item: TEST_IRON_ORE,
            direction: Direction::East,
            input_channel: 0,
            output_channel: 0,
            lane: 0,
            phase: VisibleSplitterItemPhase::Ingress,
            progress_numerator: 0,
            progress_denominator: 128,
        }]
    );
}

#[test]
fn splitter_internal_item_advances_by_splitter_speed() {
    let mut world = world_with_splitter_for_tests();
    insert_splitter_source_item(&mut world, 0, TEST_IRON_ORE);

    tick_world_for_tests(&mut world);
    tick_world_for_tests(&mut world);

    assert_eq!(
        splitter_runtime(&world).ingress_items[0].progress,
        DistanceUnits::new(4)
    );
}

#[test]
fn connected_splitter_internal_item_advances_by_placed_splitter_speed() {
    let mut catalog = catalog_for_tests();
    catalog
        .buildings
        .iter_mut()
        .find(|building| building.id == "basic_splitter")
        .unwrap()
        .behavior = CoreBuildingBehavior::splitter(UnitsPerTick::new(12));
    let mut world = connected_splitter_world_with_catalog_for_tests(catalog);

    world
        .apply_core_command_for_tests(SimCommand::DropItemOnBeltTile {
            pos: TilePos::new(-1, 0),
            lane: 0,
            distance_numerator: 128,
            distance_denominator: 128,
            item: TEST_IRON_ORE,
        })
        .unwrap();

    tick_world_for_tests(&mut world);
    tick_world_for_tests(&mut world);

    assert_eq!(
        production_splitter_runtime(&world).ingress_items[0].progress,
        DistanceUnits::new(12)
    );
}

#[test]
fn splitter_moves_centered_item_to_internal_egress_without_output_belts() {
    let mut world = connected_splitter_world_without_outputs_for_tests();
    world
        .apply_core_command_for_tests(SimCommand::DropItemOnBeltTile {
            pos: TilePos::new(-1, 0),
            lane: 0,
            distance_numerator: 128,
            distance_denominator: 128,
            item: TEST_IRON_ORE,
        })
        .unwrap();

    tick_splitter_until_center(&mut world);

    let runtime = production_splitter_runtime(&world);
    assert_eq!(runtime.ingress_items, Vec::new());
    assert_eq!(runtime.buffered_items, Vec::new());
    assert_eq!(runtime.egress_items.len(), 1);
    assert_eq!(runtime.egress_items[0].item, TEST_IRON_ORE);
    assert_eq!(runtime.egress_items[0].output_channel, 0);
    assert_eq!(runtime.egress_items[0].lane, 0);
    assert_eq!(
        total_items_on_line(
            &world,
            line_id_by_exact_path(&world, &[TilePos::new(-1, 0)])
        ),
        0
    );

    let visible = SimRenderView::extract(
        &world,
        VisibleTileBounds::new(TilePos::new(0, 0), TilePos::new(0, 1)),
    );
    assert_eq!(visible.visible_splitter_items.len(), 1);
    assert_eq!(
        visible.visible_splitter_items[0].phase,
        VisibleSplitterItemPhase::Egress
    );
}

#[test]
fn splitter_uses_remaining_output_channel_when_preferred_channel_is_removed() {
    let mut world = connected_splitter_world_for_tests();
    let splitter_node_id = world
        .transport
        .nodes_sorted()
        .find(|node| node.kind == TransportNodeKind::Splitter2x1)
        .unwrap()
        .id;
    world
        .transport
        .splitter_runtime_mut(splitter_node_id)
        .unwrap()
        .set_next_output_for_all_lanes(1);
    world
        .apply_core_command_for_tests(SimCommand::DropItemOnBeltTile {
            pos: TilePos::new(-1, 0),
            lane: 1,
            distance_numerator: 128,
            distance_denominator: 128,
            item: TEST_IRON_ORE,
        })
        .unwrap();

    tick_world_for_tests(&mut world);
    world
        .apply_core_command_for_tests(SimCommand::RemoveBuilding {
            pos: TilePos::new(1, 1),
        })
        .unwrap();
    world.rebuild_transport_lines();
    tick_splitter_until_egress(&mut world);

    assert_eq!(
        line_lane_items(
            &world,
            line_id_by_exact_path(&world, &[TilePos::new(1, 0)]),
            1
        ),
        vec![TEST_IRON_ORE]
    );
    assert_eq!(
        production_splitter_runtime(&world).buffered_items,
        Vec::new()
    );
    assert_eq!(production_splitter_runtime(&world).egress_items, Vec::new());
}

#[test]
fn splitter_does_not_change_lane_when_matching_lane_is_blocked() {
    let mut world = world_with_splitter_for_tests();
    insert_splitter_source_item_on_lane(&mut world, 0, 0, TEST_IRON_ORE);
    insert_splitter_output_blocker_on_lane(&mut world, 0, 0);
    insert_splitter_output_blocker_on_lane(&mut world, 1, 0);

    tick_splitter_until_egress(&mut world);

    assert_eq!(line_lane_items(&world, LineId(3), 1), Vec::new());
    assert_eq!(line_lane_items(&world, LineId(4), 1), Vec::new());
    assert_eq!(
        total_splitter_runtime_items(&splitter_runtime(&world)) + total_splitter_items(&world),
        3
    );
}

#[test]
fn splitter_egresses_to_selected_output_after_route_completion() {
    let mut world = world_with_splitter_for_tests();
    insert_splitter_source_item(&mut world, 0, TEST_IRON_ORE);

    tick_splitter_until_egress(&mut world);

    assert_eq!(total_splitter_runtime_items(&splitter_runtime(&world)), 0);
    assert_eq!(line_lane_items(&world, LineId(3), 0), vec![TEST_IRON_ORE]);
}

#[test]
fn blocked_outputs_keep_item_inside_splitter() {
    let mut world = world_with_splitter_for_tests();
    insert_splitter_source_item(&mut world, 0, TEST_IRON_ORE);
    insert_splitter_output_blocker(&mut world, 0);
    insert_splitter_output_blocker(&mut world, 1);

    tick_splitter_until_egress(&mut world);

    let runtime = splitter_runtime(&world);
    assert_eq!(line_lane_items(&world, LineId(3), 0), vec![TEST_WOOD]);
    assert_eq!(line_lane_items(&world, LineId(4), 0), vec![TEST_WOOD]);
    assert_eq!(runtime.buffered_items, Vec::new());
    assert_eq!(runtime.egress_items.len(), 1);
    assert_eq!(runtime.egress_items[0].item, TEST_IRON_ORE);
    assert_eq!(
        runtime.egress_items[0].progress,
        DistanceUnits::new(DistanceUnits::UNITS_PER_TILE / 2)
    );
}

#[test]
fn blocked_outputs_allow_multiple_items_on_internal_egress_lane() {
    let mut world = world_with_splitter_for_tests();
    insert_splitter_output_blocker(&mut world, 0);
    insert_splitter_output_blocker(&mut world, 1);

    for item in [TEST_IRON_ORE, TEST_COPPER_ORE, TEST_COAL] {
        insert_splitter_source_item(&mut world, 0, item);
        tick_splitter_until_egress(&mut world);
    }

    let runtime = splitter_runtime(&world);
    assert_eq!(runtime.buffered_items, Vec::new());
    assert_eq!(runtime.egress_items.len(), 3);
    assert!(runtime.egress_items.iter().any(|item| {
        item.item == TEST_IRON_ORE
            && item.output_channel == 0
            && item.lane == 0
            && item.progress == DistanceUnits::new(DistanceUnits::UNITS_PER_TILE / 2)
    }));
    assert!(runtime.egress_items.iter().any(|item| {
        item.item == TEST_COAL
            && item.output_channel == 0
            && item.lane == 0
            && item.progress < DistanceUnits::new(DistanceUnits::UNITS_PER_TILE / 2)
    }));
}

#[test]
fn full_splitter_center_buffer_does_not_block_empty_ingress_path() {
    let mut world = world_with_splitter_for_tests();
    insert_splitter_output_blocker(&mut world, 0);
    insert_splitter_output_blocker(&mut world, 1);
    insert_splitter_source_item(&mut world, 0, TEST_IRON_GEAR);

    let runtime = world
        .transport
        .splitter_runtime_mut(TransportNodeId(9))
        .unwrap();
    runtime.buffered_items = vec![
        SplitterBufferedItem {
            item: TEST_IRON_ORE,
            source_channel: 0,
            lane: 0,
        };
        5
    ];
    runtime.egress_items = [0, 1]
        .into_iter()
        .flat_map(|output_channel| {
            [
                DistanceUnits::ZERO,
                DistanceUnits::new(64),
                DistanceUnits::new(DistanceUnits::UNITS_PER_TILE / 2),
            ]
            .into_iter()
            .map(move |progress| SplitterEgressItem {
                item: TEST_COPPER_ORE,
                source_channel: 0,
                output_channel,
                lane: 0,
                progress,
            })
        })
        .collect();

    tick_world_for_tests(&mut world);

    let runtime = splitter_runtime(&world);
    assert_eq!(line_lane_items(&world, LineId(1), 0), Vec::new());
    assert_eq!(runtime.buffered_items.len(), 5);
    assert_eq!(runtime.ingress_items.len(), 1);
    assert_eq!(runtime.ingress_items[0].item, TEST_IRON_GEAR);
}

#[test]
fn splitter_single_active_input_alternates_between_free_outputs() {
    let mut world = world_with_splitter_for_tests();

    insert_splitter_source_item(&mut world, 0, TEST_IRON_ORE);
    tick_splitter_until_egress(&mut world);
    assert_eq!(line_lane_items(&world, LineId(3), 0), vec![TEST_IRON_ORE]);
    assert_eq!(splitter_next_output(&world), 1);

    remove_first_line_lane_item(&mut world, LineId(3), 0);
    insert_splitter_source_item(&mut world, 0, TEST_COPPER_ORE);
    tick_splitter_until_egress(&mut world);

    assert_eq!(line_lane_items(&world, LineId(3), 0), Vec::new());
    assert_eq!(line_lane_items(&world, LineId(4), 0), vec![TEST_COPPER_ORE]);
    assert_eq!(splitter_next_output(&world), 0);
}

#[test]
fn splitter_alternates_outputs_independently_per_lane() {
    let mut world = world_with_splitter_for_tests();

    insert_splitter_source_item_on_lane(&mut world, 0, 0, TEST_IRON_ORE);
    insert_splitter_source_item_on_lane(&mut world, 0, 1, TEST_COPPER_ORE);
    tick_splitter_until_egress(&mut world);

    assert_eq!(line_lane_items(&world, LineId(3), 0), vec![TEST_IRON_ORE]);
    assert_eq!(line_lane_items(&world, LineId(3), 1), vec![TEST_COPPER_ORE]);
    assert_eq!(line_lane_items(&world, LineId(4), 0), Vec::new());
    assert_eq!(line_lane_items(&world, LineId(4), 1), Vec::new());

    remove_first_line_lane_item(&mut world, LineId(3), 0);
    remove_first_line_lane_item(&mut world, LineId(3), 1);
    insert_splitter_source_item_on_lane(&mut world, 0, 0, TEST_COAL);
    insert_splitter_source_item_on_lane(&mut world, 0, 1, TEST_WOOD);
    tick_splitter_until_egress(&mut world);

    assert_eq!(line_lane_items(&world, LineId(3), 0), Vec::new());
    assert_eq!(line_lane_items(&world, LineId(3), 1), Vec::new());
    assert_eq!(line_lane_items(&world, LineId(4), 0), vec![TEST_COAL]);
    assert_eq!(line_lane_items(&world, LineId(4), 1), vec![TEST_WOOD]);
}

#[test]
fn splitter_uses_other_output_channel_when_preferred_output_lane_is_full() {
    let mut world = world_with_splitter_for_tests();
    insert_splitter_source_item(&mut world, 0, TEST_IRON_ORE);
    insert_splitter_output_blocker(&mut world, 0);

    tick_splitter_until_egress(&mut world);

    let runtime = splitter_runtime(&world);
    assert_eq!(line_lane_items(&world, LineId(1), 0), Vec::new());
    assert_eq!(line_lane_items(&world, LineId(3), 0), vec![TEST_WOOD]);
    assert_eq!(line_lane_items(&world, LineId(4), 0), vec![TEST_IRON_ORE]);
    assert_eq!(total_splitter_runtime_items(&runtime), 0);
    assert_eq!(runtime.next_output, 0);
}

#[test]
fn splitter_keeps_input_item_when_both_outputs_are_full() {
    let mut world = world_with_splitter_for_tests();
    insert_splitter_source_item(&mut world, 0, TEST_IRON_ORE);
    insert_splitter_output_blocker(&mut world, 0);
    insert_splitter_output_blocker(&mut world, 1);

    let output = tick_world_for_tests(&mut world);

    assert_eq!(output.diff.route_hints, Vec::new());
    assert_eq!(line_lane_items(&world, LineId(1), 0), Vec::new());
    assert_eq!(line_lane_items(&world, LineId(3), 0), vec![TEST_WOOD]);
    assert_eq!(line_lane_items(&world, LineId(4), 0), vec![TEST_WOOD]);
    assert_eq!(splitter_runtime(&world).ingress_items.len(), 1);
    assert_eq!(splitter_next_output(&world), 0);
}

#[test]
fn splitter_two_inputs_merge_deterministically_into_one_available_output() {
    let mut world = world_with_splitter_for_tests();
    insert_splitter_source_item(&mut world, 0, TEST_IRON_ORE);
    insert_splitter_source_item_on_lane(&mut world, 1, 0, TEST_COPPER_ORE);
    insert_splitter_output_blocker(&mut world, 0);

    tick_splitter_until_egress(&mut world);

    assert_eq!(line_lane_items(&world, LineId(1), 0), Vec::new());
    assert_eq!(line_lane_items(&world, LineId(2), 0), Vec::new());
    assert_eq!(line_lane_items(&world, LineId(3), 0), vec![TEST_WOOD]);
    assert_eq!(line_lane_items(&world, LineId(4), 0), vec![TEST_IRON_ORE]);
    let runtime = splitter_runtime(&world);
    assert!(
        runtime
            .buffered_items
            .iter()
            .any(|item| item.item == TEST_COPPER_ORE)
            || runtime
                .egress_items
                .iter()
                .any(|item| item.item == TEST_COPPER_ORE)
    );
    assert_eq!(
        total_splitter_items(&world) + total_splitter_runtime_items(&runtime),
        3
    );
}

#[test]
fn splitter_routes_both_lanes_of_same_input_belt() {
    let mut world = world_with_splitter_for_tests();
    insert_splitter_source_item_on_lane(&mut world, 0, 1, TEST_IRON_ORE);

    tick_splitter_until_egress(&mut world);

    assert_eq!(line_lane_items(&world, LineId(1), 1), Vec::new());
    assert_eq!(line_lane_items(&world, LineId(3), 1), vec![TEST_IRON_ORE]);
    assert_eq!(splitter_next_output(&world), 1);
}

#[test]
fn splitter_routes_both_lanes_of_second_input_belt() {
    let mut world = world_with_splitter_for_tests();
    insert_splitter_source_item_on_lane(&mut world, 1, 0, TEST_COPPER_ORE);

    tick_splitter_until_egress(&mut world);

    assert_eq!(line_lane_items(&world, LineId(2), 0), Vec::new());
    assert_eq!(line_lane_items(&world, LineId(3), 0), vec![TEST_COPPER_ORE]);
    assert_eq!(splitter_next_output(&world), 1);
}

#[test]
fn successful_splitter_ingress_emits_no_route_hint_and_stores_internal_item() {
    let mut world = world_with_splitter_for_tests();
    insert_splitter_source_item(&mut world, 0, TEST_IRON_ORE);

    let output = tick_world_for_tests(&mut world);

    let runtime = splitter_runtime(&world);
    assert_eq!(output.diff.route_hints, Vec::new());
    assert_eq!(runtime.ingress_items.len(), 1);
    assert_eq!(runtime.ingress_items[0].item, TEST_IRON_ORE);
    assert_eq!(runtime.ingress_items[0].input_channel, 0);
    assert_eq!(runtime.ingress_items[0].lane, 0);
}

#[test]
fn splitter_runtime_next_output_survives_snapshot_restore() {
    let mut world = world_with_splitter_for_tests();
    world
        .transport
        .splitter_runtime_mut(TransportNodeId(9))
        .unwrap()
        .set_next_output_for_all_lanes(1);

    let snapshot = world.snapshot();
    let restored = SimWorld::from_snapshot(catalog_for_tests(), snapshot).unwrap();

    assert_eq!(splitter_next_output(&restored), 1);
}

#[test]
fn splitter_internal_state_survives_snapshot_restore() {
    let mut world = world_with_splitter_for_tests();
    insert_splitter_source_item(&mut world, 0, TEST_IRON_ORE);
    tick_world_for_tests(&mut world);
    tick_world_for_tests(&mut world);

    let expected_runtime = splitter_runtime(&world);
    let restored = SimWorld::from_snapshot(catalog_for_tests(), world.snapshot()).unwrap();

    assert_eq!(splitter_runtime(&restored), expected_runtime);
}

#[test]
fn splitter_digest_changes_when_internal_item_progress_changes() {
    let mut left = world_with_splitter_for_tests();
    insert_splitter_source_item(&mut left, 0, TEST_IRON_ORE);
    tick_world_for_tests(&mut left);

    let mut right = SimWorld::from_snapshot(catalog_for_tests(), left.snapshot()).unwrap();
    right
        .transport
        .splitter_runtime_mut(TransportNodeId(9))
        .unwrap()
        .ingress_items[0]
        .progress = DistanceUnits::new(1);

    assert_ne!(left.digest(), right.digest());
}

#[test]
fn underground_digest_changes_when_transport_runtime_fields_change() {
    let baseline = world_with_underground_transport_runtime(
        DistanceUnits::new(4 * DistanceUnits::UNITS_PER_TILE),
        TEST_IRON_ORE,
        0,
        DistanceUnits::new(1),
    );

    let different_distance = world_with_underground_transport_runtime(
        DistanceUnits::new(5 * DistanceUnits::UNITS_PER_TILE),
        TEST_IRON_ORE,
        0,
        DistanceUnits::new(1),
    );
    let different_item = world_with_underground_transport_runtime(
        DistanceUnits::new(4 * DistanceUnits::UNITS_PER_TILE),
        TEST_COPPER_ORE,
        0,
        DistanceUnits::new(1),
    );
    let different_lane = world_with_underground_transport_runtime(
        DistanceUnits::new(4 * DistanceUnits::UNITS_PER_TILE),
        TEST_IRON_ORE,
        1,
        DistanceUnits::new(1),
    );
    let different_progress = world_with_underground_transport_runtime(
        DistanceUnits::new(4 * DistanceUnits::UNITS_PER_TILE),
        TEST_IRON_ORE,
        0,
        DistanceUnits::new(2),
    );

    assert_ne!(baseline.digest(), different_distance.digest());
    assert_ne!(baseline.digest(), different_item.digest());
    assert_ne!(baseline.digest(), different_lane.digest());
    assert_ne!(baseline.digest(), different_progress.digest());
}

#[test]
fn splitter_runtime_next_output_survives_transport_rebuild() {
    let mut world = connected_splitter_world_for_tests();

    world
        .apply_core_command_for_tests(SimCommand::DropItemOnBeltTile {
            pos: TilePos::new(-1, 0),
            lane: 0,
            distance_numerator: 128,
            distance_denominator: 128,
            item: TEST_IRON_ORE,
        })
        .unwrap();
    tick_splitter_until_egress(&mut world);
    let next_output = production_splitter_next_output(&world);
    assert_eq!(next_output, 1);

    world.rebuild_transport_lines();

    assert_eq!(production_splitter_next_output(&world), next_output);
}

#[test]
fn splitter_building_runtime_survives_snapshot_restore() {
    let mut world = connected_splitter_world_for_tests();

    world
        .apply_core_command_for_tests(SimCommand::DropItemOnBeltTile {
            pos: TilePos::new(-1, 0),
            lane: 0,
            distance_numerator: 128,
            distance_denominator: 128,
            item: TEST_IRON_ORE,
        })
        .unwrap();
    tick_splitter_until_egress(&mut world);
    assert_eq!(production_splitter_next_output(&world), 1);
    assert_eq!(splitter_building_next_output(&world), 1);

    let restored = SimWorld::from_snapshot(catalog_for_tests(), world.snapshot()).unwrap();

    assert_eq!(splitter_building_next_output(&restored), 1);
    assert_eq!(production_splitter_next_output(&restored), 1);
}

#[test]
fn splitter_runtime_next_output_survives_output_remove_and_reconnect() {
    let mut world = connected_splitter_world_for_tests();

    world
        .apply_core_command_for_tests(SimCommand::DropItemOnBeltTile {
            pos: TilePos::new(-1, 0),
            lane: 0,
            distance_numerator: 128,
            distance_denominator: 128,
            item: TEST_IRON_ORE,
        })
        .unwrap();
    tick_splitter_until_egress(&mut world);
    assert_eq!(production_splitter_next_output(&world), 1);

    world
        .apply_core_command_for_tests(SimCommand::RemoveBuilding {
            pos: TilePos::new(1, 1),
        })
        .unwrap();
    assert!(
        world
            .transport
            .nodes_sorted()
            .any(|node| node.kind == TransportNodeKind::Splitter2x1)
    );

    place_catalog_belt(
        &mut world,
        "basic_belt",
        TilePos::new(1, 1),
        Direction::East,
    );

    assert_eq!(production_splitter_next_output(&world), 1);
}

#[test]
fn end_transfer_processing_follows_node_when_interaction_disagrees() {
    let mut world = SimWorld::default();
    let source_line = LineId(1);
    let target_line = LineId(2);
    let source_lanes = [
        PackedItemStream::from_gaps(
            vec![ItemKindId(3)],
            DistanceUnits::ZERO,
            vec![],
            DistanceUnits::new(128),
        ),
        PackedItemStream::default(),
    ];

    let mut transport = TransportStorage::default();
    transport.insert_line(TransportLine::new(
        source_line,
        GroupId(source_line.0),
        LinePath::new(vec![LineTile::new(TilePos::new(0, 0))]),
        UnitsPerTick::new(8),
        source_lanes,
        LineEndpoint::Blocked,
        LineEndpoint::Open,
    ));
    transport.insert_line(TransportLine::new(
        target_line,
        GroupId(target_line.0),
        LinePath::new(vec![LineTile::new(TilePos::new(1, 0))]),
        UnitsPerTick::new(8),
        [PackedItemStream::default(), PackedItemStream::default()],
        LineEndpoint::Blocked,
        LineEndpoint::Open,
    ));
    transport.insert_interaction(BeltInteraction::new(
        BeltInteractionKind::BlockedFront,
        source_line,
        None,
        None,
        TilePos::new(0, 0),
    ));
    transport.insert_node(TransportNode::end_transfer(
        TransportNodeId(1),
        TilePos::new(1, 0),
        source_line,
        target_line,
    ));

    world
        .topology_graph
        .set_belt(TilePos::new(0, 0), BeltTile::new(Direction::East));
    world
        .topology_graph
        .set_belt(TilePos::new(1, 0), BeltTile::new(Direction::East));
    world
        .activation
        .replace_active_lines([source_line, target_line]);
    world.transport = transport;

    tick_world_for_tests(&mut world);

    let target_items = world
        .visible_items_for_bounds(VisibleTileBounds::new(
            TilePos::new(1, 0),
            TilePos::new(1, 0),
        ))
        .collect::<Vec<_>>();

    assert!(target_items.iter().any(|item| item.item == ItemKindId(3)));
}

#[test]
fn side_load_processing_follows_node_when_interaction_disagrees() {
    let mut world = SimWorld::default();
    let source_line = LineId(1);
    let target_line = LineId(2);
    let source_tile = TilePos::new(-1, 0);
    let target_tile = TilePos::new(0, 0);
    let source_side = Direction::West;
    let target_direction = Direction::North;
    let near_lane = target_direction
        .near_lane_for_source_side(source_side)
        .expect("west input should side-load onto north belt");
    let source_lanes = [
        PackedItemStream::from_gaps(
            vec![TEST_IRON_ORE],
            DistanceUnits::ZERO,
            vec![],
            DistanceUnits::new(128),
        ),
        PackedItemStream::default(),
    ];

    let mut transport = TransportStorage::default();
    transport.insert_line(TransportLine::new(
        source_line,
        GroupId(source_line.0),
        LinePath::new(vec![LineTile::new(source_tile)]),
        UnitsPerTick::new(0),
        source_lanes,
        LineEndpoint::Blocked,
        LineEndpoint::Open,
    ));
    transport.insert_line(TransportLine::new(
        target_line,
        GroupId(target_line.0),
        LinePath::new(vec![LineTile::new(target_tile)]),
        UnitsPerTick::new(0),
        [PackedItemStream::default(), PackedItemStream::default()],
        LineEndpoint::Open,
        LineEndpoint::Open,
    ));
    transport.insert_interaction(BeltInteraction::new(
        BeltInteractionKind::BlockedFront,
        source_line,
        None,
        None,
        source_tile,
    ));
    transport.insert_node(TransportNode::side_load_to(
        TransportNodeId(1),
        source_tile,
        target_tile,
        source_line,
        target_line,
        near_lane,
    ));

    world
        .topology_graph
        .set_belt(source_tile, BeltTile::new(Direction::East));
    world
        .topology_graph
        .set_belt(target_tile, BeltTile::new(target_direction));
    world
        .activation
        .replace_active_lines([source_line, target_line]);
    world.transport = transport;

    tick_world_for_tests(&mut world);

    let target_items = world
        .visible_items_for_bounds(VisibleTileBounds::new(target_tile, target_tile))
        .collect::<Vec<_>>();

    assert!(
        target_items
            .iter()
            .any(|item| item.item == TEST_IRON_ORE && item.lane == near_lane)
    );
}

fn insert_test_interaction_with_node(
    transport: &mut TransportStorage,
    node_id: TransportNodeId,
    interaction: BeltInteraction,
) {
    let node = match interaction.kind() {
        BeltInteractionKind::BlockedFront => Some(TransportNode::blocked_front(
            node_id,
            interaction.target_sort_tile(),
            interaction.source_line(),
        )),
        BeltInteractionKind::EndTransfer => interaction.target_line().map(|target_line| {
            TransportNode::end_transfer(
                node_id,
                interaction.target_sort_tile(),
                interaction.source_line(),
                target_line,
            )
        }),
        BeltInteractionKind::SideLoad { near_lane } => {
            interaction.target_line().map(|target_line| {
                TransportNode::side_load_to(
                    node_id,
                    interaction.target_sort_tile(),
                    interaction
                        .target_tile()
                        .unwrap_or_else(|| interaction.target_sort_tile()),
                    interaction.source_line(),
                    target_line,
                    near_lane,
                )
            })
        }
    };

    transport.insert_interaction(interaction);
    if let Some(node) = node {
        transport.insert_node(node);
    }
}

fn world_with_splitter_for_tests() -> SimWorld {
    let mut world = SimWorld::default();
    let mut transport = TransportStorage::default();
    for (line_id, tile) in [
        (LineId(1), TilePos::new(-1, 0)),
        (LineId(2), TilePos::new(-1, 1)),
        (LineId(3), TilePos::new(1, 0)),
        (LineId(4), TilePos::new(1, 1)),
    ] {
        transport.insert_line(TransportLine::new(
            line_id,
            GroupId(line_id.0),
            LinePath::new(vec![LineTile::new(tile)]),
            UnitsPerTick::new(0),
            [PackedItemStream::default(), PackedItemStream::default()],
            LineEndpoint::Blocked,
            LineEndpoint::Open,
        ));
        world
            .topology_graph
            .set_belt(tile, BeltTile::new(Direction::East));
    }
    transport.insert_node(TransportNode::splitter_2x1(
        TransportNodeId(9),
        TilePos::new(0, 0),
        Direction::East,
        LineId(1),
        LineId(2),
        LineId(3),
        LineId(4),
    ));
    world
        .activation
        .replace_active_lines([LineId(1), LineId(2), LineId(3), LineId(4)]);
    world.transport = transport;
    world
}

fn connected_splitter_world_for_tests() -> SimWorld {
    connected_splitter_world_with_catalog_for_tests(catalog_for_tests())
}

fn connected_splitter_world_without_outputs_for_tests() -> SimWorld {
    let mut world = SimWorld::with_catalog(catalog_for_tests());
    place_catalog_belt(
        &mut world,
        "basic_belt",
        TilePos::new(-1, 0),
        Direction::East,
    );
    place_catalog_belt(
        &mut world,
        "basic_belt",
        TilePos::new(-1, 1),
        Direction::East,
    );
    world
        .apply_core_command_for_tests(SimCommand::PlaceBuilding {
            def_id: "basic_splitter".to_string(),
            origin: TilePos::new(0, 0),
            direction: Direction::East,
            inserter_drop_direction: None,
        })
        .unwrap();
    world.rebuild_transport_lines();
    world
}

fn connected_splitter_world_with_catalog_for_tests(catalog: CoreCatalog) -> SimWorld {
    let mut world = SimWorld::with_catalog(catalog);
    place_catalog_belt(
        &mut world,
        "basic_belt",
        TilePos::new(-1, 0),
        Direction::East,
    );
    place_catalog_belt(
        &mut world,
        "basic_belt",
        TilePos::new(-1, 1),
        Direction::East,
    );
    place_catalog_belt(
        &mut world,
        "basic_belt",
        TilePos::new(1, 0),
        Direction::East,
    );
    place_catalog_belt(
        &mut world,
        "basic_belt",
        TilePos::new(1, 1),
        Direction::East,
    );
    world
        .apply_core_command_for_tests(SimCommand::PlaceBuilding {
            def_id: "basic_splitter".to_string(),
            origin: TilePos::new(0, 0),
            direction: Direction::East,
            inserter_drop_direction: None,
        })
        .unwrap();
    world.rebuild_transport_lines();
    world
}

fn insert_splitter_source_item(world: &mut SimWorld, input: usize, item: ItemKindId) {
    insert_splitter_source_item_on_lane(world, input, 0, item);
}

fn insert_splitter_source_item_on_lane(
    world: &mut SimWorld,
    input: usize,
    lane: usize,
    item: ItemKindId,
) {
    let line = match input {
        0 => LineId(1),
        1 => LineId(2),
        other => panic!("unknown splitter input {other}"),
    };
    insert_line_item_at_distance(world, line, lane, item, DistanceUnits::ZERO);
}

fn insert_splitter_output_blocker(world: &mut SimWorld, output: usize) {
    insert_splitter_output_blocker_on_lane(world, output, 0);
}

fn insert_splitter_output_blocker_on_lane(world: &mut SimWorld, output: usize, lane: usize) {
    let line = match output {
        0 => LineId(3),
        1 => LineId(4),
        other => panic!("unknown splitter output {other}"),
    };
    insert_line_item_at_distance(world, line, lane, TEST_WOOD, DistanceUnits::new(127));
}

fn tick_splitter_until_egress(world: &mut SimWorld) {
    let ticks = (DistanceUnits::UNITS_PER_TILE / 4 + 2) as usize;
    for _ in 0..ticks {
        tick_world_for_tests(world);
    }
}

fn tick_splitter_until_center(world: &mut SimWorld) {
    let ticks = (DistanceUnits::UNITS_PER_TILE / 8 + 2) as usize;
    for _ in 0..ticks {
        tick_world_for_tests(world);
    }
}

fn insert_line_item_at_distance(
    world: &mut SimWorld,
    line: LineId,
    lane: usize,
    item: ItemKindId,
    distance: DistanceUnits,
) {
    assert!(
        world
            .transport
            .line_mut(line)
            .unwrap()
            .lane_mut(lane)
            .insert_one_at_distance_with_terminal_end(
                item,
                distance,
                Some(DistanceUnits::new(128))
            )
    );
}

fn remove_first_line_lane_item(world: &mut SimWorld, line: LineId, lane: usize) {
    let distance = world
        .transport
        .line(line)
        .unwrap()
        .lane(lane)
        .positions_in_range(DistanceUnits::ZERO, DistanceUnits::new(i32::MAX))
        .first()
        .unwrap()
        .distance;
    world
        .transport
        .line_mut(line)
        .unwrap()
        .lane_mut(lane)
        .remove_one_at_distance(distance)
        .unwrap();
}

fn line_lane_items(world: &SimWorld, line: LineId, lane: usize) -> Vec<ItemKindId> {
    world
        .transport
        .line(line)
        .unwrap()
        .lane(lane)
        .items()
        .to_vec()
}

fn total_splitter_items(world: &SimWorld) -> usize {
    [LineId(1), LineId(2), LineId(3), LineId(4)]
        .into_iter()
        .map(|line_id| total_items_on_line(world, line_id))
        .sum()
}

fn total_splitter_runtime_items(runtime: &SplitterRuntime) -> usize {
    runtime.ingress_items.len() + runtime.buffered_items.len() + runtime.egress_items.len()
}

fn world_with_underground_transport_runtime(
    distance: DistanceUnits,
    item: ItemKindId,
    lane: usize,
    progress: DistanceUnits,
) -> SimWorld {
    let mut world = SimWorld::with_catalog(catalog_for_tests());
    let mut node = TransportNode::underground(
        TransportNodeId(21),
        TilePos::new(0, 0),
        TilePos::new(4, 0),
        Direction::East,
        LineId(1),
        LineId(2),
        distance,
    );
    node.runtime = TransportNodeRuntime::Underground(UndergroundTransportRuntime {
        distance,
        items: vec![UndergroundTransportItem {
            item,
            lane,
            progress,
        }],
    });
    world.transport.insert_node(node);
    world
}

fn splitter_next_output(world: &SimWorld) -> usize {
    world
        .transport
        .nodes_sorted()
        .find(|node| node.id == TransportNodeId(9))
        .and_then(|node| match &node.runtime {
            TransportNodeRuntime::Splitter(runtime) => Some(runtime.next_output),
            TransportNodeRuntime::None | TransportNodeRuntime::Underground(_) => None,
        })
        .expect("splitter runtime")
}

fn splitter_runtime(world: &SimWorld) -> SplitterRuntime {
    world
        .transport
        .nodes_sorted()
        .find(|node| node.id == TransportNodeId(9))
        .and_then(|node| match &node.runtime {
            TransportNodeRuntime::Splitter(runtime) => Some(runtime.clone()),
            TransportNodeRuntime::None | TransportNodeRuntime::Underground(_) => None,
        })
        .expect("splitter runtime")
}

fn production_splitter_next_output(world: &SimWorld) -> usize {
    world
        .transport
        .nodes_sorted()
        .find(|node| node.kind == TransportNodeKind::Splitter2x1)
        .and_then(|node| match &node.runtime {
            TransportNodeRuntime::Splitter(runtime) => Some(runtime.next_output),
            TransportNodeRuntime::None | TransportNodeRuntime::Underground(_) => None,
        })
        .expect("production splitter runtime")
}

fn production_splitter_runtime(world: &SimWorld) -> SplitterRuntime {
    world
        .transport
        .nodes_sorted()
        .find(|node| node.kind == TransportNodeKind::Splitter2x1)
        .and_then(|node| match &node.runtime {
            TransportNodeRuntime::Splitter(runtime) => Some(runtime.clone()),
            TransportNodeRuntime::None | TransportNodeRuntime::Underground(_) => None,
        })
        .expect("production splitter runtime")
}

fn splitter_building_next_output(world: &SimWorld) -> usize {
    world
        .building_snapshots()
        .into_iter()
        .find(|building| building.def_id == "basic_splitter")
        .and_then(|building| match building.state {
            SimBuildingState::Splitter(runtime) => Some(runtime.next_output),
            _ => None,
        })
        .expect("splitter building runtime")
}

fn world_with_end_transfer_for_tests(
    source_lane: usize,
    target_lane_0: PackedItemStream,
    target_lane_1: PackedItemStream,
) -> SimWorld {
    let mut world = SimWorld::default();
    let source_line = LineId(1);
    let target_line = LineId(2);
    let mut source_lanes = [PackedItemStream::default(), PackedItemStream::default()];
    source_lanes[source_lane] = PackedItemStream::from_gaps(
        vec![ItemKindId(3)],
        DistanceUnits::ZERO,
        vec![],
        DistanceUnits::new(128),
    );

    let mut transport = TransportStorage::default();
    transport.insert_line(TransportLine::new(
        source_line,
        GroupId(source_line.0),
        LinePath::new(vec![LineTile::new(TilePos::new(0, 0))]),
        UnitsPerTick::new(8),
        source_lanes,
        LineEndpoint::Blocked,
        LineEndpoint::Open,
    ));
    transport.insert_line(TransportLine::new(
        target_line,
        GroupId(target_line.0),
        LinePath::new(vec![LineTile::new(TilePos::new(1, 0))]),
        UnitsPerTick::new(8),
        [target_lane_0, target_lane_1],
        LineEndpoint::Blocked,
        LineEndpoint::Open,
    ));
    insert_test_interaction_with_node(
        &mut transport,
        TransportNodeId(1),
        BeltInteraction::new(
            BeltInteractionKind::EndTransfer,
            source_line,
            Some(target_line),
            Some(TilePos::new(1, 0)),
            TilePos::new(1, 0),
        ),
    );

    world
        .topology_graph
        .set_belt(TilePos::new(0, 0), BeltTile::new(Direction::East));
    world
        .topology_graph
        .set_belt(TilePos::new(1, 0), BeltTile::new(Direction::East));
    world
        .activation
        .replace_active_lines([source_line, target_line]);
    world.transport = transport;
    world
}

#[test]
fn side_load_transfers_source_lane_zero_to_target_lane_zero() {
    let mut world = SimWorld::default();
    for (pos, direction) in [
        (TilePos::new(0, -1), Direction::North),
        (TilePos::new(0, 0), Direction::North),
        (TilePos::new(0, 1), Direction::North),
        (TilePos::new(-1, 0), Direction::East),
    ] {
        world
            .apply_core_command_for_tests(SimCommand::PlaceBelt {
                pos,
                direction,
                input_direction: direction,
                speed: UnitsPerTick::new(8),
            })
            .unwrap();
    }
    world
        .apply_core_command_for_tests(SimCommand::DropItemOnBeltTile {
            pos: TilePos::new(-1, 0),
            lane: 0,
            distance_numerator: 128,
            distance_denominator: 128,
            item: ItemKindId(3),
        })
        .unwrap();

    for _ in 0..28 {
        tick_world_for_tests(&mut world);
    }

    let visible = world
        .visible_items_for_bounds(VisibleTileBounds::new(
            TilePos::new(0, 0),
            TilePos::new(0, 1),
        ))
        .collect::<Vec<_>>();

    assert!(
        visible
            .iter()
            .any(|item| item.item == ItemKindId(3) && item.lane == 0)
    );
    assert!(
        !visible
            .iter()
            .any(|item| item.item == ItemKindId(3) && item.lane == 1)
    );
}

#[test]
fn side_load_transfers_both_source_lanes_to_target_side_lane() {
    let lane_zero = side_load_single_source_lane_result(
        Direction::North,
        Direction::West,
        0,
        [PackedItemStream::default(), PackedItemStream::default()],
    )
    .expect("source lane 0 should transfer to target side lane");
    let lane_one = side_load_single_source_lane_result(
        Direction::North,
        Direction::West,
        1,
        [PackedItemStream::default(), PackedItemStream::default()],
    )
    .expect("source lane 1 should transfer to target side lane");

    assert_eq!(lane_zero.lane, 0);
    assert_eq!(lane_one.lane, 0);
    assert_ne!(lane_zero.progress_numerator, lane_one.progress_numerator);
}

#[test]
fn side_load_does_not_starve_second_source_lane_when_source_is_full() {
    let mut world = SimWorld::default();
    let source_line = LineId(1);
    let target_line = LineId(2);
    let lane_zero_items = vec![ItemKindId(3); 32];
    let source_lanes = [
        PackedItemStream::from_gaps(
            lane_zero_items,
            DistanceUnits::ZERO,
            vec![DistanceUnits::ZERO; 31],
            DistanceUnits::new(128),
        ),
        PackedItemStream::from_gaps(
            vec![ItemKindId(4)],
            DistanceUnits::ZERO,
            vec![],
            DistanceUnits::new(128),
        ),
    ];

    let mut transport = TransportStorage::default();
    transport.insert_line(TransportLine::new(
        source_line,
        GroupId(source_line.0),
        LinePath::new(vec![LineTile::new(TilePos::new(-1, 0))]),
        UnitsPerTick::new(8),
        source_lanes,
        LineEndpoint::Blocked,
        LineEndpoint::Open,
    ));
    transport.insert_line(TransportLine::new(
        target_line,
        GroupId(target_line.0),
        LinePath::new(vec![
            LineTile::new(TilePos::new(0, 0)),
            LineTile::new(TilePos::new(0, 1)),
        ]),
        UnitsPerTick::new(8),
        [PackedItemStream::default(), PackedItemStream::default()],
        LineEndpoint::Open,
        LineEndpoint::Open,
    ));
    insert_test_interaction_with_node(
        &mut transport,
        TransportNodeId(1),
        BeltInteraction::new(
            BeltInteractionKind::SideLoad { near_lane: 0 },
            source_line,
            Some(target_line),
            Some(TilePos::new(0, 0)),
            TilePos::new(0, 0),
        ),
    );
    world
        .topology_graph
        .set_belt(TilePos::new(-1, 0), BeltTile::new(Direction::East));
    world
        .topology_graph
        .set_belt(TilePos::new(0, 0), BeltTile::new(Direction::North));
    world
        .topology_graph
        .set_belt(TilePos::new(0, 1), BeltTile::new(Direction::North));
    world
        .activation
        .replace_active_lines([source_line, target_line]);
    world.transport = transport;

    for _ in 0..80 {
        tick_world_for_tests(&mut world);
    }

    let source = world.transport.line(source_line).unwrap();
    assert!(
        source.lane(0).item_count() > 0,
        "lane 0 should still be saturated so this test catches starvation"
    );
    assert!(
        source.lane(1).is_empty(),
        "source lane 1 should transfer without waiting for lane 0 to empty"
    );
}

#[test]
fn t_junction_transfers_opposite_side_inputs_to_opposite_target_lanes() {
    let mut world = SimWorld::default();
    for (pos, direction) in [
        (TilePos::new(0, -1), Direction::North),
        (TilePos::new(0, 0), Direction::East),
        (TilePos::new(1, 0), Direction::East),
        (TilePos::new(0, 1), Direction::South),
    ] {
        world
            .apply_core_command_for_tests(SimCommand::PlaceBelt {
                pos,
                direction,
                input_direction: direction,
                speed: UnitsPerTick::new(8),
            })
            .unwrap();
    }
    world
        .apply_core_command_for_tests(SimCommand::DropItemOnBeltTile {
            pos: TilePos::new(0, -1),
            lane: 0,
            distance_numerator: 128,
            distance_denominator: 128,
            item: ItemKindId(3),
        })
        .unwrap();
    world
        .apply_core_command_for_tests(SimCommand::DropItemOnBeltTile {
            pos: TilePos::new(0, 1),
            lane: 0,
            distance_numerator: 128,
            distance_denominator: 128,
            item: ItemKindId(4),
        })
        .unwrap();

    tick_world_for_tests(&mut world);

    let visible = world
        .visible_items_for_bounds(VisibleTileBounds::new(
            TilePos::new(0, 0),
            TilePos::new(1, 0),
        ))
        .collect::<Vec<_>>();

    assert!(
        visible
            .iter()
            .any(|item| item.item == ItemKindId(3) && item.lane == 1),
        "bottom side input should transfer to the lower target lane: {visible:?}"
    );
    assert!(
        visible
            .iter()
            .any(|item| item.item == ItemKindId(4) && item.lane == 0),
        "top side input should transfer to the upper target lane: {visible:?}"
    );
}

#[test]
fn side_load_uses_matching_target_lane_without_spilling_to_far_lane() {
    let mut world = SimWorld::default();
    let source_line = LineId(1);
    let target_line = LineId(2);
    let mut source_lanes = [PackedItemStream::default(), PackedItemStream::default()];
    source_lanes[0] = PackedItemStream::from_gaps(
        vec![ItemKindId(3)],
        DistanceUnits::ZERO,
        vec![],
        DistanceUnits::new(128),
    );
    let target_lanes = [
        PackedItemStream::from_gaps(
            vec![ItemKindId(4), ItemKindId(4)],
            DistanceUnits::new(359),
            vec![DistanceUnits::new(64)],
            DistanceUnits::new(345),
        ),
        PackedItemStream::default(),
    ];

    let mut transport = TransportStorage::default();
    transport.insert_line(TransportLine::new(
        source_line,
        GroupId(source_line.0),
        LinePath::new(vec![LineTile::new(TilePos::new(-1, 0))]),
        UnitsPerTick::new(8),
        source_lanes,
        LineEndpoint::Blocked,
        LineEndpoint::Open,
    ));
    transport.insert_line(TransportLine::new(
        target_line,
        GroupId(target_line.0),
        LinePath::new(vec![
            LineTile::new(TilePos::new(0, -1)),
            LineTile::new(TilePos::new(0, 0)),
            LineTile::new(TilePos::new(0, 1)),
        ]),
        UnitsPerTick::new(8),
        target_lanes,
        LineEndpoint::Open,
        LineEndpoint::Open,
    ));
    insert_test_interaction_with_node(
        &mut transport,
        TransportNodeId(1),
        BeltInteraction::new(
            BeltInteractionKind::SideLoad { near_lane: 0 },
            source_line,
            Some(target_line),
            Some(TilePos::new(0, 0)),
            TilePos::new(0, 0),
        ),
    );

    world
        .topology_graph
        .set_belt(TilePos::new(-1, 0), BeltTile::new(Direction::East));
    for pos in [TilePos::new(0, -1), TilePos::new(0, 0), TilePos::new(0, 1)] {
        world
            .topology_graph
            .set_belt(pos, BeltTile::new(Direction::North));
    }
    world
        .activation
        .replace_active_lines([source_line, target_line]);
    world.transport = transport;

    tick_world_for_tests(&mut world);

    let visible_after_first_tick = world
        .visible_items_for_bounds(VisibleTileBounds::new(
            TilePos::new(0, 0),
            TilePos::new(0, 1),
        ))
        .collect::<Vec<_>>();
    assert!(
        visible_after_first_tick
            .iter()
            .any(|item| item.item == ItemKindId(3) && item.lane == 0),
        "side-load should use free space on the matching target lane: {visible_after_first_tick:?}"
    );
    assert!(
        !visible_after_first_tick
            .iter()
            .any(|item| item.item == ItemKindId(3) && item.lane == 1),
        "side-load must not spill to the far target lane: {visible_after_first_tick:?}"
    );

    for _ in 0..16 {
        tick_world_for_tests(&mut world);
    }

    let visible = world
        .visible_items_for_bounds(VisibleTileBounds::new(
            TilePos::new(0, 0),
            TilePos::new(0, 1),
        ))
        .collect::<Vec<_>>();
    assert!(
        visible
            .iter()
            .any(|item| item.item == ItemKindId(3) && item.lane == 0)
    );
}

#[test]
fn side_load_right_source_lane_waits_when_right_half_is_occupied() {
    let mut world = SimWorld::default();
    let source_line = LineId(1);
    let target_line = LineId(2);
    let source_lanes = [
        PackedItemStream::default(),
        PackedItemStream::from_gaps(
            vec![ItemKindId(3)],
            DistanceUnits::ZERO,
            vec![],
            DistanceUnits::new(128),
        ),
    ];
    let target_lanes = [
        PackedItemStream::default(),
        PackedItemStream::from_gaps(
            vec![ItemKindId(4), ItemKindId(6)],
            DistanceUnits::new(32),
            vec![MIN_ITEM_SPACING],
            DistanceUnits::new(160),
        ),
    ];

    let mut transport = TransportStorage::default();
    transport.insert_line(TransportLine::new(
        source_line,
        GroupId(source_line.0),
        LinePath::new(vec![LineTile::new(TilePos::new(0, -1))]),
        UnitsPerTick::new(0),
        source_lanes,
        LineEndpoint::Blocked,
        LineEndpoint::Open,
    ));
    transport.insert_line(TransportLine::new(
        target_line,
        GroupId(target_line.0),
        LinePath::new(vec![LineTile::new(TilePos::new(0, 0))]),
        UnitsPerTick::new(0),
        target_lanes,
        LineEndpoint::Open,
        LineEndpoint::Open,
    ));
    insert_test_interaction_with_node(
        &mut transport,
        TransportNodeId(1),
        BeltInteraction::new(
            BeltInteractionKind::SideLoad { near_lane: 1 },
            source_line,
            Some(target_line),
            Some(TilePos::new(0, 0)),
            TilePos::new(0, -1),
        ),
    );

    world
        .topology_graph
        .set_belt(TilePos::new(0, -1), BeltTile::new(Direction::North));
    world
        .topology_graph
        .set_belt(TilePos::new(0, 0), BeltTile::new(Direction::East));
    world
        .activation
        .replace_active_lines([source_line, target_line]);
    world.transport = transport;

    tick_world_for_tests(&mut world);

    let visible = world
        .visible_items_for_bounds(VisibleTileBounds::new(
            TilePos::new(0, -1),
            TilePos::new(0, 0),
        ))
        .collect::<Vec<_>>();
    assert!(
        visible
            .iter()
            .any(|item| item.item == ItemKindId(3) && item.tile == TilePos::new(0, -1)),
        "right source lane should wait when its right half is occupied: {visible:?}"
    );
    assert!(
        !visible
            .iter()
            .any(|item| item.item == ItemKindId(3) && item.tile == TilePos::new(0, 0)),
        "right source lane must not fill the opposite half: {visible:?}"
    );
}

#[test]
fn side_load_lower_left_fills_free_half_while_lower_right_waits_when_right_half_is_full() {
    let mut world = SimWorld::default();
    let source_line = LineId(1);
    let target_line = LineId(2);
    let source_lanes = [
        PackedItemStream::from_gaps(
            vec![ItemKindId(3)],
            DistanceUnits::ZERO,
            vec![],
            DistanceUnits::new(128),
        ),
        PackedItemStream::from_gaps(
            vec![ItemKindId(5)],
            DistanceUnits::ZERO,
            vec![],
            DistanceUnits::new(128),
        ),
    ];
    let target_lanes = [
        PackedItemStream::from_gaps(
            vec![ItemKindId(4)],
            DistanceUnits::new(32),
            vec![],
            DistanceUnits::new(96),
        ),
        PackedItemStream::from_gaps(
            vec![ItemKindId(6), ItemKindId(7)],
            DistanceUnits::new(32),
            vec![MIN_ITEM_SPACING],
            DistanceUnits::new(160),
        ),
    ];

    let mut transport = TransportStorage::default();
    transport.insert_line(TransportLine::new(
        source_line,
        GroupId(source_line.0),
        LinePath::new(vec![LineTile::new(TilePos::new(0, -1))]),
        UnitsPerTick::new(0),
        source_lanes,
        LineEndpoint::Blocked,
        LineEndpoint::Open,
    ));
    transport.insert_line(TransportLine::new(
        target_line,
        GroupId(target_line.0),
        LinePath::new(vec![LineTile::new(TilePos::new(0, 0))]),
        UnitsPerTick::new(0),
        target_lanes,
        LineEndpoint::Open,
        LineEndpoint::Open,
    ));
    insert_test_interaction_with_node(
        &mut transport,
        TransportNodeId(1),
        BeltInteraction::new(
            BeltInteractionKind::SideLoad { near_lane: 1 },
            source_line,
            Some(target_line),
            Some(TilePos::new(0, 0)),
            TilePos::new(0, -1),
        ),
    );

    world
        .topology_graph
        .set_belt(TilePos::new(0, -1), BeltTile::new(Direction::North));
    world
        .topology_graph
        .set_belt(TilePos::new(0, 0), BeltTile::new(Direction::East));
    world
        .activation
        .replace_active_lines([source_line, target_line]);
    world.transport = transport;

    tick_world_for_tests(&mut world);

    let visible = world
        .visible_items_for_bounds(VisibleTileBounds::new(
            TilePos::new(0, -1),
            TilePos::new(0, 0),
        ))
        .collect::<Vec<_>>();
    let target_lower_lane_items = visible
        .iter()
        .filter(|item| item.tile == TilePos::new(0, 0) && item.lane == 1)
        .collect::<Vec<_>>();
    assert!(
        target_lower_lane_items
            .iter()
            .any(|item| item.item == ItemKindId(6)),
        "pre-filled T target item should remain on the lower/right target lane: {visible:?}"
    );
    assert!(
        target_lower_lane_items
            .iter()
            .any(|item| item.item == ItemKindId(3)
                && item.direction == Direction::East
                && item.progress_numerator <= 64),
        "lower-left source lane should fill the free left half of the partially occupied T lane: {visible:?}"
    );
    assert!(
        visible
            .iter()
            .any(|item| item.item == ItemKindId(5) && item.tile == TilePos::new(0, -1)),
        "lower-right source lane should wait because its right-side slot is already full: {visible:?}"
    );
    assert!(
        !visible
            .iter()
            .any(|item| item.item == ItemKindId(5) && item.tile == TilePos::new(0, 0)),
        "lower-right source lane must not fill the opposite free half: {visible:?}"
    );
}

#[test]
fn side_load_lane_halves_work_from_every_t_junction_side() {
    let cases = [
        (Direction::North, Direction::West),
        (Direction::North, Direction::East),
        (Direction::East, Direction::North),
        (Direction::East, Direction::South),
        (Direction::South, Direction::East),
        (Direction::South, Direction::West),
        (Direction::West, Direction::South),
        (Direction::West, Direction::North),
    ];

    for (target_direction, source_side) in cases {
        let source_lane_zero = side_load_single_source_lane_result(
            target_direction,
            source_side,
            0,
            [PackedItemStream::default(), PackedItemStream::default()],
        )
        .unwrap_or_else(|| {
            panic!(
                "source lane 0 should transfer for target {target_direction:?} from {source_side:?}"
            )
        });
        let source_lane_one = side_load_single_source_lane_result(
            target_direction,
            source_side,
            1,
            [PackedItemStream::default(), PackedItemStream::default()],
        )
        .unwrap_or_else(|| {
            panic!(
                "source lane 1 should transfer for target {target_direction:?} from {source_side:?}"
            )
        });

        assert_ne!(
            source_lane_zero.progress_numerator, source_lane_one.progress_numerator,
            "source lanes should land on different halves for target {target_direction:?} from {source_side:?}"
        );
    }
}

#[test]
fn side_load_each_t_junction_side_fills_only_its_free_half() {
    let cases = [
        (Direction::North, Direction::West),
        (Direction::North, Direction::East),
        (Direction::East, Direction::North),
        (Direction::East, Direction::South),
        (Direction::South, Direction::East),
        (Direction::South, Direction::West),
        (Direction::West, Direction::South),
        (Direction::West, Direction::North),
    ];

    for (target_direction, source_side) in cases {
        let near_lane = target_direction
            .near_lane_for_source_side(source_side)
            .unwrap();
        for source_lane in [0, 1] {
            let uses_exit_half = side_load_source_lane_uses_exit_half(
                Some(target_direction),
                Some(source_side),
                source_lane,
            );
            let opposite_half_occupied =
                side_load_target_lanes_with_occupied_half(near_lane, !uses_exit_half);
            let own_half_occupied =
                side_load_target_lanes_with_occupied_half(near_lane, uses_exit_half);

            let transferred =
                side_load_single_source_lane_result(
                    target_direction,
                    source_side,
                    source_lane,
                    opposite_half_occupied,
                )
                    .unwrap_or_else(|| {
                        panic!(
                            "source lane {source_lane} should fill its free half for target {target_direction:?} from {source_side:?}"
                        )
                    });
            if uses_exit_half {
                assert!(
                    transferred.progress_numerator >= 64,
                    "source lane {source_lane} should land on the exit half for target {target_direction:?} from {source_side:?}: {transferred:?}"
                );
            } else {
                assert!(
                    transferred.progress_numerator <= 64,
                    "source lane {source_lane} should land on the entry half for target {target_direction:?} from {source_side:?}: {transferred:?}"
                );
            }

            assert!(
                side_load_single_source_lane_result(
                    target_direction,
                    source_side,
                    source_lane,
                    own_half_occupied,
                )
                .is_none(),
                "source lane {source_lane} should wait when only the opposite half is free for target {target_direction:?} from {source_side:?}"
            );
        }
    }
}

fn side_load_single_source_lane_result(
    target_direction: Direction,
    source_side: Direction,
    source_lane: usize,
    target_lanes: [PackedItemStream; 2],
) -> Option<VisibleItem> {
    let mut world = SimWorld::default();
    let source_line = LineId(1);
    let target_line = LineId(2);
    let target_tile = TilePos::new(0, 0);
    let (source_x, source_y) = source_side.delta();
    let source_tile = TilePos::new(source_x, source_y);
    let near_lane = target_direction.near_lane_for_source_side(source_side)?;
    let mut source_lanes = [PackedItemStream::default(), PackedItemStream::default()];
    source_lanes[source_lane] = PackedItemStream::from_gaps(
        vec![ItemKindId(3)],
        DistanceUnits::ZERO,
        vec![],
        DistanceUnits::new(128),
    );

    let mut transport = TransportStorage::default();
    transport.insert_line(TransportLine::new(
        source_line,
        GroupId(source_line.0),
        LinePath::new(vec![LineTile::new(source_tile)]),
        UnitsPerTick::new(0),
        source_lanes,
        LineEndpoint::Blocked,
        LineEndpoint::Open,
    ));
    transport.insert_line(TransportLine::new(
        target_line,
        GroupId(target_line.0),
        LinePath::new(vec![LineTile::new(target_tile)]),
        UnitsPerTick::new(0),
        target_lanes,
        LineEndpoint::Open,
        LineEndpoint::Open,
    ));
    insert_test_interaction_with_node(
        &mut transport,
        TransportNodeId(1),
        BeltInteraction::new(
            BeltInteractionKind::SideLoad { near_lane },
            source_line,
            Some(target_line),
            Some(target_tile),
            source_tile,
        ),
    );

    world
        .topology_graph
        .set_belt(source_tile, BeltTile::new(source_side.opposite()));
    world
        .topology_graph
        .set_belt(target_tile, BeltTile::new(target_direction));
    world
        .activation
        .replace_active_lines([source_line, target_line]);
    world.transport = transport;

    tick_world_for_tests(&mut world);

    let min = TilePos::new(source_tile.x.min(0), source_tile.y.min(0));
    let max = TilePos::new(source_tile.x.max(0), source_tile.y.max(0));
    world
        .visible_items_for_bounds(VisibleTileBounds::new(min, max))
        .find(|item| item.item == ItemKindId(3) && item.tile == target_tile)
}

fn side_load_target_lanes_with_occupied_half(
    near_lane: usize,
    right_half: bool,
) -> [PackedItemStream; 2] {
    let start = if right_half {
        DistanceUnits::new(32)
    } else {
        DistanceUnits::new(160)
    };
    let back = if right_half {
        DistanceUnits::new(160)
    } else {
        DistanceUnits::new(32)
    };
    let mut lanes = [PackedItemStream::default(), PackedItemStream::default()];
    lanes[near_lane] = PackedItemStream::from_gaps(
        vec![ItemKindId(4), ItemKindId(6)],
        start,
        vec![MIN_ITEM_SPACING],
        back,
    );
    lanes
}

#[test]
fn multi_turn_path_keeps_items_flowing_through_each_corner() {
    let mut world = SimWorld::default();
    for (pos, belt) in multi_turn_path_for_tests() {
        world
            .apply_core_command_for_tests(SimCommand::PlaceBelt {
                pos,
                direction: belt.direction,
                input_direction: belt.input_direction,
                speed: UnitsPerTick::new(8),
            })
            .unwrap();
    }

    world
        .apply_core_command_for_tests(SimCommand::DropItemOnBeltTile {
            pos: TilePos::new(0, 0),
            lane: 0,
            distance_numerator: 64,
            distance_denominator: 128,
            item: ItemKindId(3),
        })
        .unwrap();
    let mut visited_tiles = BTreeSet::new();
    for _ in 0..220 {
        tick_world_for_tests(&mut world);
        for item in world.visible_items_for_bounds(VisibleTileBounds::new(
            TilePos::new(0, -2),
            TilePos::new(2, 0),
        )) {
            visited_tiles.insert(item.tile);
        }
    }

    for (pos, _) in multi_turn_path_for_tests() {
        assert!(
            visited_tiles.contains(&pos),
            "item never visited multi-turn tile {pos:?}; visited {visited_tiles:?}"
        );
    }
}

#[test]
fn rectangular_cycle_keeps_item_moving_around_closed_loop() {
    let mut world = SimWorld::default();
    for (pos, belt) in rectangular_cycle_for_tests() {
        world
            .apply_core_command_for_tests(SimCommand::PlaceBelt {
                pos,
                direction: belt.direction,
                input_direction: belt.input_direction,
                speed: UnitsPerTick::new(8),
            })
            .unwrap();
    }

    world
        .apply_core_command_for_tests(SimCommand::DropItemOnBeltTile {
            pos: TilePos::new(0, 0),
            lane: 0,
            distance_numerator: 64,
            distance_denominator: 128,
            item: ItemKindId(3),
        })
        .unwrap();

    let mut visited_tiles = BTreeSet::new();
    for _ in 0..360 {
        tick_world_for_tests(&mut world);
        for item in world.visible_items_for_bounds(VisibleTileBounds::new(
            TilePos::new(0, -2),
            TilePos::new(2, 0),
        )) {
            visited_tiles.insert(item.tile);
        }
    }

    for (pos, _) in rectangular_cycle_for_tests() {
        assert!(
            visited_tiles.contains(&pos),
            "item never visited closed loop tile {pos:?}; visited {visited_tiles:?}"
        );
    }
}

#[test]
fn restoring_corner_belt_preserves_survivors_and_allows_refill() {
    let mut world = SimWorld::default();
    for (pos, direction, input_direction) in [
        (TilePos::new(0, 0), Direction::East, Direction::East),
        (TilePos::new(1, 0), Direction::East, Direction::East),
        (TilePos::new(2, 0), Direction::South, Direction::East),
        (TilePos::new(2, -1), Direction::South, Direction::South),
    ] {
        world
            .apply_core_command_for_tests(SimCommand::PlaceBelt {
                pos,
                direction,
                input_direction,
                speed: UnitsPerTick::new(8),
            })
            .unwrap();
    }
    for _ in 0..8 {
        world
            .apply_core_command_for_tests(SimCommand::DropItemOnBeltTile {
                pos: TilePos::new(1, 0),
                lane: 0,
                distance_numerator: 64,
                distance_denominator: 128,
                item: ItemKindId(3),
            })
            .unwrap();
    }

    world
        .apply_core_command_for_tests(SimCommand::RemoveBuilding {
            pos: TilePos::new(2, 0),
        })
        .unwrap();
    world
        .apply_core_command_for_tests(SimCommand::PlaceBelt {
            pos: TilePos::new(2, 0),
            direction: Direction::South,
            input_direction: Direction::East,
            speed: UnitsPerTick::new(8),
        })
        .unwrap();

    for _ in 0..60 {
        tick_world_for_tests(&mut world);
    }

    let survivor_count = world
        .visible_items_for_bounds(VisibleTileBounds::new(
            TilePos::new(0, -1),
            TilePos::new(2, 0),
        ))
        .count();
    assert!(survivor_count > 0);
    assert!(survivor_count < 16);

    let mut inserted_count = 0;
    loop {
        match world.apply_core_command_for_tests(SimCommand::DropItemOnBeltTile {
            pos: TilePos::new(1, 0),
            lane: 0,
            distance_numerator: 64,
            distance_denominator: 128,
            item: ItemKindId(3),
        }) {
            Ok(()) => inserted_count += 1,
            Err(SimCommandError::CapacityExceeded) => break,
            Err(error) => {
                panic!("unexpected drop error while refilling restored corner: {error:?}")
            }
        }
    }

    assert_eq!(survivor_count + inserted_count, 16);
    assert_eq!(
        world
            .visible_items_for_bounds(VisibleTileBounds::new(
                TilePos::new(0, -1),
                TilePos::new(2, 0),
            ))
            .count(),
        16
    );
}

#[test]
fn restored_split_belt_line_can_be_refilled_after_items_shift_to_end() {
    let mut world = SimWorld::default();
    world
        .build_straight_belt_line_for_tests(
            TilePos::new(0, 0),
            4,
            Direction::East,
            UnitsPerTick::new(8),
        )
        .unwrap();

    for _ in 0..16 {
        world
            .apply_core_command_for_tests(SimCommand::DropItemOnBeltTile {
                pos: TilePos::new(1, 0),
                lane: 0,
                distance_numerator: 64,
                distance_denominator: 128,
                item: ItemKindId(3),
            })
            .unwrap();
    }
    world
        .apply_core_command_for_tests(SimCommand::RemoveBuilding {
            pos: TilePos::new(1, 0),
        })
        .unwrap();
    world
        .apply_core_command_for_tests(SimCommand::PlaceBelt {
            pos: TilePos::new(1, 0),
            direction: Direction::East,
            input_direction: Direction::East,
            speed: UnitsPerTick::new(8),
        })
        .unwrap();
    for _ in 0..80 {
        tick_world_for_tests(&mut world);
    }
    for _ in 0..4 {
        world
            .apply_core_command_for_tests(SimCommand::DropItemOnBeltTile {
                pos: TilePos::new(1, 0),
                lane: 0,
                distance_numerator: 64,
                distance_denominator: 128,
                item: ItemKindId(3),
            })
            .unwrap();
    }
    let error = world
        .apply_core_command_for_tests(SimCommand::DropItemOnBeltTile {
            pos: TilePos::new(1, 0),
            lane: 0,
            distance_numerator: 64,
            distance_denominator: 128,
            item: ItemKindId(3),
        })
        .unwrap_err();

    assert_eq!(error, SimCommandError::CapacityExceeded);
    let visible = world
        .visible_items_for_bounds(VisibleTileBounds::new(
            TilePos::new(0, 0),
            TilePos::new(3, 0),
        ))
        .collect::<Vec<_>>();
    assert_eq!(visible.len(), 16);
    for tile_x in 0..4 {
        let mut progress = visible
            .iter()
            .filter(|item| item.tile == TilePos::new(tile_x, 0) && item.lane == 0)
            .map(|item| item.progress_numerator)
            .collect::<Vec<_>>();
        progress.sort_unstable();
        assert_eq!(progress, vec![32, 64, 96, 128]);
    }
}

fn multi_turn_path_for_tests() -> [(TilePos, BeltTile); 7] {
    [
        (TilePos::new(0, 0), BeltTile::new(Direction::East)),
        (TilePos::new(1, 0), BeltTile::new(Direction::East)),
        (
            TilePos::new(2, 0),
            BeltTile::turn(Direction::East, Direction::South),
        ),
        (TilePos::new(2, -1), BeltTile::new(Direction::South)),
        (
            TilePos::new(2, -2),
            BeltTile::turn(Direction::South, Direction::West),
        ),
        (TilePos::new(1, -2), BeltTile::new(Direction::West)),
        (
            TilePos::new(0, -2),
            BeltTile::turn(Direction::West, Direction::North),
        ),
    ]
}

fn rectangular_cycle_for_tests() -> [(TilePos, BeltTile); 8] {
    [
        (
            TilePos::new(0, 0),
            BeltTile::turn(Direction::North, Direction::East),
        ),
        (TilePos::new(1, 0), BeltTile::new(Direction::East)),
        (
            TilePos::new(2, 0),
            BeltTile::turn(Direction::East, Direction::South),
        ),
        (TilePos::new(2, -1), BeltTile::new(Direction::South)),
        (
            TilePos::new(2, -2),
            BeltTile::turn(Direction::South, Direction::West),
        ),
        (TilePos::new(1, -2), BeltTile::new(Direction::West)),
        (
            TilePos::new(0, -2),
            BeltTile::turn(Direction::West, Direction::North),
        ),
        (TilePos::new(0, -1), BeltTile::new(Direction::North)),
    ]
}

#[test]
fn drop_item_on_non_belt_tile_rejects_without_items() {
    let mut world = SimWorld::default();

    let error = world
        .apply_core_command_for_tests(SimCommand::DropItemOnBeltTile {
            pos: TilePos::new(5, 5),
            lane: 0,
            distance_numerator: 0,
            distance_denominator: 128,
            item: ItemKindId(3),
        })
        .unwrap_err();

    assert_eq!(
        error,
        SimCommandError::MissingBuilding {
            pos: TilePos::new(5, 5)
        }
    );
    assert_eq!(
        world
            .visible_items_for_bounds(VisibleTileBounds::new(
                TilePos::new(-10, -10),
                TilePos::new(10, 10),
            ))
            .count(),
        0
    );
}

#[test]
fn mixed_speed_contiguous_line_builds_transfer_boundary() {
    let mut world = SimWorld::default();
    world
        .apply_core_command_for_tests(SimCommand::PlaceBelt {
            pos: TilePos::new(0, 0),
            direction: Direction::East,
            input_direction: Direction::East,
            speed: UnitsPerTick::new(8),
        })
        .unwrap();

    world
        .apply_core_command_for_tests(SimCommand::PlaceBelt {
            pos: TilePos::new(1, 0),
            direction: Direction::East,
            input_direction: Direction::East,
            speed: UnitsPerTick::new(16),
        })
        .unwrap();

    assert_eq!(world.transport.line_ids_sorted().count(), 2);
    world
        .apply_core_command_for_tests(SimCommand::DropItemOnBeltTile {
            pos: TilePos::new(0, 0),
            lane: 0,
            distance_numerator: 128,
            distance_denominator: 128,
            item: TEST_IRON_ORE,
        })
        .unwrap();

    for _ in 0..80 {
        tick_world_for_tests(&mut world);
    }

    let visible = world
        .visible_items_for_bounds(VisibleTileBounds::new(
            TilePos::new(0, 0),
            TilePos::new(1, 0),
        ))
        .collect::<Vec<_>>();
    assert_eq!(visible.len(), 1, "{visible:?}");
    assert_eq!(visible[0].tile, TilePos::new(1, 0));
}

#[test]
fn mixed_speed_end_transfer_inserts_at_entry_boundary_for_smooth_visuals() {
    let mut world = SimWorld::default();
    world
        .apply_core_command_for_tests(SimCommand::PlaceBelt {
            pos: TilePos::new(0, 0),
            direction: Direction::East,
            input_direction: Direction::East,
            speed: UnitsPerTick::new(4),
        })
        .unwrap();
    world
        .apply_core_command_for_tests(SimCommand::PlaceBelt {
            pos: TilePos::new(1, 0),
            direction: Direction::East,
            input_direction: Direction::East,
            speed: UnitsPerTick::new(8),
        })
        .unwrap();
    world
        .apply_core_command_for_tests(SimCommand::DropItemOnBeltTile {
            pos: TilePos::new(0, 0),
            lane: 0,
            distance_numerator: 128,
            distance_denominator: 128,
            item: TEST_IRON_ORE,
        })
        .unwrap();

    tick_world_for_tests(&mut world);

    let visible = world
        .visible_items_for_bounds(VisibleTileBounds::new(
            TilePos::new(0, 0),
            TilePos::new(1, 0),
        ))
        .collect::<Vec<_>>();
    assert_eq!(visible.len(), 1, "{visible:?}");
    assert_eq!(visible[0].tile, TilePos::new(1, 0));
    assert_eq!(
        visible[0].progress_numerator, 0,
        "transfer should appear on the target entry edge instead of jumping inward"
    );
}

#[test]
fn mixed_speed_turn_boundary_keeps_turn_entry_direction() {
    let mut world = SimWorld::with_catalog(catalog_for_tests());
    place_catalog_belt(
        &mut world,
        "basic_belt",
        TilePos::new(0, 0),
        Direction::East,
    );
    place_catalog_belt(
        &mut world,
        "fast_belt",
        TilePos::new(1, 0),
        Direction::South,
    );
    place_catalog_belt(
        &mut world,
        "fast_belt",
        TilePos::new(1, -1),
        Direction::South,
    );

    let turn_belt = world.topology_graph.belt(TilePos::new(1, 0)).unwrap();
    assert_eq!(turn_belt.input_direction, Direction::East);
    assert_eq!(turn_belt.direction, Direction::South);

    world
        .apply_core_command_for_tests(SimCommand::DropItemOnBeltTile {
            pos: TilePos::new(0, 0),
            lane: 0,
            distance_numerator: 128,
            distance_denominator: 128,
            item: TEST_IRON_ORE,
        })
        .unwrap();

    tick_world_for_tests(&mut world);

    let visible = world
        .visible_items_for_bounds(VisibleTileBounds::new(
            TilePos::new(1, 0),
            TilePos::new(1, 0),
        ))
        .collect::<Vec<_>>();
    assert_eq!(visible.len(), 1, "{visible:?}");
    assert_eq!(visible[0].tile, TilePos::new(1, 0));
    assert_eq!(visible[0].direction, Direction::South);
    assert_eq!(
        visible[0].entry_direction,
        Direction::East,
        "mixed-speed boundary into a turn should render as a turn, not as a side-load"
    );
    assert_eq!(visible[0].progress_numerator, 0);
}

#[test]
fn digest_differs_for_same_counts_on_different_paths() {
    let mut left = SimWorld::default();
    let mut right = SimWorld::default();

    left.apply_core_command_for_tests(SimCommand::PlaceBelt {
        pos: TilePos::new(0, 0),
        direction: Direction::East,
        input_direction: Direction::East,
        speed: UnitsPerTick::new(8),
    })
    .unwrap();
    right
        .apply_core_command_for_tests(SimCommand::PlaceBelt {
            pos: TilePos::new(5, 0),
            direction: Direction::East,
            input_direction: Direction::East,
            speed: UnitsPerTick::new(8),
        })
        .unwrap();

    assert_ne!(left.digest(), right.digest());
}

#[test]
fn digest_differs_for_same_item_count_with_different_lane_gaps() {
    let mut left = SimWorld::default();
    let mut right = SimWorld::default();

    for world in [&mut left, &mut right] {
        world
            .apply_core_command_for_tests(SimCommand::PlaceBelt {
                pos: TilePos::new(0, 0),
                direction: Direction::East,
                input_direction: Direction::East,
                speed: UnitsPerTick::new(8),
            })
            .unwrap();
    }

    left.apply_core_command_for_tests(SimCommand::InsertItemAtLineStart {
        line_index: 0,
        lane: 0,
        item: ItemKindId(1),
    })
    .unwrap();
    tick_world_for_tests(&mut left);

    tick_world_for_tests(&mut right);
    right
        .apply_core_command_for_tests(SimCommand::InsertItemAtLineStart {
            line_index: 0,
            lane: 0,
            item: ItemKindId(1),
        })
        .unwrap();

    assert_eq!(left.tick.raw(), right.tick.raw());
    assert_ne!(left.digest(), right.digest());
}

#[test]
fn interaction_order_is_deterministic_for_simultaneous_side_loads() {
    fn build_world() -> SimWorld {
        let mut world = SimWorld::default();
        for (pos, direction) in [
            (TilePos::new(0, -1), Direction::North),
            (TilePos::new(0, 0), Direction::North),
            (TilePos::new(0, 1), Direction::North),
            (TilePos::new(-1, 0), Direction::East),
            (TilePos::new(1, 0), Direction::West),
        ] {
            world
                .apply_core_command_for_tests(SimCommand::PlaceBelt {
                    pos,
                    direction,
                    input_direction: direction,
                    speed: UnitsPerTick::new(8),
                })
                .unwrap();
        }
        world
            .apply_core_command_for_tests(SimCommand::DropItemOnBeltTile {
                pos: TilePos::new(-1, 0),
                lane: 0,
                distance_numerator: 128,
                distance_denominator: 128,
                item: ItemKindId(3),
            })
            .unwrap();
        world
            .apply_core_command_for_tests(SimCommand::DropItemOnBeltTile {
                pos: TilePos::new(1, 0),
                lane: 0,
                distance_numerator: 128,
                distance_denominator: 128,
                item: ItemKindId(4),
            })
            .unwrap();
        world
    }

    let mut left = build_world();
    let mut right = build_world();

    for _ in 0..80 {
        tick_world_for_tests(&mut left);
        tick_world_for_tests(&mut right);
    }

    assert_eq!(left.digest(), right.digest());
}

#[test]
fn digest_includes_belt_interaction_fields() {
    fn build_world() -> (SimWorld, Vec<LineId>) {
        let mut world = SimWorld::default();
        for pos in [TilePos::new(0, 0), TilePos::new(10, 0)] {
            world
                .apply_core_command_for_tests(SimCommand::PlaceBelt {
                    pos,
                    direction: Direction::East,
                    input_direction: Direction::East,
                    speed: UnitsPerTick::new(8),
                })
                .unwrap();
        }

        let lines = world.transport.line_ids_sorted().collect::<Vec<_>>();
        assert_eq!(lines.len(), 2);
        (world, lines)
    }

    fn assert_digest_differs_for_interaction_delta(
        left: impl FnOnce(&[LineId]) -> BeltInteraction,
        right: impl FnOnce(&[LineId]) -> BeltInteraction,
    ) {
        let (mut left_world, left_lines) = build_world();
        let (mut right_world, right_lines) = build_world();

        assert_eq!(left_world.digest(), right_world.digest());

        left_world.transport.insert_interaction(left(&left_lines));
        right_world
            .transport
            .insert_interaction(right(&right_lines));

        assert_ne!(left_world.digest(), right_world.digest());
    }

    assert_digest_differs_for_interaction_delta(
        |lines| {
            BeltInteraction::new(
                BeltInteractionKind::EndTransfer,
                lines[0],
                Some(lines[1]),
                Some(TilePos::new(10, 0)),
                TilePos::new(90, 0),
            )
        },
        |lines| {
            BeltInteraction::new(
                BeltInteractionKind::SideLoad { near_lane: 0 },
                lines[0],
                Some(lines[1]),
                Some(TilePos::new(10, 0)),
                TilePos::new(90, 0),
            )
        },
    );
    assert_digest_differs_for_interaction_delta(
        |lines| {
            BeltInteraction::new(
                BeltInteractionKind::SideLoad { near_lane: 0 },
                lines[0],
                Some(lines[1]),
                Some(TilePos::new(10, 0)),
                TilePos::new(91, 0),
            )
        },
        |lines| {
            BeltInteraction::new(
                BeltInteractionKind::SideLoad { near_lane: 1 },
                lines[0],
                Some(lines[1]),
                Some(TilePos::new(10, 0)),
                TilePos::new(91, 0),
            )
        },
    );
    assert_digest_differs_for_interaction_delta(
        |lines| {
            BeltInteraction::new(
                BeltInteractionKind::BlockedFront,
                lines[0],
                None,
                None,
                TilePos::new(92, 0),
            )
        },
        |lines| {
            BeltInteraction::new(
                BeltInteractionKind::BlockedFront,
                lines[1],
                None,
                None,
                TilePos::new(92, 0),
            )
        },
    );
    assert_digest_differs_for_interaction_delta(
        |lines| {
            BeltInteraction::new(
                BeltInteractionKind::EndTransfer,
                lines[0],
                Some(lines[1]),
                Some(TilePos::new(10, 0)),
                TilePos::new(93, 0),
            )
        },
        |lines| {
            BeltInteraction::new(
                BeltInteractionKind::EndTransfer,
                lines[0],
                Some(lines[0]),
                Some(TilePos::new(10, 0)),
                TilePos::new(93, 0),
            )
        },
    );
    assert_digest_differs_for_interaction_delta(
        |lines| {
            BeltInteraction::new(
                BeltInteractionKind::EndTransfer,
                lines[0],
                Some(lines[1]),
                Some(TilePos::new(10, 0)),
                TilePos::new(94, 0),
            )
        },
        |lines| {
            BeltInteraction::new(
                BeltInteractionKind::EndTransfer,
                lines[0],
                Some(lines[1]),
                None,
                TilePos::new(94, 0),
            )
        },
    );
    assert_digest_differs_for_interaction_delta(
        |lines| {
            BeltInteraction::new(
                BeltInteractionKind::EndTransfer,
                lines[0],
                Some(lines[1]),
                Some(TilePos::new(10, 0)),
                TilePos::new(95, 0),
            )
        },
        |lines| {
            BeltInteraction::new(
                BeltInteractionKind::EndTransfer,
                lines[0],
                Some(lines[1]),
                Some(TilePos::new(10, 1)),
                TilePos::new(95, 0),
            )
        },
    );
    assert_digest_differs_for_interaction_delta(
        |lines| {
            BeltInteraction::new(
                BeltInteractionKind::EndTransfer,
                lines[0],
                Some(lines[1]),
                Some(TilePos::new(10, 0)),
                TilePos::new(96, 0),
            )
        },
        |lines| {
            BeltInteraction::new(
                BeltInteractionKind::EndTransfer,
                lines[0],
                Some(lines[1]),
                Some(TilePos::new(10, 0)),
                TilePos::new(96, 1),
            )
        },
    );
}

#[test]
fn insert_many_at_line_start_for_tests_rejects_invalid_lane() {
    let mut world = SimWorld::default();
    world
        .apply_core_command_for_tests(SimCommand::PlaceBelt {
            pos: TilePos::new(0, 0),
            direction: Direction::East,
            input_direction: Direction::East,
            speed: UnitsPerTick::new(8),
        })
        .unwrap();

    let error = world
        .insert_many_at_line_start_for_tests(0, 2, ItemKindId(1), 3)
        .unwrap_err();

    assert_eq!(error, SimCommandError::InvalidPort);
}

#[test]
fn insert_many_at_line_start_for_tests_updates_metrics_with_bounded_scans() {
    let mut world = SimWorld::default();
    world
        .apply_core_command_for_tests(SimCommand::PlaceBelt {
            pos: TilePos::new(0, 0),
            direction: Direction::East,
            input_direction: Direction::East,
            speed: UnitsPerTick::new(8),
        })
        .unwrap();

    world
        .insert_many_at_line_start_for_tests(0, 0, ItemKindId(1), 5)
        .unwrap();

    let output = tick_world_for_tests(&mut world);

    assert_eq!(output.metrics.simulated_items, 5);
    assert_eq!(world.last_metrics().simulated_items, 5);
    assert!(output.metrics.items_scanned < 5);
}

#[test]
fn removing_chest_with_contents_records_drops() {
    let mut world = SimWorld::with_catalog(catalog_for_tests());
    world
        .apply_core_command_for_tests(SimCommand::PlaceBuilding {
            def_id: "wooden_chest".to_string(),
            origin: TilePos::new(0, 0),
            direction: Direction::East,
            inserter_drop_direction: None,
        })
        .unwrap();
    let chest = world.building_snapshots()[0].id;
    world
        .insert_into_inventory_for_tests(
            chest,
            CoreInventoryRole::Storage,
            CoreItemStack {
                kind: TEST_IRON_ORE,
                amount: 3,
            },
        )
        .unwrap();

    apply_command_with_behavior_for_tests(
        &mut world,
        SimCommand::RemoveBuilding {
            pos: TilePos::new(0, 0),
        },
    )
    .unwrap();

    assert_eq!(
        world.removed_item_drops_for_tests(),
        vec![CoreItemStack {
            kind: TEST_IRON_ORE,
            amount: 3,
        }]
    );
}

#[test]
fn removing_splitter_with_internal_items_records_drops() {
    let mut world = connected_splitter_world_without_outputs_for_tests();
    let splitter_id = world
        .building_snapshots()
        .into_iter()
        .find(|snapshot| snapshot.def_id == "basic_splitter")
        .unwrap()
        .id;
    let splitter = world.buildings.get_mut(&splitter_id).unwrap();
    splitter.state = SimBuildingState::Splitter(SplitterRuntime {
        next_output: 0,
        next_output_by_lane: [0, 0],
        ingress_items: vec![SplitterIngressItem {
            item: TEST_IRON_ORE,
            input_channel: 0,
            lane: 0,
            progress: DistanceUnits::new(16),
        }],
        buffered_items: vec![SplitterBufferedItem {
            item: TEST_COPPER_ORE,
            source_channel: 0,
            lane: 0,
        }],
        egress_items: vec![SplitterEgressItem {
            item: TEST_COAL,
            source_channel: 0,
            output_channel: 1,
            lane: 1,
            progress: DistanceUnits::new(32),
        }],
    });

    apply_command_with_behavior_for_tests(
        &mut world,
        SimCommand::RemoveBuilding {
            pos: TilePos::new(0, 0),
        },
    )
    .unwrap();

    assert_eq!(
        world.removed_item_drops_for_tests(),
        vec![
            CoreItemStack {
                kind: TEST_IRON_ORE,
                amount: 1,
            },
            CoreItemStack {
                kind: TEST_COPPER_ORE,
                amount: 1,
            },
            CoreItemStack {
                kind: TEST_COAL,
                amount: 1,
            },
        ]
    );
}

#[test]
fn removed_core_inventory_drops_are_exposed_on_next_tick() {
    let mut world = SimWorld::with_catalog(catalog_for_tests());
    world
        .apply_core_command_for_tests(SimCommand::PlaceBuilding {
            def_id: "wooden_chest".to_string(),
            origin: TilePos::new(0, 0),
            direction: Direction::East,
            inserter_drop_direction: None,
        })
        .unwrap();
    let chest = world.building_snapshots()[0].id;
    world
        .insert_into_inventory_for_tests(
            chest,
            CoreInventoryRole::Storage,
            CoreItemStack {
                kind: TEST_IRON_ORE,
                amount: 3,
            },
        )
        .unwrap();
    world
        .apply_core_command_for_tests(SimCommand::RemoveBuilding {
            pos: TilePos::new(0, 0),
        })
        .unwrap();

    let output = tick_world_for_tests(&mut world);

    assert_eq!(
        output.removal_drops,
        vec![CoreRemovalDrop {
            origin: TilePos::new(0, 0),
            stack: CoreItemStack {
                kind: TEST_IRON_ORE,
                amount: 3,
            },
            instance: None,
        }]
    );
    assert!(world.removed_item_drops_for_tests().is_empty());
}

#[test]
fn take_from_inventory_command_removes_exact_core_slot_amount() {
    let mut world = SimWorld::with_catalog(catalog_for_tests());
    world
        .apply_core_command_for_tests(SimCommand::PlaceBuilding {
            def_id: "wooden_chest".to_string(),
            origin: TilePos::new(0, 0),
            direction: Direction::East,
            inserter_drop_direction: None,
        })
        .unwrap();
    let chest = world.building_snapshots()[0].id;
    world
        .apply_core_command_for_tests(SimCommand::InsertIntoInventory {
            building: chest,
            role: CoreInventoryRole::Storage,
            stack: CoreItemStack {
                kind: TEST_IRON_ORE,
                amount: 7,
            },
        })
        .unwrap();

    world
        .apply_core_command_for_tests(SimCommand::TakeFromInventory {
            building: chest,
            role: CoreInventoryRole::Storage,
            slot: 0,
            amount: 3,
        })
        .unwrap();

    let snapshot = world.building_snapshots().remove(0);
    let storage = snapshot
        .inventories
        .iter()
        .find(|inventory| inventory.role == CoreInventoryRole::Storage)
        .unwrap();
    assert_eq!(
        storage.slots[0],
        Some(CoreItemStack {
            kind: TEST_IRON_ORE,
            amount: 4,
        })
    );
}

#[test]
fn insert_into_inventory_rejects_overflow_without_partial_mutation() {
    let mut world = SimWorld::with_catalog(catalog_for_tests());
    world
        .apply_core_command_for_tests(SimCommand::PlaceBuilding {
            def_id: "stone_furnace".to_string(),
            origin: TilePos::new(0, 0),
            direction: Direction::East,
            inserter_drop_direction: None,
        })
        .unwrap();
    let furnace = world.building_snapshots()[0].id;
    world
        .apply_core_command_for_tests(SimCommand::InsertIntoInventory {
            building: furnace,
            role: CoreInventoryRole::Fuel,
            stack: CoreItemStack {
                kind: TEST_COAL,
                amount: 99,
            },
        })
        .unwrap();

    let error = world
        .apply_core_command_for_tests(SimCommand::InsertIntoInventory {
            building: furnace,
            role: CoreInventoryRole::Fuel,
            stack: CoreItemStack {
                kind: TEST_COAL,
                amount: 2,
            },
        })
        .unwrap_err();

    assert_eq!(error, SimCommandError::InventoryRejected);
    let snapshot = world.building_snapshots().remove(0);
    let storage = snapshot
        .inventories
        .iter()
        .find(|inventory| inventory.role == CoreInventoryRole::Fuel)
        .unwrap();
    assert_eq!(
        storage.slots[0],
        Some(CoreItemStack {
            kind: TEST_COAL,
            amount: 99,
        })
    );
}

#[test]
fn clear_machine_recipe_command_resets_behavior_machine_state() {
    let mut world = SimWorld::with_catalog(catalog_for_tests());
    world
        .apply_core_command_for_tests(SimCommand::PlaceBuilding {
            def_id: "stone_furnace".to_string(),
            origin: TilePos::new(0, 0),
            direction: Direction::East,
            inserter_drop_direction: None,
        })
        .unwrap();
    let furnace = world.building_snapshots()[0].id;

    set_machine_recipe_for_tests(&mut world, furnace, None);

    let snapshot = world.building_snapshots().remove(0);
    let machine = machine_runtime_for_tests(&snapshot);
    assert_eq!(machine.active_recipe, None);
    assert_eq!(machine.status, MachineStatus::NoRecipeSelected);
    assert_eq!(machine.progress_ticks, 0);
}

#[test]
fn clearing_machine_recipe_drains_recipe_inventories_to_removed_drops() {
    let mut world = SimWorld::with_catalog(catalog_for_tests());
    world
        .apply_core_command_for_tests(SimCommand::PlaceBuilding {
            def_id: "stone_furnace".to_string(),
            origin: TilePos::new(0, 0),
            direction: Direction::East,
            inserter_drop_direction: None,
        })
        .unwrap();
    let furnace = world.building_snapshots()[0].id;
    select_iron_plate_recipe(&mut world, furnace);
    for (role, stack) in [
        (
            CoreInventoryRole::Input,
            CoreItemStack {
                kind: TEST_IRON_ORE,
                amount: 2,
            },
        ),
        (
            CoreInventoryRole::Output,
            CoreItemStack {
                kind: TEST_IRON_PLATE,
                amount: 3,
            },
        ),
        (
            CoreInventoryRole::Fuel,
            CoreItemStack {
                kind: TEST_COAL,
                amount: 4,
            },
        ),
    ] {
        world
            .insert_into_inventory_for_tests(furnace, role, stack)
            .unwrap();
    }

    set_machine_recipe_for_tests(&mut world, furnace, None);

    let drops = world.removed_item_drops_for_tests();
    for stack in [
        CoreItemStack {
            kind: TEST_IRON_ORE,
            amount: 2,
        },
        CoreItemStack {
            kind: TEST_IRON_PLATE,
            amount: 3,
        },
        CoreItemStack {
            kind: TEST_COAL,
            amount: 4,
        },
    ] {
        assert!(
            drops.contains(&stack),
            "missing recipe reset drop: {stack:?}"
        );
    }
    let snapshot = world
        .building_snapshots()
        .into_iter()
        .find(|snapshot| snapshot.id == furnace)
        .unwrap();
    for role in [
        CoreInventoryRole::Input,
        CoreInventoryRole::Output,
        CoreInventoryRole::Fuel,
    ] {
        let inventory = snapshot
            .inventories
            .iter()
            .find(|inventory| inventory.role == role)
            .unwrap();
        assert!(inventory.slots.iter().all(Option::is_none));
    }
}

#[test]
fn changing_machine_recipe_drains_previous_recipe_inventories() {
    let mut world = SimWorld::with_catalog(catalog_for_tests());
    world
        .apply_core_command_for_tests(SimCommand::PlaceBuilding {
            def_id: "stone_furnace".to_string(),
            origin: TilePos::new(0, 0),
            direction: Direction::East,
            inserter_drop_direction: None,
        })
        .unwrap();
    let furnace = world.building_snapshots()[0].id;
    select_iron_plate_recipe(&mut world, furnace);
    world
        .insert_into_inventory_for_tests(
            furnace,
            CoreInventoryRole::Input,
            CoreItemStack {
                kind: TEST_IRON_ORE,
                amount: 2,
            },
        )
        .unwrap();

    set_machine_recipe_for_tests(&mut world, furnace, Some("copper_plate".to_string()));

    assert!(
        world
            .removed_item_drops_for_tests()
            .contains(&CoreItemStack {
                kind: TEST_IRON_ORE,
                amount: 2,
            })
    );
    let snapshot = world
        .building_snapshots()
        .into_iter()
        .find(|snapshot| snapshot.id == furnace)
        .unwrap();
    let machine = machine_runtime_for_tests(&snapshot);
    assert_eq!(machine.active_recipe.as_deref(), Some("copper_plate"));
    assert_eq!(machine.status, MachineStatus::Idle);
    assert_eq!(machine.progress_ticks, 0);
}

#[test]
fn furnace_input_inventory_accepts_drop_from_edge_ports() {
    let mut world = SimWorld::with_catalog(catalog_for_tests());
    world
        .apply_core_command_for_tests(SimCommand::PlaceBuilding {
            def_id: "stone_furnace".to_string(),
            origin: TilePos::new(0, 0),
            direction: Direction::East,
            inserter_drop_direction: None,
        })
        .unwrap();
    let furnace = world.building_snapshots()[0].id;
    select_iron_plate_recipe(&mut world, furnace);
    world
        .apply_core_command_for_tests(SimCommand::PlaceBuilding {
            def_id: "basic_inserter".to_string(),
            origin: TilePos::new(-1, 1),
            direction: Direction::West,
            inserter_drop_direction: Some(Direction::East),
        })
        .unwrap();
    world
        .apply_core_command_for_tests(SimCommand::PlaceBuilding {
            def_id: "basic_inserter".to_string(),
            origin: TilePos::new(1, 3),
            direction: Direction::North,
            inserter_drop_direction: Some(Direction::South),
        })
        .unwrap();

    let west_inserter_id = world.building_snapshots()[1].id;
    let west_inserter_building = world.buildings.get(&west_inserter_id).unwrap().clone();
    let SimBuildingState::Inserter(west_inserter) = west_inserter_building.state.clone() else {
        panic!("expected inserter");
    };

    assert!(world.try_inserter_drop(
        &west_inserter_building,
        &west_inserter,
        CoreItemStack {
            kind: TEST_IRON_ORE,
            amount: 1,
        },
        &mut SimDiff::default(),
    ));

    let north_inserter_id = world.building_snapshots()[2].id;
    let north_inserter_building = world.buildings.get(&north_inserter_id).unwrap().clone();
    let SimBuildingState::Inserter(north_inserter) = north_inserter_building.state.clone() else {
        panic!("expected inserter");
    };

    assert!(world.try_inserter_drop(
        &north_inserter_building,
        &north_inserter,
        CoreItemStack {
            kind: TEST_IRON_ORE,
            amount: 1,
        },
        &mut SimDiff::default(),
    ));

    let snapshot = world
        .building_snapshots()
        .into_iter()
        .find(|snapshot| {
            snapshot.kind == CoreBuildingKind::Machine && snapshot.def_id == "stone_furnace"
        })
        .unwrap();
    let input = snapshot
        .inventories
        .iter()
        .find(|inventory| inventory.role == CoreInventoryRole::Input)
        .unwrap();
    assert_eq!(
        input.slots[0],
        Some(CoreItemStack {
            kind: TEST_IRON_ORE,
            amount: 2,
        })
    );
}

#[test]
fn inserter_cannot_drop_into_machine_without_selected_recipe() {
    let mut world = SimWorld::with_catalog(catalog_for_tests());
    world
        .apply_core_command_for_tests(SimCommand::PlaceBuilding {
            def_id: "stone_furnace".to_string(),
            origin: TilePos::new(0, 0),
            direction: Direction::East,
            inserter_drop_direction: None,
        })
        .unwrap();
    let furnace = world.building_snapshots()[0].id;
    set_machine_recipe_for_tests(&mut world, furnace, None);
    world
        .apply_core_command_for_tests(SimCommand::PlaceBuilding {
            def_id: "basic_inserter".to_string(),
            origin: TilePos::new(-1, 1),
            direction: Direction::West,
            inserter_drop_direction: Some(Direction::East),
        })
        .unwrap();
    let inserter_id = world.building_snapshots()[1].id;
    let inserter_building = world.buildings.get(&inserter_id).unwrap().clone();
    let SimBuildingState::Inserter(inserter) = inserter_building.state.clone() else {
        panic!("expected inserter");
    };

    assert!(!world.try_inserter_drop_with_behavior(
        &inserter_building,
        &inserter,
        CoreItemStack {
            kind: TEST_IRON_ORE,
            amount: 1,
        },
        &mut SimDiff::default(),
        &TestBehaviorHost,
        &test_behavior_catalog(),
    ));
}

#[test]
fn furnace_output_pickup_uses_configured_output_ports() {
    let mut world = SimWorld::with_catalog(catalog_for_tests());
    world
        .apply_core_command_for_tests(SimCommand::PlaceBuilding {
            def_id: "stone_furnace".to_string(),
            origin: TilePos::new(0, 0),
            direction: Direction::East,
            inserter_drop_direction: None,
        })
        .unwrap();
    let furnace = world.building_snapshots()[0].id;
    select_iron_plate_recipe(&mut world, furnace);
    world
        .insert_into_inventory_for_tests(
            furnace,
            CoreInventoryRole::Output,
            CoreItemStack {
                kind: TEST_IRON_PLATE,
                amount: 1,
            },
        )
        .unwrap();
    world
        .apply_core_command_for_tests(SimCommand::PlaceBuilding {
            def_id: "basic_inserter".to_string(),
            origin: TilePos::new(3, 1),
            direction: Direction::West,
            inserter_drop_direction: Some(Direction::East),
        })
        .unwrap();
    let inserter_id = world.building_snapshots()[1].id;
    let inserter_building = world.buildings.get(&inserter_id).unwrap().clone();
    let SimBuildingState::Inserter(inserter) = inserter_building.state.clone() else {
        panic!("expected inserter");
    };

    let picked = world.try_inserter_pickup(&inserter_building, &inserter, &mut SimDiff::default());

    assert_eq!(
        picked,
        Some(CoreItemStack {
            kind: TEST_IRON_PLATE,
            amount: 1,
        })
    );
}

#[test]
fn furnace_input_inventory_rejects_unaccepted_items_on_edge_ports() {
    let mut world = SimWorld::with_catalog(catalog_for_tests());
    world
        .apply_core_command_for_tests(SimCommand::PlaceBuilding {
            def_id: "stone_furnace".to_string(),
            origin: TilePos::new(0, 0),
            direction: Direction::East,
            inserter_drop_direction: None,
        })
        .unwrap();
    world
        .apply_core_command_for_tests(SimCommand::PlaceBuilding {
            def_id: "basic_inserter".to_string(),
            origin: TilePos::new(1, -1),
            direction: Direction::South,
            inserter_drop_direction: Some(Direction::North),
        })
        .unwrap();
    let inserter_id = world.building_snapshots()[1].id;
    let inserter_building = world.buildings.get(&inserter_id).unwrap().clone();
    let SimBuildingState::Inserter(inserter) = inserter_building.state.clone() else {
        panic!("expected inserter");
    };

    assert!(!world.try_inserter_drop(
        &inserter_building,
        &inserter,
        CoreItemStack {
            kind: TEST_IRON_PLATE,
            amount: 1,
        },
        &mut SimDiff::default(),
    ));
}

#[test]
fn removing_inserter_with_carried_item_records_drop() {
    let mut world = SimWorld::with_catalog(catalog_for_tests());
    world
        .apply_core_command_for_tests(SimCommand::PlaceBuilding {
            def_id: "basic_inserter".to_string(),
            origin: TilePos::new(0, 0),
            direction: Direction::East,
            inserter_drop_direction: Some(Direction::West),
        })
        .unwrap();
    let inserter = world.building_snapshots()[0].id;
    world.replace_inserter_state(
        inserter,
        InserterRuntime {
            pickup_direction: Direction::East,
            drop_direction: Direction::West,
            cooldown_remaining_ticks: 0,
            carried: Some(CoreItemStack {
                kind: TEST_IRON_PLATE,
                amount: 1,
            }),
        },
    );

    world
        .apply_core_command_for_tests(SimCommand::RemoveBuilding {
            pos: TilePos::new(0, 0),
        })
        .unwrap();

    assert_eq!(
        world.removed_item_drops_for_tests(),
        vec![CoreItemStack {
            kind: TEST_IRON_PLATE,
            amount: 1,
        }]
    );
}

#[test]
fn extending_belt_line_preserves_existing_items_without_drops() {
    let mut world = SimWorld::with_catalog(catalog_for_tests());
    world
        .apply_core_command_for_tests(SimCommand::PlaceBelt {
            pos: TilePos::new(0, 0),
            direction: Direction::East,
            input_direction: Direction::East,
            speed: UnitsPerTick::new(8),
        })
        .unwrap();
    world
        .insert_many_at_line_start_for_tests(0, 0, TEST_IRON_ORE, 1)
        .unwrap();

    world
        .apply_core_command_for_tests(SimCommand::PlaceBelt {
            pos: TilePos::new(1, 0),
            direction: Direction::East,
            input_direction: Direction::East,
            speed: UnitsPerTick::new(8),
        })
        .unwrap();

    let visible_count = world
        .visible_items_for_bounds(VisibleTileBounds::new(
            TilePos::new(-10, -10),
            TilePos::new(10, 10),
        ))
        .filter(|item| item.item == TEST_IRON_ORE)
        .count();

    assert_eq!(visible_count, 1);
    assert!(world.removed_item_drops_for_tests().is_empty());
}

#[test]
fn removing_unrelated_belt_tile_preserves_items_on_surviving_tiles() {
    let mut world = SimWorld::with_catalog(catalog_for_tests());
    world
        .build_straight_belt_line(TilePos::new(0, 0), 3, Direction::East, UnitsPerTick::new(8))
        .unwrap();
    world
        .insert_many_at_line_start_for_tests(0, 0, TEST_IRON_ORE, 1)
        .unwrap();

    world
        .apply_core_command_for_tests(SimCommand::RemoveBuilding {
            pos: TilePos::new(1, 0),
        })
        .unwrap();

    let visible_items = world
        .visible_items_for_bounds(VisibleTileBounds::new(
            TilePos::new(-10, -10),
            TilePos::new(10, 10),
        ))
        .filter(|item| item.item == TEST_IRON_ORE)
        .collect::<Vec<_>>();

    assert_eq!(visible_items.len(), 1);
    assert_eq!(visible_items[0].tile, TilePos::new(2, 0));
    assert!(world.removed_item_drops_for_tests().is_empty());
}

#[test]
fn removing_belt_with_items_records_drops() {
    let mut world = SimWorld::with_catalog(catalog_for_tests());
    world
        .apply_core_command_for_tests(SimCommand::PlaceBelt {
            pos: TilePos::new(0, 0),
            direction: Direction::East,
            input_direction: Direction::East,
            speed: UnitsPerTick::new(8),
        })
        .unwrap();
    world
        .insert_many_at_line_start_for_tests(0, 0, TEST_IRON_ORE, 1)
        .unwrap();

    world
        .apply_core_command_for_tests(SimCommand::RemoveBuilding {
            pos: TilePos::new(0, 0),
        })
        .unwrap();

    assert_eq!(
        world.removed_item_drops_for_tests(),
        vec![CoreItemStack {
            kind: TEST_IRON_ORE,
            amount: 1,
        }]
    );
    assert_eq!(
        world.removed_item_drop_records_for_tests(),
        &[CoreRemovalDrop {
            origin: TilePos::new(0, 0),
            stack: CoreItemStack {
                kind: TEST_IRON_ORE,
                amount: 1,
            },
            instance: None,
        }]
    );
}

#[test]
fn removing_working_furnace_records_in_progress_inputs() {
    let mut world = SimWorld::with_catalog(catalog_for_tests());
    world
        .apply_core_command_for_tests(SimCommand::PlaceBuilding {
            def_id: "stone_furnace".to_string(),
            origin: TilePos::new(0, 0),
            direction: Direction::East,
            inserter_drop_direction: None,
        })
        .unwrap();
    let furnace = world.building_snapshots()[0].id;
    world
        .insert_into_inventory_for_tests(
            furnace,
            CoreInventoryRole::Input,
            CoreItemStack {
                kind: TEST_IRON_ORE,
                amount: 1,
            },
        )
        .unwrap();
    world
        .insert_into_inventory_for_tests(
            furnace,
            CoreInventoryRole::Fuel,
            CoreItemStack {
                kind: TEST_COAL,
                amount: 1,
            },
        )
        .unwrap();

    tick_world_for_tests(&mut world);
    apply_command_with_behavior_for_tests(
        &mut world,
        SimCommand::RemoveBuilding {
            pos: TilePos::new(0, 0),
        },
    )
    .unwrap();

    assert!(
        world
            .removed_item_drops_for_tests()
            .contains(&CoreItemStack {
                kind: TEST_IRON_ORE,
                amount: 1,
            })
    );
}

#[test]
fn resetting_working_furnace_records_in_progress_input_drop() {
    let mut world = SimWorld::with_catalog(catalog_for_tests());
    world
        .apply_core_command_for_tests(SimCommand::PlaceBuilding {
            def_id: "stone_furnace".to_string(),
            origin: TilePos::new(0, 0),
            direction: Direction::East,
            inserter_drop_direction: None,
        })
        .unwrap();
    let furnace = world.building_snapshots()[0].id;
    select_iron_plate_recipe(&mut world, furnace);
    world
        .insert_into_inventory_for_tests(
            furnace,
            CoreInventoryRole::Input,
            CoreItemStack {
                kind: TEST_IRON_ORE,
                amount: 1,
            },
        )
        .unwrap();
    world
        .insert_into_inventory_for_tests(
            furnace,
            CoreInventoryRole::Fuel,
            CoreItemStack {
                kind: TEST_COAL,
                amount: 1,
            },
        )
        .unwrap();

    tick_world_for_tests(&mut world);
    set_machine_recipe_for_tests(&mut world, furnace, None);

    assert!(
        world
            .removed_item_drops_for_tests()
            .contains(&CoreItemStack {
                kind: TEST_IRON_ORE,
                amount: 1,
            })
    );
    let snapshot = world
        .building_snapshots()
        .into_iter()
        .find(|snapshot| snapshot.id == furnace)
        .unwrap();
    let machine = machine_runtime_for_tests(&snapshot);
    assert_eq!(machine.active_recipe, None);
    assert_eq!(machine.progress_ticks, 0);
    assert_eq!(machine.status, MachineStatus::NoRecipeSelected);
}

#[test]
fn removing_working_miner_records_extracted_resource_drop() {
    let mut world = SimWorld::with_catalog(catalog_for_tests());
    world.seed_resource_for_tests(TilePos::new(2, 2), TEST_IRON_ORE, 1);
    world
        .apply_core_command_for_tests(SimCommand::PlaceBuilding {
            def_id: "basic_miner".to_string(),
            origin: TilePos::new(0, 0),
            direction: Direction::East,
            inserter_drop_direction: None,
        })
        .unwrap();
    let miner = world.building_snapshots()[0].id;
    world
        .insert_into_inventory_for_tests(
            miner,
            CoreInventoryRole::Fuel,
            CoreItemStack {
                kind: TEST_COAL,
                amount: 1,
            },
        )
        .unwrap();

    tick_world_for_tests(&mut world);
    apply_command_with_behavior_for_tests(
        &mut world,
        SimCommand::RemoveBuilding {
            pos: TilePos::new(0, 0),
        },
    )
    .unwrap();

    assert!(
        world
            .removed_item_drops_for_tests()
            .contains(&CoreItemStack {
                kind: TEST_IRON_ORE,
                amount: 1,
            })
    );
}

#[test]
fn removing_machine_with_pending_outputs_records_drops() {
    let mut world = SimWorld::with_catalog(catalog_for_tests());
    world
        .apply_core_command_for_tests(SimCommand::PlaceBuilding {
            def_id: "stone_furnace".to_string(),
            origin: TilePos::new(0, 0),
            direction: Direction::East,
            inserter_drop_direction: None,
        })
        .unwrap();
    let furnace = world.building_snapshots()[0].id;
    world.replace_behavior_state(
        furnace,
        test_machine_behavior_state(MachineRuntime {
            active_recipe: None,
            progress_ticks: 1,
            status: MachineStatus::OutputBlocked,
            fuel_remaining_ticks: 0,
            fuel_total_ticks: 0,
            fuel_temperature: 0,
            pending_outputs: vec![
                behavior_stack(TEST_IRON_PLATE, 1),
                behavior_stack(TEST_COAL, 2),
            ],
        }),
    );

    apply_command_with_behavior_for_tests(
        &mut world,
        SimCommand::RemoveBuilding {
            pos: TilePos::new(0, 0),
        },
    )
    .unwrap();

    assert_eq!(
        world.removed_item_drops_for_tests(),
        vec![
            CoreItemStack {
                kind: TEST_IRON_PLATE,
                amount: 1,
            },
            CoreItemStack {
                kind: TEST_COAL,
                amount: 2,
            },
        ]
    );
}

#[test]
fn removing_missing_building_returns_structured_error() {
    let mut world = SimWorld::default();
    let error = world
        .apply_core_command_for_tests(SimCommand::RemoveBuilding {
            pos: TilePos::new(99, 99),
        })
        .unwrap_err();

    assert_eq!(
        error,
        SimCommandError::MissingBuilding {
            pos: TilePos::new(99, 99)
        }
    );
}

#[test]
fn save_roundtrip_preserves_core_tick() {
    let mut world = SimWorld::with_catalog(catalog_for_tests());
    tick_world_for_tests(&mut world);
    tick_world_for_tests(&mut world);

    let snapshot = world.snapshot();
    let restored = SimWorld::from_snapshot(catalog_for_tests(), snapshot).unwrap();

    assert_eq!(restored.digest(), world.digest());
}

#[test]
fn save_roundtrip_preserves_player_inventory() {
    let mut world = SimWorld::with_catalog(catalog_for_tests());
    world
        .insert_into_player_inventory_for_tests(CoreItemStack {
            kind: TEST_IRON_PLATE,
            amount: 10,
        })
        .unwrap();

    let snapshot = world.snapshot();
    let restored = SimWorld::from_snapshot(catalog_for_tests(), snapshot).unwrap();

    assert_eq!(
        restored.player_inventory_snapshot().slots[0],
        Some(CoreItemStack {
            kind: TEST_IRON_PLATE,
            amount: 10,
        })
    );
}

#[test]
fn equipped_items_create_separate_character_containers() {
    let world = SimWorld::with_catalog(character_catalog_for_tests());
    let sections = world.character_container_sections();

    let ids = sections
        .iter()
        .map(|section| section.container_id.as_str())
        .collect::<Vec<_>>();

    assert_eq!(
        ids,
        vec!["tool_belt", "left_pocket", "right_pocket", "backpack_main"]
    );
    assert!(
        sections
            .iter()
            .any(|section| section.container_id.as_str() == "tool_belt" && section.quick_access)
    );
    assert!(
        sections.iter().any(
            |section| section.container_id.as_str() == "backpack_main" && !section.quick_access
        )
    );
}

#[test]
fn duplicate_character_container_ids_are_ignored() {
    let mut catalog = character_catalog_for_tests();
    let work_pants = catalog
        .items
        .iter_mut()
        .find(|item| item.def_id == "work_pants")
        .unwrap();
    let equipment = work_pants.equipment.as_mut().unwrap();
    equipment.provides_containers[1].id = "left_pocket".to_string();

    let world = SimWorld::with_catalog(catalog);
    let sections = world.character_container_sections();
    let ids = sections
        .iter()
        .map(|section| section.container_id.as_str())
        .collect::<Vec<_>>();

    assert_eq!(ids, vec!["tool_belt", "left_pocket", "backpack_main"]);
}

#[test]
fn duplicate_starting_equipment_slots_are_ignored() {
    let mut catalog = character_catalog_for_tests();
    catalog.personal_inventories.starting_equipment[1].slot = "waist".to_string();

    let world = SimWorld::with_catalog(catalog);
    let sections = world.character_container_sections();
    let ids = sections
        .iter()
        .map(|section| section.container_id.as_str())
        .collect::<Vec<_>>();

    assert_eq!(ids, vec!["tool_belt", "backpack_main"]);
}

#[test]
fn inserting_into_named_character_container_uses_that_container_policy() {
    let mut world = SimWorld::with_catalog(character_catalog_for_tests());

    let belt_result = world.insert_into_character_container(
        "tool_belt",
        CoreItemStack {
            kind: TEST_IRON_PLATE,
            amount: 1,
        },
        InsertMode::AtomicAllOrNothing,
    );
    let pocket_result = world.insert_into_character_container(
        "left_pocket",
        CoreItemStack {
            kind: TEST_IRON_PLATE,
            amount: 1,
        },
        InsertMode::AtomicAllOrNothing,
    );

    assert_eq!(
        belt_result.rejection,
        Some(InventoryRejection::ItemNotAccepted)
    );
    assert_eq!(pocket_result.rejected, None);
    assert_eq!(
        world.character_container_slot("left_pocket", 0),
        Some(CoreItemStack {
            kind: TEST_IRON_PLATE,
            amount: 1,
        })
    );
}

#[test]
fn inserting_into_missing_character_container_reports_missing_container() {
    let mut world = SimWorld::with_catalog(character_catalog_for_tests());

    let result = world.insert_into_character_container(
        "vest_pocket",
        CoreItemStack {
            kind: TEST_IRON_PLATE,
            amount: 1,
        },
        InsertMode::AtomicAllOrNothing,
    );

    assert_eq!(result.accepted, None);
    assert_eq!(result.rejection, Some(InventoryRejection::MissingContainer));
}

#[test]
fn pickup_routing_prefers_specialized_then_quick_then_backpack() {
    let mut world = SimWorld::with_catalog(character_catalog_for_tests());

    let ore = world.route_stack_into_character(
        CoreItemStack {
            kind: TEST_IRON_ORE,
            amount: 1,
        },
        InsertMode::AtomicAllOrNothing,
    );
    let plate = world.route_stack_into_character(
        CoreItemStack {
            kind: TEST_IRON_PLATE,
            amount: 1,
        },
        InsertMode::AtomicAllOrNothing,
    );

    assert_eq!(ore.accepted_container.as_deref(), Some("backpack_main"));
    assert_eq!(plate.accepted_container.as_deref(), Some("left_pocket"));
}

#[test]
fn pickup_routing_continues_to_backpack_when_quick_container_is_full() {
    let mut world = SimWorld::with_catalog(character_catalog_for_tests());
    for _ in 0..4 {
        let result = world.insert_into_character_container(
            "left_pocket",
            CoreItemStack {
                kind: TEST_IRON_PLATE,
                amount: 1,
            },
            InsertMode::AtomicAllOrNothing,
        );
        assert_eq!(result.rejected, None);
    }

    let result = world.route_stack_into_character(
        CoreItemStack {
            kind: TEST_IRON_PLATE,
            amount: 1,
        },
        InsertMode::AtomicAllOrNothing,
    );

    assert_eq!(result.accepted_container.as_deref(), Some("right_pocket"));
}

#[test]
fn partial_pickup_routing_carries_remainder_to_later_containers() {
    let mut world = SimWorld::with_catalog(character_catalog_for_tests());

    let result = world.route_stack_into_character(
        CoreItemStack {
            kind: TEST_IRON_PLATE,
            amount: 20,
        },
        InsertMode::PartialFit,
    );

    assert_eq!(
        result.accepted,
        Some(CoreItemStack {
            kind: TEST_IRON_PLATE,
            amount: 20,
        })
    );
    assert_eq!(result.rejected, None);
    assert_eq!(result.accepted_container.as_deref(), Some("left_pocket"));
    assert_eq!(
        world.character_container_slot("left_pocket", 0),
        Some(CoreItemStack {
            kind: TEST_IRON_PLATE,
            amount: 4,
        })
    );
    assert_eq!(
        world.character_container_slot("right_pocket", 0),
        Some(CoreItemStack {
            kind: TEST_IRON_PLATE,
            amount: 4,
        })
    );
    assert_eq!(
        world.character_container_slot("backpack_main", 0),
        Some(CoreItemStack {
            kind: TEST_IRON_PLATE,
            amount: 12,
        })
    );
}

#[test]
fn right_click_takes_one_item_and_ctrl_click_splits_half() {
    let mut world = SimWorld::with_catalog(character_catalog_for_tests());
    let result = world.insert_into_character_container(
        "backpack_main",
        CoreItemStack {
            kind: TEST_IRON_ORE,
            amount: 9,
        },
        InsertMode::AtomicAllOrNothing,
    );
    assert_eq!(result.rejected, None);

    let one = world.take_one_from_character_slot("backpack_main", 0);
    let half = world.split_half_from_character_slot("backpack_main", 0);

    assert_eq!(
        one,
        Some(CoreItemStack {
            kind: TEST_IRON_ORE,
            amount: 1,
        })
    );
    assert_eq!(
        half,
        Some(CoreItemStack {
            kind: TEST_IRON_ORE,
            amount: 4,
        })
    );
    assert_eq!(
        world.character_container_slot("backpack_main", 0),
        Some(CoreItemStack {
            kind: TEST_IRON_ORE,
            amount: 4,
        })
    );
}

#[test]
fn save_roundtrip_preserves_character_container_contents() {
    let mut world = SimWorld::with_catalog(character_catalog_for_tests());
    world.insert_into_character_container(
        "left_pocket",
        CoreItemStack {
            kind: TEST_IRON_PLATE,
            amount: 3,
        },
        InsertMode::AtomicAllOrNothing,
    );

    let snapshot = world.snapshot();
    let restored = SimWorld::from_snapshot(character_catalog_for_tests(), snapshot).unwrap();

    assert_eq!(
        restored.character_container_slot("left_pocket", 0),
        Some(CoreItemStack {
            kind: TEST_IRON_PLATE,
            amount: 3,
        })
    );
    assert_eq!(restored.digest(), world.digest());
}

#[test]
fn restore_rejects_tampered_character_container_snapshot() {
    let world = SimWorld::with_catalog(character_catalog_for_tests());
    let mut snapshot = world.snapshot();
    let character = snapshot.character_inventory.as_mut().unwrap();
    character.containers[0].name = "Wrong container".to_string();

    let error = match SimWorld::from_snapshot(character_catalog_for_tests(), snapshot) {
        Ok(_) => panic!("tampered character container snapshot should be rejected"),
        Err(error) => error,
    };

    assert!(error.contains("character inventory"), "{error}");
}

#[test]
fn digest_differs_for_character_container_contents() {
    let empty = SimWorld::with_catalog(character_catalog_for_tests());
    let mut filled = SimWorld::with_catalog(character_catalog_for_tests());
    filled.insert_into_character_container(
        "left_pocket",
        CoreItemStack {
            kind: TEST_IRON_PLATE,
            amount: 1,
        },
        InsertMode::AtomicAllOrNothing,
    );

    assert_ne!(empty.digest(), filled.digest());
}

#[test]
fn dropped_loaded_backpack_preserves_contents() {
    let mut world = SimWorld::with_catalog(character_catalog_for_tests());
    let backpack = world.create_loaded_container_item_for_tests(
        "small_backpack",
        "backpack_main",
        vec![CoreItemStack {
            kind: TEST_IRON_ORE,
            amount: 7,
        }],
    );

    world.drop_loaded_container_on_surface_for_tests(TilePos::new(2, 3), backpack);
    let dropped = world.surface_item_drops_snapshot_for_tests();
    assert_eq!(dropped.len(), 1);
    assert_eq!(dropped[0].stack.kind, backpack.kind);
    assert_eq!(dropped[0].instance, backpack.instance);
    assert_eq!(
        world.loaded_container_contents_for_tests(backpack.instance.unwrap()),
        vec![CoreItemStack {
            kind: TEST_IRON_ORE,
            amount: 7,
        }]
    );
}

#[test]
fn loaded_backpack_cannot_be_inserted_into_player_backpack() {
    let mut world = SimWorld::with_catalog(character_catalog_for_tests());
    let backpack = world.create_loaded_container_item_for_tests(
        "small_backpack",
        "backpack_main",
        vec![CoreItemStack {
            kind: TEST_IRON_ORE,
            amount: 7,
        }],
    );

    let result = world.insert_loaded_item_into_character_container(
        "backpack_main",
        backpack,
        InsertMode::AtomicAllOrNothing,
    );

    assert_eq!(result.accepted, None);
    assert_eq!(
        result.rejection,
        Some(InventoryRejection::LoadedContainerNotAllowed)
    );
}

#[test]
fn taking_equipped_backpack_preserves_contents_as_loaded_item() {
    let mut world = SimWorld::with_catalog(character_catalog_for_tests());
    world.insert_into_character_container(
        "backpack_main",
        CoreItemStack {
            kind: TEST_IRON_ORE,
            amount: 7,
        },
        InsertMode::AtomicAllOrNothing,
    );

    let backpack = world
        .take_from_character_equipment_slot("back")
        .expect("equipped backpack should be removable");
    let instance = backpack.instance.expect("filled backpack should be loaded");

    assert!(
        world
            .character_equipment()
            .into_iter()
            .all(|equipment| equipment.slot.as_str() != "back")
    );
    assert!(
        world
            .character_container_sections()
            .into_iter()
            .all(|section| section.container_id.as_str() != "backpack_main")
    );
    assert_eq!(
        world.loaded_container_contents_for_tests(instance),
        vec![CoreItemStack {
            kind: TEST_IRON_ORE,
            amount: 7,
        }]
    );
}

#[test]
fn snapshot_roundtrip_accepts_backpack_removed_to_surface_drop() {
    let mut world = SimWorld::with_catalog(character_catalog_for_tests());
    world.insert_into_character_container(
        "backpack_main",
        CoreItemStack {
            kind: TEST_IRON_ORE,
            amount: 7,
        },
        InsertMode::AtomicAllOrNothing,
    );
    let backpack = world
        .take_from_character_equipment_slot("back")
        .expect("equipped backpack should be removable");
    world.drop_loaded_container_on_surface_for_tests(TilePos::new(2, 3), backpack);

    let restored = SimWorld::from_snapshot(character_catalog_for_tests(), world.snapshot())
        .expect("removed equipped backpack should be a valid saved character state");

    assert!(
        restored
            .character_equipment()
            .into_iter()
            .all(|equipment| equipment.slot.as_str() != "back")
    );
    assert_eq!(restored.surface_item_drops_snapshot_for_tests().len(), 1);
    assert_eq!(
        restored.loaded_container_contents_for_tests(backpack.instance.unwrap()),
        vec![CoreItemStack {
            kind: TEST_IRON_ORE,
            amount: 7,
        }]
    );
}

#[test]
fn equipping_loaded_backpack_restores_its_container_contents() {
    let mut world = SimWorld::with_catalog(character_catalog_for_tests());
    world.insert_into_character_container(
        "backpack_main",
        CoreItemStack {
            kind: TEST_IRON_ORE,
            amount: 7,
        },
        InsertMode::AtomicAllOrNothing,
    );
    let backpack = world
        .take_from_character_equipment_slot("back")
        .expect("equipped backpack should be removable");

    let result = world.equip_character_item(backpack);

    assert_eq!(result.rejected, None);
    assert_eq!(result.replaced, None);
    assert!(
        world
            .character_equipment()
            .into_iter()
            .any(|equipment| equipment.slot.as_str() == "back")
    );
    assert_eq!(
        world.character_container_slot("backpack_main", 0),
        Some(CoreItemStack {
            kind: TEST_IRON_ORE,
            amount: 7,
        })
    );
}

#[test]
fn equipping_backpack_swaps_existing_loaded_backpack_to_replaced_entry() {
    let mut world = SimWorld::with_catalog(character_catalog_for_tests());
    world.insert_into_character_container(
        "backpack_main",
        CoreItemStack {
            kind: TEST_IRON_ORE,
            amount: 7,
        },
        InsertMode::AtomicAllOrNothing,
    );
    let replacement = world.create_loaded_container_item_for_tests(
        "small_backpack",
        "backpack_main",
        vec![CoreItemStack {
            kind: TEST_IRON_PLATE,
            amount: 3,
        }],
    );

    let result = world.equip_character_item(replacement);
    let replaced = result
        .replaced
        .expect("old backpack should be returned to cursor");

    assert_eq!(result.rejected, None);
    assert_eq!(
        world.character_container_slot("backpack_main", 0),
        Some(CoreItemStack {
            kind: TEST_IRON_PLATE,
            amount: 3,
        })
    );
    assert_eq!(
        world.loaded_container_contents_for_tests(replaced.instance.unwrap()),
        vec![CoreItemStack {
            kind: TEST_IRON_ORE,
            amount: 7,
        }]
    );
}

#[test]
fn behavior_insert_effect_is_rejected_when_loaded_container_rule_is_violated() {
    let mut world = SimWorld::with_catalog(character_catalog_for_tests());
    let backpack = world.create_loaded_container_item_for_tests(
        "small_backpack",
        "packed_backpack",
        vec![CoreItemStack {
            kind: TEST_IRON_ORE,
            amount: 1,
        }],
    );

    let result = world.insert_loaded_item_into_character_container(
        "backpack_main",
        backpack,
        InsertMode::AtomicAllOrNothing,
    );

    assert_eq!(
        result.rejection,
        Some(InventoryRejection::LoadedContainerNotAllowed)
    );
}

#[test]
fn loaded_backpack_cannot_be_taken_from_chest_as_stack() {
    let mut world = SimWorld::with_catalog(character_catalog_for_tests());
    world
        .apply_core_command_for_tests(SimCommand::PlaceBuilding {
            def_id: "wooden_chest".to_string(),
            origin: TilePos::new(0, 0),
            direction: Direction::East,
            inserter_drop_direction: None,
        })
        .unwrap();
    let chest = world
        .building_id_at_origin_for_tests(TilePos::new(0, 0))
        .unwrap();
    let backpack = world.create_loaded_container_item_for_tests(
        "small_backpack",
        "packed_backpack",
        vec![CoreItemStack {
            kind: TEST_IRON_ORE,
            amount: 7,
        }],
    );
    let instance = backpack.instance.unwrap();

    let result = world.insert_loaded_item_into_building_inventory_for_tests(
        chest,
        CoreInventoryRole::Storage,
        backpack,
        InsertMode::AtomicAllOrNothing,
    );
    assert_eq!(result.rejected, None);

    assert_eq!(
        world.take_from_inventory_stack(chest, CoreInventoryRole::Storage, 0, 1),
        Err(SimCommandError::InventoryRejected)
    );
    assert_eq!(
        world.loaded_container_contents_for_tests(instance),
        vec![CoreItemStack {
            kind: TEST_IRON_ORE,
            amount: 7,
        }]
    );
    assert_eq!(
        world.inventory_slot(chest, CoreInventoryRole::Storage, 0),
        Some(CoreItemStack {
            kind: backpack.kind,
            amount: 1,
        })
    );
}

#[test]
fn loaded_backpack_can_be_stored_in_chest() {
    let mut world = SimWorld::with_catalog(character_catalog_for_tests());
    world
        .apply_core_command_for_tests(SimCommand::PlaceBuilding {
            def_id: "wooden_chest".to_string(),
            origin: TilePos::new(0, 0),
            direction: Direction::East,
            inserter_drop_direction: None,
        })
        .unwrap();
    let chest = world
        .building_id_at_origin_for_tests(TilePos::new(0, 0))
        .unwrap();
    let backpack = world.create_loaded_container_item_for_tests(
        "small_backpack",
        "packed_backpack",
        vec![CoreItemStack {
            kind: TEST_IRON_ORE,
            amount: 7,
        }],
    );

    let result = world.insert_loaded_item_into_building_inventory_for_tests(
        chest,
        CoreInventoryRole::Storage,
        backpack,
        InsertMode::AtomicAllOrNothing,
    );

    assert_eq!(result.rejected, None);
    assert_eq!(
        world.loaded_container_contents_for_tests(backpack.instance.unwrap()),
        vec![CoreItemStack {
            kind: TEST_IRON_ORE,
            amount: 7,
        }]
    );
}

#[test]
fn loaded_container_snapshot_roundtrip_preserves_contents() {
    let mut world = SimWorld::with_catalog(character_catalog_for_tests());
    let backpack = world.create_loaded_container_item_for_tests(
        "small_backpack",
        "backpack_main",
        vec![CoreItemStack {
            kind: TEST_IRON_ORE,
            amount: 7,
        }],
    );

    let restored =
        SimWorld::from_snapshot(character_catalog_for_tests(), world.snapshot()).unwrap();

    assert_eq!(
        restored.loaded_container_contents_for_tests(backpack.instance.unwrap()),
        vec![CoreItemStack {
            kind: TEST_IRON_ORE,
            amount: 7,
        }]
    );
    assert_eq!(restored.digest(), world.digest());
}

#[test]
fn snapshot_roundtrip_preserves_pending_loaded_container_surface_drop() {
    let mut world = SimWorld::with_catalog(character_catalog_for_tests());
    let backpack =
        world.create_loaded_container_item_for_tests("small_backpack", "backpack_main", Vec::new());
    world.drop_loaded_container_on_surface_for_tests(TilePos::new(2, 3), backpack);

    let restored =
        SimWorld::from_snapshot(character_catalog_for_tests(), world.snapshot()).unwrap();

    assert_eq!(restored.surface_item_drops_snapshot_for_tests().len(), 1);
    assert_eq!(
        restored.surface_item_drops_snapshot_for_tests()[0].instance,
        backpack.instance
    );
}

#[test]
fn pending_loaded_container_drops_are_not_drained_as_stack_only_app_drops() {
    let mut world = SimWorld::with_catalog(character_catalog_for_tests());
    let backpack = world.create_loaded_container_item_for_tests(
        "small_backpack",
        "packed_backpack",
        Vec::new(),
    );
    world.drop_loaded_container_on_surface_for_tests(TilePos::new(2, 3), backpack);

    let (_removed, surface) = world.take_pending_item_drops();

    assert!(surface.is_empty());
    assert_eq!(world.surface_item_drops_snapshot_for_tests().len(), 1);
    assert_eq!(
        world.surface_item_drops_snapshot_for_tests()[0].instance,
        backpack.instance
    );
}

#[test]
fn pending_loaded_container_surface_drop_keeps_tile_occupied_after_app_sync() {
    let mut world = SimWorld::with_catalog(character_catalog_for_tests());
    let backpack = world.create_loaded_container_item_for_tests(
        "small_backpack",
        "packed_backpack",
        Vec::new(),
    );
    world.drop_loaded_container_on_surface_for_tests(TilePos::new(2, 3), backpack);

    world.set_occupied_surface_tiles(Vec::new());

    assert!(!world.try_inserter_drop_to_surface(
        TilePos::new(2, 3),
        crate::ids::DEFAULT_SURFACE_Z,
        CoreItemStack {
            kind: TEST_WOOD,
            amount: 1,
        },
        &mut SimDiff::default(),
    ));
}

#[test]
fn restore_rejects_missing_loaded_container_instance_reference() {
    let mut world = SimWorld::with_catalog(character_catalog_for_tests());
    world
        .apply_core_command_for_tests(SimCommand::PlaceBuilding {
            def_id: "wooden_chest".to_string(),
            origin: TilePos::new(0, 0),
            direction: Direction::East,
            inserter_drop_direction: None,
        })
        .unwrap();
    let chest = world
        .building_id_at_origin_for_tests(TilePos::new(0, 0))
        .unwrap();
    let backpack = world.create_loaded_container_item_for_tests(
        "small_backpack",
        "packed_backpack",
        Vec::new(),
    );
    let result = world.insert_loaded_item_into_building_inventory_for_tests(
        chest,
        CoreInventoryRole::Storage,
        backpack,
        InsertMode::AtomicAllOrNothing,
    );
    assert_eq!(result.rejected, None);
    let mut snapshot = world.snapshot();
    snapshot.loaded_containers.clear();

    let error = match SimWorld::from_snapshot(character_catalog_for_tests(), snapshot) {
        Ok(_) => panic!("missing loaded container reference should be rejected"),
        Err(error) => error,
    };

    assert!(
        error.contains("missing loaded container instance"),
        "{error}"
    );
}

#[test]
fn restore_normalizes_missing_slot_instance_vectors_for_old_saves() {
    let world = SimWorld::with_catalog(character_catalog_for_tests());
    let mut snapshot = world.snapshot();
    snapshot
        .player_inventory
        .as_mut()
        .unwrap()
        .clear_slot_instances_for_tests();

    let restored = SimWorld::from_snapshot(character_catalog_for_tests(), snapshot).unwrap();

    assert!(restored.is_empty_for_catalog_install());
    assert_eq!(
        restored.player_inventory_snapshot().slot_instances,
        vec![None; restored.player_inventory_snapshot().slots.len()]
    );
}

#[test]
fn loaded_container_digest_tracks_contents() {
    let mut empty = SimWorld::with_catalog(character_catalog_for_tests());
    empty.create_loaded_container_item_for_tests("small_backpack", "packed_backpack", Vec::new());
    let mut filled = SimWorld::with_catalog(character_catalog_for_tests());
    filled.create_loaded_container_item_for_tests(
        "small_backpack",
        "packed_backpack",
        vec![CoreItemStack {
            kind: TEST_IRON_ORE,
            amount: 7,
        }],
    );

    assert_ne!(empty.digest(), filled.digest());
}

#[test]
fn digest_tracks_loaded_container_surface_drop_instance() {
    let mut first = SimWorld::with_catalog(character_catalog_for_tests());
    let first_a = first.create_loaded_container_item_for_tests(
        "small_backpack",
        "packed_backpack",
        Vec::new(),
    );
    let first_b = first.create_loaded_container_item_for_tests(
        "small_backpack",
        "packed_backpack",
        Vec::new(),
    );

    let mut second = SimWorld::with_catalog(character_catalog_for_tests());
    let second_a = second.create_loaded_container_item_for_tests(
        "small_backpack",
        "packed_backpack",
        Vec::new(),
    );
    let second_b = second.create_loaded_container_item_for_tests(
        "small_backpack",
        "packed_backpack",
        Vec::new(),
    );
    assert_eq!(first_a.instance, second_a.instance);
    assert_eq!(first_b.instance, second_b.instance);

    first.drop_loaded_container_on_surface_for_tests(TilePos::new(4, 5), first_a);
    second.drop_loaded_container_on_surface_for_tests(TilePos::new(4, 5), second_b);

    assert_ne!(first.digest(), second.digest());
}

#[test]
fn digest_tracks_loaded_container_removal_drop_instance() {
    let mut first = SimWorld::with_catalog(character_catalog_for_tests());
    first
        .apply_core_command_for_tests(SimCommand::PlaceBuilding {
            def_id: "wooden_chest".to_string(),
            origin: TilePos::new(0, 0),
            direction: Direction::East,
            inserter_drop_direction: None,
        })
        .unwrap();
    let first_chest = first
        .building_id_at_origin_for_tests(TilePos::new(0, 0))
        .unwrap();
    let first_a = first.create_loaded_container_item_for_tests(
        "small_backpack",
        "packed_backpack",
        Vec::new(),
    );
    let first_b = first.create_loaded_container_item_for_tests(
        "small_backpack",
        "packed_backpack",
        Vec::new(),
    );

    let mut second = SimWorld::with_catalog(character_catalog_for_tests());
    second
        .apply_core_command_for_tests(SimCommand::PlaceBuilding {
            def_id: "wooden_chest".to_string(),
            origin: TilePos::new(0, 0),
            direction: Direction::East,
            inserter_drop_direction: None,
        })
        .unwrap();
    let second_chest = second
        .building_id_at_origin_for_tests(TilePos::new(0, 0))
        .unwrap();
    let second_a = second.create_loaded_container_item_for_tests(
        "small_backpack",
        "packed_backpack",
        Vec::new(),
    );
    let second_b = second.create_loaded_container_item_for_tests(
        "small_backpack",
        "packed_backpack",
        Vec::new(),
    );
    assert_eq!(first_a.instance, second_a.instance);
    assert_eq!(first_b.instance, second_b.instance);

    assert_eq!(
        first
            .insert_loaded_item_into_building_inventory_for_tests(
                first_chest,
                CoreInventoryRole::Storage,
                first_a,
                InsertMode::AtomicAllOrNothing,
            )
            .rejected,
        None
    );
    assert_eq!(
        second
            .insert_loaded_item_into_building_inventory_for_tests(
                second_chest,
                CoreInventoryRole::Storage,
                second_b,
                InsertMode::AtomicAllOrNothing,
            )
            .rejected,
        None
    );

    first
        .apply_core_command_for_tests(SimCommand::RemoveBuilding {
            pos: TilePos::new(0, 0),
        })
        .unwrap();
    second
        .apply_core_command_for_tests(SimCommand::RemoveBuilding {
            pos: TilePos::new(0, 0),
        })
        .unwrap();

    assert_ne!(first.digest(), second.digest());
}

#[test]
fn player_slot_insert_respects_player_wide_item_limit() {
    let mut catalog = catalog_for_tests();
    catalog.personal_inventories.player.stack_limits = vec![CoreItemStackLimit {
        item: TEST_IRON_PLATE,
        max_stack: 10,
    }];
    let mut world = SimWorld::with_catalog(catalog);
    world
        .insert_into_player_inventory_for_tests(CoreItemStack {
            kind: TEST_IRON_PLATE,
            amount: 10,
        })
        .unwrap();

    let result = world.insert_into_player_inventory_slot(
        1,
        CoreItemStack {
            kind: TEST_IRON_PLATE,
            amount: 1,
        },
        InsertMode::AtomicAllOrNothing,
    );

    assert_eq!(result.accepted, None);
    assert_eq!(
        result.rejected,
        Some(CoreItemStack {
            kind: TEST_IRON_PLATE,
            amount: 1,
        })
    );
    assert_eq!(
        result.rejection,
        Some(InventoryRejection::StackLimitExceeded)
    );
    assert_eq!(world.player_inventory_snapshot().slots[1], None);
}

#[test]
fn pocket_rejects_item_that_exceeds_max_size() {
    let mut catalog = catalog_for_tests();
    catalog.personal_inventories.player.max_item_size = CoreItemSizeClass::Small;
    let pipe = ItemKindId(42);
    catalog.items.push(CoreItemDef {
        id: pipe,
        def_id: "pipe".to_string(),
        max_stack: 100,
        weight_grams: 1_200,
        bulk_units: 5,
        size_class: CoreItemSizeClass::Medium,
        tags: vec!["component".to_string()],
        equipment: None,
    });
    let mut world = SimWorld::with_catalog(catalog);

    let result = world.insert_into_player_inventory(
        CoreItemStack {
            kind: pipe,
            amount: 1,
        },
        InsertMode::AtomicAllOrNothing,
    );

    assert_eq!(result.accepted, None);
    assert_eq!(
        result.rejected,
        Some(CoreItemStack {
            kind: pipe,
            amount: 1,
        })
    );
    assert_eq!(result.rejection, Some(InventoryRejection::ItemTooLarge));
}

#[test]
fn bulk_limit_rejects_stack_when_weight_and_slots_fit() {
    let mut catalog = catalog_for_tests();
    catalog.personal_inventories.player.hard_weight_limit_grams = Some(100_000);
    catalog.personal_inventories.player.max_bulk_units = Some(4);
    let mut world = SimWorld::with_catalog(catalog);

    let result = world.insert_into_player_inventory(
        CoreItemStack {
            kind: TEST_IRON_ORE,
            amount: 2,
        },
        InsertMode::AtomicAllOrNothing,
    );

    assert_eq!(result.accepted, None);
    assert_eq!(
        result.rejected,
        Some(CoreItemStack {
            kind: TEST_IRON_ORE,
            amount: 2,
        })
    );
    assert_eq!(
        result.rejection,
        Some(InventoryRejection::BulkLimitExceeded)
    );
}

#[test]
fn tag_filtered_container_accepts_matching_item_and_rejects_other_item() {
    let mut catalog = catalog_for_tests();
    catalog.personal_inventories.player.accepts_tags = vec!["ore".to_string()];
    let mut world = SimWorld::with_catalog(catalog);

    let ore = world.insert_into_player_inventory(
        CoreItemStack {
            kind: TEST_IRON_ORE,
            amount: 1,
        },
        InsertMode::AtomicAllOrNothing,
    );
    let plate = world.insert_into_player_inventory(
        CoreItemStack {
            kind: TEST_IRON_PLATE,
            amount: 1,
        },
        InsertMode::AtomicAllOrNothing,
    );

    assert_eq!(ore.rejected, None);
    assert_eq!(plate.accepted, None);
    assert_eq!(plate.rejection, Some(InventoryRejection::ItemNotAccepted));
}

#[test]
fn behavior_insert_inventory_rejects_item_that_exceeds_inventory_size() {
    let mut catalog = catalog_for_tests();
    let assembler = catalog
        .buildings
        .iter_mut()
        .find(|building| building.id == "basic_assembler")
        .unwrap();
    let output = assembler
        .inventories
        .iter_mut()
        .find(|inventory| inventory.role == CoreInventoryRole::Output)
        .unwrap();
    output.max_item_size = CoreItemSizeClass::Tiny;
    let mut world = SimWorld::with_catalog(catalog);
    world
        .apply_core_command_for_tests(SimCommand::PlaceBuilding {
            def_id: "basic_assembler".to_string(),
            origin: TilePos::new(0, 0),
            direction: Direction::East,
            inserter_drop_direction: None,
        })
        .unwrap();

    let output = world.tick_with_behavior_runtime(oversized_insert_runtime());

    assert_eq!(output.metrics.behavior_effects_applied, 0);
    assert_eq!(output.metrics.behavior_effects_rejected, 1);
    let BehaviorEffectApplication::Rejected { effects } =
        &output.behavior_effect_reports[0].application
    else {
        panic!("expected rejected behavior effect report");
    };
    assert!(matches!(
        effects[0].reason,
        BehaviorEffectRejectionReason::InventoryRejected
    ));
}

#[test]
fn save_roundtrip_preserves_cursor_inventory() {
    let mut world = SimWorld::with_catalog(catalog_for_tests());
    world
        .set_cursor_stack_for_tests(CoreItemStack {
            kind: TEST_IRON_ORE,
            amount: 7,
        })
        .unwrap();

    let snapshot = world.snapshot();
    let restored = SimWorld::from_snapshot(catalog_for_tests(), snapshot).unwrap();

    assert_eq!(
        restored.cursor_inventory_snapshot().slots[0],
        Some(CoreItemStack {
            kind: TEST_IRON_ORE,
            amount: 7,
        })
    );
}

#[test]
fn time_of_day_advances_with_core_ticks_and_wraps() {
    let mut world = SimWorld::with_catalog(catalog_for_tests());
    world.set_time_of_day(TimeOfDay::from_normalized(0.999_99).unwrap());

    tick_world_for_tests(&mut world);

    assert!(
        world.time_of_day().normalized() < 0.01,
        "time of day should wrap after reaching the end of the day"
    );
}

#[test]
fn save_roundtrip_preserves_time_of_day() {
    let mut world = SimWorld::with_catalog(catalog_for_tests());
    world.set_time_of_day(TimeOfDay::from_normalized(0.75).unwrap());

    let snapshot = world.snapshot();
    let restored = SimWorld::from_snapshot(catalog_for_tests(), snapshot).unwrap();

    assert_eq!(
        restored.time_of_day(),
        TimeOfDay::from_normalized(0.75).unwrap()
    );
}

#[test]
fn default_solar_curve_is_game_like_with_midday_plateau() {
    let settings = DayNightSettings::default();

    assert_eq!(
        settings.solar_factor(TimeOfDay::from_normalized(0.10).unwrap()),
        SuppliedRatio::ZERO
    );
    assert!(
        settings
            .solar_factor(TimeOfDay::from_normalized(0.25).unwrap())
            .ppm()
            > 0
    );
    assert_eq!(
        settings.solar_factor(TimeOfDay::from_normalized(0.50).unwrap()),
        SuppliedRatio::FULL
    );
    assert_eq!(
        settings.solar_factor(TimeOfDay::from_normalized(0.65).unwrap()),
        SuppliedRatio::FULL
    );
    assert!(
        settings
            .solar_factor(TimeOfDay::from_normalized(0.76).unwrap())
            .ppm()
            < SuppliedRatio::FULL.ppm()
    );
    assert_eq!(
        settings.solar_factor(TimeOfDay::from_normalized(0.90).unwrap()),
        SuppliedRatio::ZERO
    );
}

#[test]
fn save_roundtrip_preserves_day_night_settings() {
    let mut world = SimWorld::with_catalog(catalog_for_tests());
    let settings = DayNightSettings {
        day_length_ticks: 12_000,
        solar_curve: SolarCurveSettings::GameLike {
            sunrise_start: 0.18,
            full_day_start: 0.24,
            full_day_end: 0.68,
            sunset_end: 0.84,
        },
    };
    world.set_day_night_settings(settings);

    let restored = SimWorld::from_snapshot(catalog_for_tests(), world.snapshot()).unwrap();

    assert_eq!(restored.day_night_settings(), settings);
}

#[test]
fn save_roundtrip_preserves_behavior_quarantine() {
    let mut world = SimWorld::with_catalog(catalog_for_tests());
    place_invalid_effect_building(&mut world);
    let building = world
        .building_id_at_origin_for_tests(TilePos::new(0, 0))
        .unwrap();
    world.tick_with_behavior_runtime(invalid_effect_runtime(quarantine_instance_policy()));

    let snapshot = world.snapshot();

    assert_eq!(snapshot.behavior_quarantine.len(), 1);
    assert_eq!(snapshot.behavior_quarantine[0].building, building);
    assert_eq!(snapshot.behavior_quarantine[0].origin, TilePos::new(0, 0));
    assert_eq!(
        snapshot.behavior_quarantine[0]
            .behavior_id
            .as_ref()
            .map(BehaviorId::as_str),
        Some(TEST_MACHINE_BEHAVIOR_ID)
    );
    assert_eq!(
        snapshot.behavior_quarantine[0].reason,
        BehaviorEffectRejectionReason::MissingResource {
            pos: TilePos::new(999, 999),
        }
    );

    let mut restored = SimWorld::from_snapshot(catalog_for_tests(), snapshot).unwrap();
    assert_eq!(restored.behavior_quarantine_count(), 1);
    assert_eq!(
        restored
            .building_at(TilePos::new(0, 0))
            .unwrap()
            .state
            .behavior_state()
            .unwrap()
            .status
            .as_str(),
        "old_state"
    );

    let output =
        restored.tick_with_behavior_runtime(invalid_effect_runtime(quarantine_instance_policy()));

    assert_eq!(output.metrics.behavior_ticks_skipped, 1);
    assert_eq!(output.metrics.behavior_effects_rejected, 0);
    let BehaviorEffectApplication::Skipped { reason } =
        &output.behavior_effect_reports[0].application
    else {
        panic!("expected restored quarantine to skip behavior tick");
    };
    assert_eq!(*reason, BehaviorTickSkipReason::Quarantined);
}

#[test]
fn clearing_behavior_quarantine_releases_instance_for_next_tick() {
    let mut world = SimWorld::with_catalog(catalog_for_tests());
    place_invalid_effect_building(&mut world);
    let building = world
        .building_id_at_origin_for_tests(TilePos::new(0, 0))
        .unwrap();
    world.tick_with_behavior_runtime(invalid_effect_runtime(quarantine_instance_policy()));
    assert_eq!(world.behavior_quarantine_count(), 1);

    let cleared = world.clear_behavior_quarantine(building);

    assert!(cleared.is_some());
    assert_eq!(world.behavior_quarantine_count(), 0);

    let output =
        world.tick_with_behavior_runtime(invalid_effect_runtime(quarantine_instance_policy()));
    let BehaviorEffectApplication::Quarantined { .. } =
        &output.behavior_effect_reports[0].application
    else {
        panic!("expected released behavior to tick and quarantine again");
    };
}

#[test]
fn save_roundtrip_preserves_belt_items() {
    let mut world = SimWorld::with_catalog(catalog_for_tests());
    world
        .build_straight_belt_line(TilePos::new(0, 0), 3, Direction::East, UnitsPerTick::new(8))
        .unwrap();
    world
        .insert_many_at_line_start_for_tests(0, 0, TEST_IRON_ORE, 1)
        .unwrap();
    tick_world_for_tests(&mut world);

    let snapshot = world.snapshot();
    let restored = SimWorld::from_snapshot(catalog_for_tests(), snapshot).unwrap();

    assert_eq!(restored.digest(), world.digest());
    let visible = restored
        .visible_items_for_bounds(VisibleTileBounds::new(
            TilePos::new(-10, -10),
            TilePos::new(10, 10),
        ))
        .collect::<Vec<_>>();
    assert_eq!(visible.len(), 1);
    assert_eq!(visible[0].item, TEST_IRON_ORE);
}

#[test]
fn restore_rejects_invalid_belt_item_gap_snapshot() {
    let mut world = SimWorld::with_catalog(catalog_for_tests());
    world
        .build_straight_belt_line(TilePos::new(0, 0), 3, Direction::East, UnitsPerTick::new(8))
        .unwrap();
    world
        .insert_many_at_line_start_for_tests(0, 0, TEST_IRON_ORE, 1)
        .unwrap();

    let mut snapshot = world.snapshot();
    let line = snapshot.transport.lines.values_mut().next().unwrap();
    line.lanes[0]
        .gaps_after
        .push(crate::units::DistanceUnits::ZERO);

    let error = match SimWorld::from_snapshot(catalog_for_tests(), snapshot) {
        Ok(_) => panic!("invalid stream gap snapshot should be rejected"),
        Err(error) => error,
    };

    assert!(error.contains("line"), "{error}");
    assert!(error.contains("lane"), "{error}");
    assert!(error.contains("gap"), "{error}");
}

#[test]
fn restore_rejects_tampered_inventory_definition_snapshot() {
    let mut world = SimWorld::with_catalog(catalog_for_tests());
    world
        .apply_core_command_for_tests(SimCommand::PlaceBuilding {
            def_id: "stone_furnace".to_string(),
            origin: TilePos::new(0, 0),
            direction: Direction::East,
            inserter_drop_direction: None,
        })
        .unwrap();

    let mut snapshot = world.snapshot();
    let record = snapshot.inventories.values_mut().next().unwrap();
    record.inventory.set_max_stack_for_tests(999);

    let error = match SimWorld::from_snapshot(catalog_for_tests(), snapshot) {
        Ok(_) => panic!("tampered inventory definition snapshot should be rejected"),
        Err(error) => error,
    };

    assert!(error.contains("inventory"), "{error}");
    assert!(
        error.contains("catalog") || error.contains("definition") || error.contains("max_stack"),
        "{error}"
    );
}

#[test]
fn restore_rejects_tampered_building_ports_snapshot() {
    let mut world = SimWorld::with_catalog(catalog_for_tests());
    world
        .apply_core_command_for_tests(SimCommand::PlaceBuilding {
            def_id: "stone_furnace".to_string(),
            origin: TilePos::new(0, 0),
            direction: Direction::East,
            inserter_drop_direction: None,
        })
        .unwrap();

    let mut snapshot = world.snapshot();
    let building = snapshot.buildings.values_mut().next().unwrap();
    building.ports.clear();

    let error = match SimWorld::from_snapshot(catalog_for_tests(), snapshot) {
        Ok(_) => panic!("tampered building ports snapshot should be rejected"),
        Err(error) => error,
    };

    assert!(error.contains("building"), "{error}");
    assert!(error.contains("ports"), "{error}");
}

#[test]
fn restore_rejects_quarantine_for_missing_building() {
    let world = SimWorld::with_catalog(catalog_for_tests());
    let mut snapshot = world.snapshot();
    snapshot
        .behavior_quarantine
        .push(SimBehaviorQuarantineSnapshot {
            building: crate::ids::BuildingId(999),
            origin: TilePos::new(0, 0),
            behavior_id: Some(BehaviorId::new(TEST_MACHINE_BEHAVIOR_ID)),
            reason: BehaviorEffectRejectionReason::InventoryRejected,
        });

    let error = match SimWorld::from_snapshot(catalog_for_tests(), snapshot) {
        Ok(_) => panic!("missing quarantine building should be rejected"),
        Err(error) => error,
    };

    assert!(error.contains("quarantine"), "{error}");
    assert!(error.contains("missing building"), "{error}");
}

#[test]
fn save_roundtrip_preserves_machine_inventory_and_recipe() {
    let mut world = SimWorld::with_catalog(catalog_for_tests());
    world
        .apply_core_command_for_tests(SimCommand::PlaceBuilding {
            def_id: "stone_furnace".to_string(),
            origin: TilePos::new(0, 0),
            direction: Direction::East,
            inserter_drop_direction: None,
        })
        .unwrap();
    let furnace = world.building_snapshots()[0].id;
    select_iron_plate_recipe(&mut world, furnace);
    world
        .insert_into_inventory_for_tests(
            furnace,
            CoreInventoryRole::Input,
            CoreItemStack {
                kind: TEST_IRON_ORE,
                amount: 1,
            },
        )
        .unwrap();
    world
        .insert_into_inventory_for_tests(
            furnace,
            CoreInventoryRole::Fuel,
            CoreItemStack {
                kind: TEST_COAL,
                amount: 1,
            },
        )
        .unwrap();
    tick_world_for_tests(&mut world);

    let snapshot = world.snapshot();
    let restored = SimWorld::from_snapshot(catalog_for_tests(), snapshot).unwrap();

    assert_eq!(restored.digest(), world.digest());
    let restored_furnace = restored
        .building_snapshots()
        .into_iter()
        .find(|snapshot| snapshot.def_id == "stone_furnace")
        .unwrap();
    let machine = machine_runtime_for_tests(&restored_furnace);
    assert_eq!(machine.active_recipe.as_deref(), Some("iron_plate"));
    assert_eq!(machine.progress_ticks, 1);
}

fn belt_items_at_tile(world: &SimWorld, pos: TilePos) -> usize {
    (0..2)
        .filter(|&lane| belt_lane_has_item_at_tile(world, pos, lane))
        .count()
}

fn belt_lane_has_item_at_tile(world: &SimWorld, pos: TilePos, lane: usize) -> bool {
    let Some((line_id, _, min_distance, max_distance)) = world.line_window_for_tile(pos) else {
        return false;
    };
    let Some(line) = world.transport.line(line_id) else {
        return false;
    };
    line.first_in_window(lane, min_distance, max_distance)
        .is_some()
}

fn place_underground_pair_with_gap(world: &mut SimWorld, gap: u8) -> (TilePos, TilePos) {
    let entrance = TilePos::new(0, 0);
    let exit = TilePos::new(i32::from(gap), 0);
    world
        .apply_core_command_for_tests(SimCommand::PlaceUndergroundBelt {
            def_id: "basic_underground_belt".to_string(),
            entrance,
            exit,
            direction: Direction::East,
        })
        .unwrap();
    (entrance, exit)
}

fn tile_has_item_in_line_window(world: &SimWorld, pos: TilePos) -> bool {
    let Some((line_id, _, min_distance, max_distance)) = world.line_window_for_tile(pos) else {
        return false;
    };
    let Some(line) = world.transport.line(line_id) else {
        return false;
    };
    (0..2).any(|lane| {
        !line
            .lane_positions_in_range(lane, min_distance, max_distance)
            .is_empty()
    })
}

fn ticks_until_item_at_tile(world: &mut SimWorld, pos: TilePos, max_ticks: usize) -> Option<usize> {
    for tick in 1..=max_ticks {
        tick_world_for_tests(world);
        if tile_has_item_in_line_window(world, pos) {
            return Some(tick);
        }
    }
    None
}

fn total_items_on_line(world: &SimWorld, line_id: LineId) -> usize {
    let Some(line) = world.transport.line(line_id) else {
        return 0;
    };
    line.lane(0).item_count() + line.lane(1).item_count()
}

fn underground_runtime_items(world: &SimWorld) -> Vec<UndergroundTransportItem> {
    world
        .transport
        .nodes_sorted()
        .find_map(|node| match &node.runtime {
            TransportNodeRuntime::Underground(runtime) => Some(runtime.items.clone()),
            _ => None,
        })
        .unwrap_or_default()
}

fn underground_runtime_node_id(world: &SimWorld) -> TransportNodeId {
    world
        .transport
        .nodes_sorted()
        .find(|node| node.kind == TransportNodeKind::Underground)
        .unwrap()
        .id
}

fn push_visible_underground_entrance_item(world: &mut SimWorld, item: ItemKindId, lane: usize) {
    let node_id = underground_runtime_node_id(world);
    world
        .transport
        .underground_runtime_mut(node_id)
        .unwrap()
        .items
        .push(UndergroundTransportItem {
            item,
            lane,
            progress: DistanceUnits::new(1),
        });
}

fn drop_item_on_underground_entrance(world: &mut SimWorld, entrance: TilePos) {
    world
        .apply_core_command_for_tests(SimCommand::DropItemOnBeltTile {
            pos: entrance,
            lane: 0,
            distance_numerator: 128,
            distance_denominator: 128,
            item: TEST_IRON_ORE,
        })
        .unwrap();
}

#[test]
fn underground_runtime_survives_snapshot_restore() {
    let mut world = SimWorld::with_catalog(catalog_for_tests());
    world
        .apply_core_command_for_tests(SimCommand::PlaceUndergroundBelt {
            def_id: "basic_underground_belt".to_string(),
            entrance: TilePos::new(0, 0),
            exit: TilePos::new(3, 0),
            direction: Direction::East,
        })
        .unwrap();
    world.rebuild_transport_lines();
    world
        .apply_core_command_for_tests(SimCommand::DropItemOnBeltTile {
            pos: TilePos::new(0, 0),
            lane: 0,
            distance_numerator: 128,
            distance_denominator: 128,
            item: TEST_IRON_ORE,
        })
        .unwrap();
    tick_world_for_tests(&mut world);

    let restored = SimWorld::from_snapshot(catalog_for_tests(), world.snapshot()).unwrap();

    let underground_items = restored
        .transport
        .nodes_sorted()
        .filter_map(|node| match &node.runtime {
            TransportNodeRuntime::Underground(runtime) => Some(runtime.items.len()),
            _ => None,
        })
        .sum::<usize>();
    assert_eq!(underground_items, 1);
}

#[test]
fn player_can_take_visible_underground_endpoint_item() {
    let mut world = SimWorld::with_catalog(catalog_for_tests());
    world
        .apply_core_command_for_tests(SimCommand::PlaceUndergroundBelt {
            def_id: "basic_underground_belt".to_string(),
            entrance: TilePos::new(0, 0),
            exit: TilePos::new(3, 0),
            direction: Direction::East,
        })
        .unwrap();
    world.rebuild_transport_lines();
    push_visible_underground_entrance_item(&mut world, TEST_IRON_ORE, 0);

    let stack = world.take_item_from_belt_tile(TilePos::new(0, 0)).unwrap();

    assert_eq!(stack.kind, TEST_IRON_ORE);
    assert_eq!(stack.amount, 1);
}

#[test]
fn first_item_stack_on_belt_tile_returns_visible_underground_endpoint_item() {
    let mut world = SimWorld::with_catalog(catalog_for_tests());
    world
        .apply_core_command_for_tests(SimCommand::PlaceUndergroundBelt {
            def_id: "basic_underground_belt".to_string(),
            entrance: TilePos::new(0, 0),
            exit: TilePos::new(3, 0),
            direction: Direction::East,
        })
        .unwrap();
    world.rebuild_transport_lines();
    push_visible_underground_entrance_item(&mut world, TEST_IRON_ORE, 0);

    assert_eq!(
        world.first_item_stack_on_belt_tile(TilePos::new(0, 0)),
        Some(CoreItemStack {
            kind: TEST_IRON_ORE,
            amount: 1,
        })
    );
}

#[test]
fn first_item_stack_on_belt_tile_matches_take_order_for_underground_and_surface_item() {
    let mut world = SimWorld::with_catalog(catalog_for_tests());
    world
        .apply_core_command_for_tests(SimCommand::PlaceUndergroundBelt {
            def_id: "basic_underground_belt".to_string(),
            entrance: TilePos::new(0, 0),
            exit: TilePos::new(3, 0),
            direction: Direction::East,
        })
        .unwrap();
    world.rebuild_transport_lines();
    push_visible_underground_entrance_item(&mut world, TEST_IRON_ORE, 0);
    world
        .apply_core_command_for_tests(SimCommand::DropItemOnBeltTile {
            pos: TilePos::new(0, 0),
            lane: 1,
            distance_numerator: 64,
            distance_denominator: 128,
            item: TEST_COPPER_ORE,
        })
        .unwrap();

    let preview = world
        .first_item_stack_on_belt_tile(TilePos::new(0, 0))
        .unwrap();
    let removed = world.take_item_from_belt_tile(TilePos::new(0, 0)).unwrap();

    assert_eq!(
        preview,
        CoreItemStack {
            kind: TEST_IRON_ORE,
            amount: 1,
        }
    );
    assert_eq!(removed, preview);
}

#[test]
fn underground_entrance_endpoint_item_is_visible_while_progress_before_half_tile() {
    let mut world = SimWorld::with_catalog(catalog_for_tests());
    world
        .apply_core_command_for_tests(SimCommand::PlaceUndergroundBelt {
            def_id: "basic_underground_belt".to_string(),
            entrance: TilePos::new(0, 0),
            exit: TilePos::new(4, 0),
            direction: Direction::East,
        })
        .unwrap();
    world.rebuild_transport_lines();

    let node_id = underground_runtime_node_id(&world);
    world
        .transport
        .underground_runtime_mut(node_id)
        .unwrap()
        .items
        .push(UndergroundTransportItem {
            item: TEST_IRON_ORE,
            lane: 0,
            progress: DistanceUnits::new(DistanceUnits::UNITS_PER_TILE / 2 - 1),
        });

    let visible = SimRenderView::extract(
        &world,
        VisibleTileBounds::new(TilePos::new(0, 0), TilePos::new(0, 0)),
    );

    assert_eq!(visible.visible_items.len(), 1);
    assert_eq!(visible.visible_items[0].tile, TilePos::new(0, 0));
    assert_eq!(visible.visible_items[0].item, TEST_IRON_ORE);
}

#[test]
fn underground_exit_endpoint_item_is_visible_while_progress_in_last_half_tile() {
    let mut world = SimWorld::with_catalog(catalog_for_tests());
    world
        .apply_core_command_for_tests(SimCommand::PlaceUndergroundBelt {
            def_id: "basic_underground_belt".to_string(),
            entrance: TilePos::new(0, 0),
            exit: TilePos::new(4, 0),
            direction: Direction::East,
        })
        .unwrap();
    world.rebuild_transport_lines();

    let node_id = underground_runtime_node_id(&world);
    let runtime = world.transport.underground_runtime_mut(node_id).unwrap();
    runtime.items.push(UndergroundTransportItem {
        item: TEST_IRON_ORE,
        lane: 1,
        progress: runtime.distance - DistanceUnits::new(DistanceUnits::UNITS_PER_TILE / 2),
    });

    let visible = SimRenderView::extract(
        &world,
        VisibleTileBounds::new(TilePos::new(4, 0), TilePos::new(4, 0)),
    );

    assert_eq!(visible.visible_items.len(), 1);
    assert_eq!(visible.visible_items[0].tile, TilePos::new(4, 0));
    assert_eq!(visible.visible_items[0].item, TEST_IRON_ORE);
    assert_eq!(visible.visible_items[0].lane, 1);
}

#[test]
fn malformed_underground_runtime_lane_is_hidden_and_not_pickable() {
    let mut world = SimWorld::with_catalog(catalog_for_tests());
    world
        .apply_core_command_for_tests(SimCommand::PlaceUndergroundBelt {
            def_id: "basic_underground_belt".to_string(),
            entrance: TilePos::new(0, 0),
            exit: TilePos::new(4, 0),
            direction: Direction::East,
        })
        .unwrap();
    world.rebuild_transport_lines();

    let node_id = underground_runtime_node_id(&world);
    world
        .transport
        .underground_runtime_mut(node_id)
        .unwrap()
        .items
        .push(UndergroundTransportItem {
            item: TEST_IRON_ORE,
            lane: 2,
            progress: DistanceUnits::new(1),
        });

    let visible = SimRenderView::extract(
        &world,
        VisibleTileBounds::new(TilePos::new(0, 0), TilePos::new(4, 0)),
    );

    assert!(visible.visible_items.is_empty());
    assert_eq!(
        world.first_item_stack_on_belt_tile(TilePos::new(0, 0)),
        None
    );
    assert!(world.take_item_from_belt_tile(TilePos::new(0, 0)).is_err());
    assert!(
        world
            .belt_pickup_candidates(
                TilePos::new(0, 0),
                TilePos::new(0, -1),
                crate::ids::DEFAULT_SURFACE_Z
            )
            .is_empty()
    );
    assert_eq!(underground_runtime_items(&world).len(), 1);
}

#[test]
fn underground_mid_tunnel_item_is_hidden_and_not_pickable() {
    let mut world = SimWorld::with_catalog(catalog_for_tests());
    world
        .apply_core_command_for_tests(SimCommand::PlaceUndergroundBelt {
            def_id: "basic_underground_belt".to_string(),
            entrance: TilePos::new(0, 0),
            exit: TilePos::new(4, 0),
            direction: Direction::East,
        })
        .unwrap();
    world.rebuild_transport_lines();

    let node_id = underground_runtime_node_id(&world);
    world
        .transport
        .underground_runtime_mut(node_id)
        .unwrap()
        .items
        .push(UndergroundTransportItem {
            item: TEST_IRON_ORE,
            lane: 0,
            progress: DistanceUnits::new(DistanceUnits::UNITS_PER_TILE),
        });

    let visible = SimRenderView::extract(
        &world,
        VisibleTileBounds::new(TilePos::new(0, 0), TilePos::new(4, 0)),
    );

    assert!(visible.visible_items.is_empty());
    assert!(world.take_item_from_belt_tile(TilePos::new(0, 0)).is_err());
    assert!(world.take_item_from_belt_tile(TilePos::new(4, 0)).is_err());
    assert_eq!(underground_runtime_items(&world).len(), 1);
}

#[test]
fn underground_ingressed_item_hides_after_reaching_entrance_front() {
    let mut world = SimWorld::with_catalog(catalog_for_tests());
    let entrance = TilePos::new(0, 0);
    world
        .apply_core_command_for_tests(SimCommand::PlaceUndergroundBelt {
            def_id: "basic_underground_belt".to_string(),
            entrance,
            exit: TilePos::new(4, 0),
            direction: Direction::East,
        })
        .unwrap();
    world.rebuild_transport_lines();
    world
        .apply_core_command_for_tests(SimCommand::DropItemOnBeltTile {
            pos: entrance,
            lane: 0,
            distance_numerator: 0,
            distance_denominator: 128,
            item: TEST_IRON_ORE,
        })
        .unwrap();

    for _ in 0..80 {
        tick_world_for_tests(&mut world);
        if !underground_runtime_items(&world).is_empty() {
            break;
        }
    }

    assert_eq!(underground_runtime_items(&world).len(), 1);
    let visible = SimRenderView::extract(&world, VisibleTileBounds::new(entrance, entrance));
    assert!(
        visible.visible_items.is_empty(),
        "ingressed item should be underground after reaching entrance front: {:?}",
        visible.visible_items
    );
}

#[test]
fn underground_item_traverses_node_with_distance_latency() {
    let mut world = SimWorld::with_catalog(catalog_for_tests());
    place_catalog_belt(
        &mut world,
        "basic_belt",
        TilePos::new(-1, 0),
        Direction::East,
    );
    world
        .apply_core_command_for_tests(SimCommand::PlaceUndergroundBelt {
            def_id: "basic_underground_belt".to_string(),
            entrance: TilePos::new(0, 0),
            exit: TilePos::new(4, 0),
            direction: Direction::East,
        })
        .unwrap();
    place_catalog_belt(
        &mut world,
        "basic_belt",
        TilePos::new(5, 0),
        Direction::East,
    );
    world.rebuild_transport_lines();

    world
        .apply_core_command_for_tests(SimCommand::DropItemOnBeltTile {
            pos: TilePos::new(0, 0),
            lane: 0,
            distance_numerator: 128,
            distance_denominator: 128,
            item: TEST_IRON_ORE,
        })
        .unwrap();

    for _ in 0..10 {
        tick_world_for_tests(&mut world);
    }
    assert_eq!(belt_items_at_tile(&world, TilePos::new(5, 0)), 0);

    for _ in 0..300 {
        tick_world_for_tests(&mut world);
        if belt_items_at_tile(&world, TilePos::new(4, 0)) > 0
            || belt_items_at_tile(&world, TilePos::new(5, 0)) > 0
        {
            return;
        }
    }
    panic!("item should exit underground node");
}

#[test]
fn underground_preserves_lane_at_exit() {
    let mut world = SimWorld::with_catalog(catalog_for_tests());
    world
        .apply_core_command_for_tests(SimCommand::PlaceUndergroundBelt {
            def_id: "basic_underground_belt".to_string(),
            entrance: TilePos::new(0, 0),
            exit: TilePos::new(3, 0),
            direction: Direction::East,
        })
        .unwrap();
    world.rebuild_transport_lines();
    world
        .apply_core_command_for_tests(SimCommand::DropItemOnBeltTile {
            pos: TilePos::new(0, 0),
            lane: 1,
            distance_numerator: 128,
            distance_denominator: 128,
            item: TEST_COPPER_ORE,
        })
        .unwrap();

    for _ in 0..200 {
        tick_world_for_tests(&mut world);
    }

    let (exit_line, _, _, _) = world.line_window_for_tile(TilePos::new(3, 0)).unwrap();
    assert_eq!(line_lane_items(&world, exit_line, 0), Vec::new());
    assert_eq!(line_lane_items(&world, exit_line, 1), vec![TEST_COPPER_ORE]);
}

#[test]
fn underground_blocked_output_lane_does_not_block_other_lane() {
    let mut world = SimWorld::with_catalog(catalog_for_tests());
    world
        .apply_core_command_for_tests(SimCommand::PlaceUndergroundBelt {
            def_id: "basic_underground_belt".to_string(),
            entrance: TilePos::new(0, 0),
            exit: TilePos::new(3, 0),
            direction: Direction::East,
        })
        .unwrap();
    world.rebuild_transport_lines();
    let (exit_line, _, _, _) = world.line_window_for_tile(TilePos::new(3, 0)).unwrap();
    world
        .apply_core_command_for_tests(SimCommand::DropItemOnBeltTile {
            pos: TilePos::new(3, 0),
            lane: 0,
            distance_numerator: 128,
            distance_denominator: 128,
            item: TEST_WOOD,
        })
        .unwrap();
    world
        .apply_core_command_for_tests(SimCommand::DropItemOnBeltTile {
            pos: TilePos::new(0, 0),
            lane: 0,
            distance_numerator: 128,
            distance_denominator: 128,
            item: TEST_IRON_ORE,
        })
        .unwrap();
    world
        .apply_core_command_for_tests(SimCommand::DropItemOnBeltTile {
            pos: TilePos::new(0, 0),
            lane: 1,
            distance_numerator: 128,
            distance_denominator: 128,
            item: TEST_COPPER_ORE,
        })
        .unwrap();

    for _ in 0..200 {
        tick_world_for_tests(&mut world);
    }

    let lane_0_items = line_lane_items(&world, exit_line, 0);
    let runtime_items = underground_runtime_items(&world);
    assert!(lane_0_items.contains(&TEST_WOOD));
    assert!(
        lane_0_items.contains(&TEST_IRON_ORE)
            || runtime_items
                .iter()
                .any(|item| item.lane == 0 && item.item == TEST_IRON_ORE)
    );
    assert!(line_lane_items(&world, exit_line, 1).contains(&TEST_COPPER_ORE));
}

#[test]
fn underground_transit_latency_scales_with_gap() {
    let mut short_world = SimWorld::with_catalog(catalog_for_tests());
    let (short_entrance, short_exit) = place_underground_pair_with_gap(&mut short_world, 1);
    short_world
        .apply_core_command_for_tests(SimCommand::DropItemOnBeltTile {
            pos: short_entrance,
            lane: 0,
            distance_numerator: 128,
            distance_denominator: 128,
            item: TEST_IRON_ORE,
        })
        .unwrap();
    let short_latency =
        ticks_until_item_at_tile(&mut short_world, short_exit, 500).expect("short tunnel");

    let mut long_world = SimWorld::with_catalog(catalog_for_tests());
    let (long_entrance, long_exit) = place_underground_pair_with_gap(&mut long_world, 4);
    long_world
        .apply_core_command_for_tests(SimCommand::DropItemOnBeltTile {
            pos: long_entrance,
            lane: 0,
            distance_numerator: 128,
            distance_denominator: 128,
            item: TEST_IRON_ORE,
        })
        .unwrap();
    let long_latency =
        ticks_until_item_at_tile(&mut long_world, long_exit, 500).expect("long tunnel");

    let min_extra = usize::try_from(3 * DistanceUnits::UNITS_PER_TILE / 4).unwrap_or(96);
    assert!(
        long_latency >= short_latency + min_extra,
        "long tunnel latency {long_latency} should exceed short {short_latency} by at least {min_extra}"
    );
}

#[test]
fn underground_blocked_exit_backs_up_through_tunnel() {
    let mut world = SimWorld::with_catalog(catalog_for_tests());
    let (entrance, exit) = place_underground_pair_with_gap(&mut world, 4);
    world.rebuild_transport_lines();

    for _ in 0..8 {
        drop_item_on_underground_entrance(&mut world, entrance);
        for _ in 0..80 {
            tick_world_for_tests(&mut world);
        }
    }

    let (exit_line, _, _, _) = world.line_window_for_tile(exit).unwrap();
    let exit_line_items = total_items_on_line(&world, exit_line);
    let runtime_items = underground_runtime_items(&world).len();
    assert!(
        exit_line_items > 0,
        "blocked exit should back up items on the exit line"
    );
    assert!(
        runtime_items > 0,
        "blocked exit should retain items in underground runtime"
    );
    assert_eq!(
        belt_items_at_tile(&world, Direction::East.output_pos(exit)),
        0,
        "exit has no output belt so items should not leave the tunnel"
    );
}

#[test]
fn underground_blocked_tunnel_blocks_entrance_intake() {
    let mut world = SimWorld::with_catalog(catalog_for_tests());
    let (entrance, _) = place_underground_pair_with_gap(&mut world, 4);
    world.rebuild_transport_lines();

    drop_item_on_underground_entrance(&mut world, entrance);
    tick_world_for_tests(&mut world);
    assert_eq!(underground_runtime_items(&world).len(), 1);

    drop_item_on_underground_entrance(&mut world, entrance);
    tick_world_for_tests(&mut world);

    let (entrance_line, _, _, _) = world.line_window_for_tile(entrance).unwrap();
    assert_eq!(
        total_items_on_line(&world, entrance_line),
        1,
        "runtime item inside spacing window should block entrance intake"
    );
    assert!(
        underground_runtime_items(&world)
            .iter()
            .any(|item| item.lane == 0
                && item.progress
                    < DistanceUnits::new(DistanceUnits::UNITS_PER_TILE / 2) + MIN_ITEM_SPACING),
        "backpressure should be represented in node runtime"
    );
}

#[test]
fn underground_runtime_advance_preserves_same_lane_spacing() {
    let mut world = SimWorld::with_catalog(catalog_for_tests());
    world
        .apply_core_command_for_tests(SimCommand::PlaceUndergroundBelt {
            def_id: "basic_underground_belt".to_string(),
            entrance: TilePos::new(0, 0),
            exit: TilePos::new(4, 0),
            direction: Direction::East,
        })
        .unwrap();
    world.rebuild_transport_lines();

    let node_id = world
        .transport
        .nodes_sorted()
        .find(|node| node.kind == TransportNodeKind::Underground)
        .unwrap()
        .id;
    let runtime = world.transport.underground_runtime_mut(node_id).unwrap();
    runtime.items.push(UndergroundTransportItem {
        item: TEST_IRON_ORE,
        lane: 0,
        progress: runtime.distance - DistanceUnits::new(1),
    });
    runtime.items.push(UndergroundTransportItem {
        item: TEST_COPPER_ORE,
        lane: 0,
        progress: runtime.distance - DistanceUnits::new(2),
    });

    tick_world_for_tests(&mut world);

    let mut progress = underground_runtime_items(&world)
        .into_iter()
        .filter(|item| item.lane == 0)
        .map(|item| item.progress)
        .collect::<Vec<_>>();
    progress.sort();

    assert_eq!(progress.len(), 2);
    assert!(
        progress[1] - progress[0] >= MIN_ITEM_SPACING,
        "same-lane underground progress should remain spaced: {progress:?}"
    );
}

#[test]
fn underground_blocked_one_output_lane_does_not_stop_other_lane() {
    let mut world = SimWorld::with_catalog(catalog_for_tests());
    world
        .apply_core_command_for_tests(SimCommand::PlaceUndergroundBelt {
            def_id: "basic_underground_belt".to_string(),
            entrance: TilePos::new(0, 0),
            exit: TilePos::new(4, 0),
            direction: Direction::East,
        })
        .unwrap();
    world.rebuild_transport_lines();

    let node_id = world
        .transport
        .nodes_sorted()
        .find(|node| node.kind == TransportNodeKind::Underground)
        .unwrap()
        .id;
    let runtime = world.transport.underground_runtime_mut(node_id).unwrap();
    runtime.items.push(UndergroundTransportItem {
        item: TEST_IRON_ORE,
        lane: 0,
        progress: runtime.distance,
    });
    runtime.items.push(UndergroundTransportItem {
        item: TEST_COPPER_ORE,
        lane: 1,
        progress: runtime.distance,
    });

    let (exit_line, _, _, _) = world.line_window_for_tile(TilePos::new(4, 0)).unwrap();
    assert!(
        world
            .transport
            .line_mut(exit_line)
            .unwrap()
            .insert_item_at_entry_boundary(0, TEST_WOOD)
    );

    tick_world_for_tests(&mut world);

    let runtime_items = underground_runtime_items(&world);
    assert_eq!(line_lane_items(&world, exit_line, 1), vec![TEST_COPPER_ORE]);
    assert!(
        runtime_items
            .iter()
            .any(|item| item.lane == 0 && item.item == TEST_IRON_ORE)
    );
    assert!(!runtime_items.iter().any(|item| item.lane == 1));
}

#[test]
fn place_underground_single_solo_when_no_partner() {
    let mut world = SimWorld::with_catalog(catalog_for_tests());
    world
        .apply_core_command_for_tests(SimCommand::PlaceUnderground {
            def_id: "basic_underground_belt".to_string(),
            pos: TilePos::new(0, 0),
            direction: Direction::East,
        })
        .unwrap();

    let building = world.building_at(TilePos::new(0, 0)).unwrap();
    let SimBuildingState::Underground(runtime) = &building.state else {
        panic!("expected underground");
    };
    assert_eq!(runtime.partner, building.id);
    assert_eq!(world.transport.line_ids_sorted().count(), 1);
    assert_eq!(
        world
            .transport
            .line(world.transport.line_ids_sorted().next().unwrap())
            .unwrap()
            .path()
            .tiles()
            .len(),
        1
    );
}

#[test]
fn place_underground_single_auto_pairs_to_existing_solo() {
    let mut world = SimWorld::with_catalog(catalog_for_tests());
    world
        .apply_core_command_for_tests(SimCommand::PlaceUnderground {
            def_id: "basic_underground_belt".to_string(),
            pos: TilePos::new(0, 0),
            direction: Direction::East,
        })
        .unwrap();
    world
        .apply_core_command_for_tests(SimCommand::PlaceUnderground {
            def_id: "basic_underground_belt".to_string(),
            pos: TilePos::new(3, 0),
            direction: Direction::East,
        })
        .unwrap();

    assert_eq!(world.transport.line_ids_sorted().count(), 2);
    assert_eq!(
        world
            .transport
            .nodes_sorted()
            .filter(|node| node.kind == TransportNodeKind::Underground)
            .count(),
        1
    );
    for line_id in world.transport.line_ids_sorted() {
        let line = world.transport.line(line_id).unwrap();
        assert_eq!(line.path().tiles().len(), 1);
    }
}

#[test]
fn upstream_underground_endpoint_first_pairs_with_correct_roles_and_flow() {
    let mut world = SimWorld::with_catalog(catalog_for_tests());
    world
        .apply_core_command_for_tests(SimCommand::PlaceUnderground {
            def_id: "basic_underground_belt".to_string(),
            pos: TilePos::new(0, 0),
            direction: Direction::East,
        })
        .unwrap();
    world
        .apply_core_command_for_tests(SimCommand::PlaceUnderground {
            def_id: "basic_underground_belt".to_string(),
            pos: TilePos::new(4, 0),
            direction: Direction::East,
        })
        .unwrap();

    let entrance = world.building_at(TilePos::new(0, 0)).unwrap();
    let exit = world.building_at(TilePos::new(4, 0)).unwrap();
    let SimBuildingState::Underground(entrance_runtime) = &entrance.state else {
        panic!("expected underground entrance");
    };
    let SimBuildingState::Underground(exit_runtime) = &exit.state else {
        panic!("expected underground exit");
    };
    assert_eq!(entrance_runtime.role, UndergroundRole::Entrance);
    assert_eq!(exit_runtime.role, UndergroundRole::Exit);
    assert_eq!(entrance_runtime.partner, exit.id);
    assert_eq!(exit_runtime.partner, entrance.id);

    let node = world
        .transport
        .nodes_sorted()
        .find(|node| node.kind == TransportNodeKind::Underground)
        .unwrap();
    assert!(
        node.input_ports()
            .all(|port| port.tile == TilePos::new(0, 0))
    );
    assert!(
        node.output_ports()
            .all(|port| port.tile == TilePos::new(4, 0))
    );

    world
        .apply_core_command_for_tests(SimCommand::PlaceBelt {
            pos: TilePos::new(5, 0),
            direction: Direction::East,
            input_direction: Direction::East,
            speed: UnitsPerTick::new(4),
        })
        .unwrap();
    world
        .apply_core_command_for_tests(SimCommand::DropItemOnBeltTile {
            pos: TilePos::new(0, 0),
            lane: 0,
            distance_numerator: 128,
            distance_denominator: 128,
            item: TEST_IRON_ORE,
        })
        .unwrap();

    for _ in 0..600 {
        tick_world_for_tests(&mut world);
        if tile_has_item_in_line_window(&world, TilePos::new(4, 0))
            || belt_items_at_tile(&world, TilePos::new(5, 0)) > 0
        {
            return;
        }
    }
    panic!("item should flow through upstream-first independent underground pair");
}

#[test]
fn paired_underground_builds_separate_surface_lines_and_transport_node() {
    let mut world = SimWorld::with_catalog(catalog_for_tests());
    world
        .apply_core_command_for_tests(SimCommand::PlaceUndergroundBelt {
            def_id: "basic_underground_belt".to_string(),
            entrance: TilePos::new(0, 0),
            exit: TilePos::new(4, 0),
            direction: Direction::East,
        })
        .unwrap();
    world.rebuild_transport_lines();

    let line_ids = world.transport.line_ids_sorted().collect::<Vec<_>>();
    assert_eq!(line_ids.len(), 2);
    assert_ne!(
        world.line_window_for_tile(TilePos::new(0, 0)).unwrap().0,
        world.line_window_for_tile(TilePos::new(4, 0)).unwrap().0
    );

    let underground_nodes = world
        .transport
        .nodes_sorted()
        .filter(|node| node.kind == TransportNodeKind::Underground)
        .collect::<Vec<_>>();
    assert_eq!(underground_nodes.len(), 1);
    assert_eq!(underground_nodes[0].input_ports().count(), 2);
    assert_eq!(underground_nodes[0].output_ports().count(), 2);
}

#[test]
fn underground_node_input_ignores_side_line_blocked_at_entrance() {
    let mut world = SimWorld::with_catalog(catalog_for_tests());
    world
        .apply_core_command_for_tests(SimCommand::PlaceUndergroundBelt {
            def_id: "basic_underground_belt".to_string(),
            entrance: TilePos::new(0, 0),
            exit: TilePos::new(4, 0),
            direction: Direction::East,
        })
        .unwrap();
    world
        .apply_core_command_for_tests(SimCommand::PlaceBelt {
            pos: TilePos::new(0, -1),
            direction: Direction::North,
            input_direction: Direction::North,
            speed: UnitsPerTick::new(4),
        })
        .unwrap();
    world.rebuild_transport_lines();

    let entrance_line = world.line_window_for_tile(TilePos::new(0, 0)).unwrap().0;
    let side_line = world.line_window_for_tile(TilePos::new(0, -1)).unwrap().0;
    assert_ne!(entrance_line, side_line);

    let underground_node = world
        .transport
        .nodes_sorted()
        .find(|node| node.kind == TransportNodeKind::Underground)
        .unwrap();
    let underground_input_lines = underground_node
        .input_ports()
        .map(|port| port.line)
        .collect::<BTreeSet<_>>();
    assert_eq!(underground_input_lines, BTreeSet::from([entrance_line]));
    assert!(!underground_input_lines.contains(&side_line));

    let side_source = world.transport.line(side_line).unwrap();
    assert_eq!(side_source.path().tiles().len(), 1);
    assert_eq!(side_source.path().tiles()[0].pos, TilePos::new(0, -1));
    assert!(world.transport.nodes_sorted().any(|node| {
        node.kind == TransportNodeKind::BlockedFront
            && node.input_ports().any(|port| port.line == side_line)
    }));
}

#[test]
fn underground_transport_runtime_survives_rebuild_by_identity() {
    let mut world = SimWorld::with_catalog(catalog_for_tests());
    world
        .apply_core_command_for_tests(SimCommand::PlaceUndergroundBelt {
            def_id: "basic_underground_belt".to_string(),
            entrance: TilePos::new(0, 0),
            exit: TilePos::new(4, 0),
            direction: Direction::East,
        })
        .unwrap();
    world.rebuild_transport_lines();

    let node_id = world
        .transport
        .nodes_sorted()
        .find(|node| node.kind == TransportNodeKind::Underground)
        .unwrap()
        .id;
    let runtime = world.transport.underground_runtime_mut(node_id).unwrap();
    runtime.items.push(UndergroundTransportItem {
        item: TEST_IRON_ORE,
        lane: 1,
        progress: DistanceUnits::new(42),
    });
    let expected = runtime.clone();

    world.rebuild_transport_lines();

    let restored = world
        .transport
        .nodes_sorted()
        .find_map(|node| match &node.runtime {
            TransportNodeRuntime::Underground(runtime) => Some(runtime),
            _ => None,
        })
        .unwrap();
    assert_eq!(restored, &expected);
}

#[test]
fn underground_internal_progress_marks_source_and_target_lines_changed() {
    let mut world = SimWorld::with_catalog(catalog_for_tests());
    world
        .apply_core_command_for_tests(SimCommand::PlaceUndergroundBelt {
            def_id: "basic_underground_belt".to_string(),
            entrance: TilePos::new(0, 0),
            exit: TilePos::new(4, 0),
            direction: Direction::East,
        })
        .unwrap();
    world.rebuild_transport_lines();

    let node = world
        .transport
        .nodes_sorted()
        .find(|node| node.kind == TransportNodeKind::Underground)
        .unwrap()
        .clone();
    let port_lines = node
        .ports
        .iter()
        .map(|port| port.line)
        .collect::<BTreeSet<_>>();
    let runtime = world.transport.underground_runtime_mut(node.id).unwrap();
    runtime.items.push(UndergroundTransportItem {
        item: TEST_IRON_ORE,
        lane: 0,
        progress: DistanceUnits::new(128),
    });

    let output = tick_world_for_tests(&mut world);
    let changed_lines = output
        .diff
        .changed_lines
        .into_iter()
        .collect::<BTreeSet<_>>();

    assert!(port_lines.is_subset(&changed_lines));
}

#[test]
fn underground_ingress_marks_source_and_target_lines_changed() {
    let mut world = SimWorld::with_catalog(catalog_for_tests());
    world
        .apply_core_command_for_tests(SimCommand::PlaceUndergroundBelt {
            def_id: "basic_underground_belt".to_string(),
            entrance: TilePos::new(0, 0),
            exit: TilePos::new(4, 0),
            direction: Direction::East,
        })
        .unwrap();
    world.rebuild_transport_lines();

    let node = world
        .transport
        .nodes_sorted()
        .find(|node| node.kind == TransportNodeKind::Underground)
        .unwrap()
        .clone();
    let port_lines = node
        .ports
        .iter()
        .map(|port| port.line)
        .collect::<BTreeSet<_>>();
    drop_item_on_underground_entrance(&mut world, TilePos::new(0, 0));

    let output = tick_world_for_tests(&mut world);
    let changed_lines = output
        .diff
        .changed_lines
        .into_iter()
        .collect::<BTreeSet<_>>();

    assert!(port_lines.is_subset(&changed_lines));
}

#[test]
fn underground_egress_marks_source_and_target_lines_changed() {
    let mut world = SimWorld::with_catalog(catalog_for_tests());
    world
        .apply_core_command_for_tests(SimCommand::PlaceUndergroundBelt {
            def_id: "basic_underground_belt".to_string(),
            entrance: TilePos::new(0, 0),
            exit: TilePos::new(4, 0),
            direction: Direction::East,
        })
        .unwrap();
    world.rebuild_transport_lines();

    let node = world
        .transport
        .nodes_sorted()
        .find(|node| node.kind == TransportNodeKind::Underground)
        .unwrap()
        .clone();
    let port_lines = node
        .ports
        .iter()
        .map(|port| port.line)
        .collect::<BTreeSet<_>>();
    let runtime = world.transport.underground_runtime_mut(node.id).unwrap();
    runtime.items.push(UndergroundTransportItem {
        item: TEST_IRON_ORE,
        lane: 0,
        progress: runtime.distance,
    });

    let output = tick_world_for_tests(&mut world);
    let changed_lines = output
        .diff
        .changed_lines
        .into_iter()
        .collect::<BTreeSet<_>>();

    assert!(port_lines.is_subset(&changed_lines));
}

#[test]
fn downstream_underground_endpoint_can_be_placed_before_entrance() {
    let mut world = SimWorld::with_catalog(catalog_for_tests());
    world
        .apply_core_command_for_tests(SimCommand::PlaceUnderground {
            def_id: "basic_underground_belt".to_string(),
            pos: TilePos::new(4, 0),
            direction: Direction::East,
        })
        .unwrap();
    world
        .apply_core_command_for_tests(SimCommand::PlaceBelt {
            pos: TilePos::new(2, 0),
            direction: Direction::North,
            input_direction: Direction::North,
            speed: UnitsPerTick::new(4),
        })
        .unwrap();
    world
        .apply_core_command_for_tests(SimCommand::PlaceUnderground {
            def_id: "basic_underground_belt".to_string(),
            pos: TilePos::new(0, 0),
            direction: Direction::East,
        })
        .unwrap();

    let entrance = world.building_at(TilePos::new(0, 0)).unwrap();
    let exit = world.building_at(TilePos::new(4, 0)).unwrap();
    let SimBuildingState::Underground(entrance_runtime) = &entrance.state else {
        panic!("expected underground entrance");
    };
    let SimBuildingState::Underground(exit_runtime) = &exit.state else {
        panic!("expected underground exit");
    };
    assert_eq!(entrance_runtime.role, UndergroundRole::Entrance);
    assert_eq!(exit_runtime.role, UndergroundRole::Exit);
    assert_eq!(entrance_runtime.partner, exit.id);
    assert_eq!(exit_runtime.partner, entrance.id);
}

#[test]
fn place_underground_outside_range_does_not_pair() {
    let mut world = SimWorld::with_catalog(catalog_for_tests());
    world
        .apply_core_command_for_tests(SimCommand::PlaceUnderground {
            def_id: "basic_underground_belt".to_string(),
            pos: TilePos::new(0, 0),
            direction: Direction::East,
        })
        .unwrap();
    world
        .apply_core_command_for_tests(SimCommand::PlaceUnderground {
            def_id: "basic_underground_belt".to_string(),
            pos: TilePos::new(10, 0),
            direction: Direction::East,
        })
        .unwrap();

    assert_eq!(world.transport.line_ids_sorted().count(), 2);
}

#[test]
fn rotate_solo_underground_flips_direction() {
    let mut world = SimWorld::with_catalog(catalog_for_tests());
    world
        .apply_core_command_for_tests(SimCommand::PlaceUnderground {
            def_id: "basic_underground_belt".to_string(),
            pos: TilePos::new(0, 0),
            direction: Direction::East,
        })
        .unwrap();

    world
        .apply_core_command_for_tests(SimCommand::RotateUnderground {
            pos: TilePos::new(0, 0),
        })
        .unwrap();

    let building = world.building_at(TilePos::new(0, 0)).unwrap();
    assert_eq!(building.direction, Direction::West);
}

#[test]
fn rotate_paired_underground_unpairs_and_repairs() {
    let mut world = SimWorld::with_catalog(catalog_for_tests());
    world
        .apply_core_command_for_tests(SimCommand::PlaceUndergroundBelt {
            def_id: "basic_underground_belt".to_string(),
            entrance: TilePos::new(0, 0),
            exit: TilePos::new(4, 0),
            direction: Direction::East,
        })
        .unwrap();

    world
        .apply_core_command_for_tests(SimCommand::RotateUnderground {
            pos: TilePos::new(0, 0),
        })
        .unwrap();

    let entrance = world.building_at(TilePos::new(0, 0)).unwrap();
    let SimBuildingState::Underground(entrance_runtime) = &entrance.state else {
        panic!("expected underground");
    };
    assert_eq!(entrance_runtime.partner, entrance.id);
    assert_eq!(world.transport.line_ids_sorted().count(), 2);
}

#[test]
fn side_load_onto_entrance_tunnel_half_is_blocked() {
    let mut world = SimWorld::with_catalog(catalog_for_tests());
    world
        .apply_core_command_for_tests(SimCommand::PlaceUnderground {
            def_id: "basic_underground_belt".to_string(),
            pos: TilePos::new(0, 0),
            direction: Direction::East,
        })
        .unwrap();
    world
        .apply_core_command_for_tests(SimCommand::PlaceBelt {
            pos: TilePos::new(1, 0),
            direction: Direction::West,
            input_direction: Direction::West,
            speed: UnitsPerTick::new(4),
        })
        .unwrap();
    world.rebuild_transport_lines();

    let interactions = world
        .transport
        .interactions_sorted()
        .map(|interaction| interaction.kind())
        .collect::<Vec<_>>();
    assert!(
        !interactions
            .iter()
            .any(|kind| matches!(kind, BeltInteractionKind::SideLoad { .. })),
        "tunnel half of entrance should block side load"
    );
}

#[test]
fn side_load_onto_entrance_belt_half_succeeds() {
    let mut world = SimWorld::with_catalog(catalog_for_tests());
    world
        .apply_core_command_for_tests(SimCommand::PlaceUnderground {
            def_id: "basic_underground_belt".to_string(),
            pos: TilePos::new(0, 0),
            direction: Direction::East,
        })
        .unwrap();
    world
        .apply_core_command_for_tests(SimCommand::PlaceBelt {
            pos: TilePos::new(-1, 0),
            direction: Direction::East,
            input_direction: Direction::East,
            speed: UnitsPerTick::new(4),
        })
        .unwrap();
    world.rebuild_transport_lines();
    world
        .apply_core_command_for_tests(SimCommand::DropItemOnBeltTile {
            pos: TilePos::new(-1, 0),
            lane: 0,
            distance_numerator: 128,
            distance_denominator: 128,
            item: TEST_IRON_ORE,
        })
        .unwrap();
    for _ in 0..50 {
        tick_world_for_tests(&mut world);
    }
    assert!(
        tile_has_item_in_line_window(&world, TilePos::new(0, 0)),
        "belt half behind entrance should accept input"
    );
}

#[test]
fn placing_same_tier_underground_over_existing_corridor_fails() {
    let mut world = SimWorld::with_catalog(catalog_for_tests());
    world
        .apply_core_command_for_tests(SimCommand::PlaceUndergroundBelt {
            def_id: "basic_underground_belt".to_string(),
            entrance: TilePos::new(0, 0),
            exit: TilePos::new(4, 0),
            direction: Direction::East,
        })
        .unwrap();

    assert!(matches!(
        world
            .apply_core_command_for_tests(SimCommand::PlaceUndergroundBelt {
                def_id: "basic_underground_belt".to_string(),
                entrance: TilePos::new(1, 0),
                exit: TilePos::new(3, 0),
                direction: Direction::East,
            })
            .unwrap_err(),
        SimCommandError::TopologyConflict { .. }
    ));
}

#[test]
fn failed_independent_underground_pairing_rolls_back_new_endpoint() {
    let mut world = SimWorld::with_catalog(catalog_for_tests());
    world
        .apply_core_command_for_tests(SimCommand::PlaceUndergroundBelt {
            def_id: "basic_underground_belt".to_string(),
            entrance: TilePos::new(0, 0),
            exit: TilePos::new(4, 0),
            direction: Direction::East,
        })
        .unwrap();
    world
        .apply_core_command_for_tests(SimCommand::PlaceUnderground {
            def_id: "basic_underground_belt".to_string(),
            pos: TilePos::new(1, 0),
            direction: Direction::East,
        })
        .unwrap();

    assert!(matches!(
        world
            .apply_core_command_for_tests(SimCommand::PlaceUnderground {
                def_id: "basic_underground_belt".to_string(),
                pos: TilePos::new(3, 0),
                direction: Direction::East,
            })
            .unwrap_err(),
        SimCommandError::TopologyConflict { .. }
    ));

    assert!(world.building_at(TilePos::new(3, 0)).is_none());
    assert!(!world.is_occupied_for_tests(TilePos::new(3, 0)));
    assert!(world.building_at(TilePos::new(1, 0)).is_some());
    assert_eq!(
        world
            .transport
            .nodes_sorted()
            .filter(|node| node.kind == TransportNodeKind::Underground)
            .count(),
        1
    );
}

#[test]
fn cross_tier_underground_over_corridor_succeeds() {
    let mut world = SimWorld::with_catalog(catalog_for_tests());
    world
        .apply_core_command_for_tests(SimCommand::PlaceUndergroundBelt {
            def_id: "basic_underground_belt".to_string(),
            entrance: TilePos::new(0, 0),
            exit: TilePos::new(4, 0),
            direction: Direction::East,
        })
        .unwrap();

    world
        .apply_core_command_for_tests(SimCommand::PlaceUndergroundBelt {
            def_id: "fast_underground_belt".to_string(),
            entrance: TilePos::new(1, 0),
            exit: TilePos::new(3, 0),
            direction: Direction::East,
        })
        .unwrap();
}

#[test]
fn surface_building_above_corridor_succeeds() {
    let mut world = SimWorld::with_catalog(catalog_for_tests());
    world
        .apply_core_command_for_tests(SimCommand::PlaceUndergroundBelt {
            def_id: "basic_underground_belt".to_string(),
            entrance: TilePos::new(0, 0),
            exit: TilePos::new(4, 0),
            direction: Direction::East,
        })
        .unwrap();

    world
        .apply_core_command_for_tests(SimCommand::PlaceBelt {
            pos: TilePos::new(2, 0),
            direction: Direction::North,
            input_direction: Direction::North,
            speed: UnitsPerTick::new(4),
        })
        .unwrap();
    assert!(world.occupied_tiles.contains_key(&TilePos::new(2, 0)));
}

#[test]
fn surface_belt_can_occupy_tile_above_underground_corridor() {
    let mut world = SimWorld::with_catalog(catalog_for_tests());
    world
        .apply_core_command_for_tests(SimCommand::PlaceUndergroundBelt {
            def_id: "basic_underground_belt".to_string(),
            entrance: TilePos::new(0, 0),
            exit: TilePos::new(4, 0),
            direction: Direction::East,
        })
        .unwrap();
    world
        .apply_core_command_for_tests(SimCommand::PlaceBelt {
            pos: TilePos::new(2, 0),
            direction: Direction::North,
            input_direction: Direction::North,
            speed: UnitsPerTick::new(4),
        })
        .unwrap();

    let corridor_tile = TilePos::new(2, 0);
    let (surface_line, _, _, _) = world.line_window_for_tile(corridor_tile).unwrap();
    let surface_line_len = world
        .transport
        .line(surface_line)
        .unwrap()
        .path()
        .tiles()
        .len();
    assert_eq!(
        surface_line_len, 1,
        "corridor tile should host its own surface belt line"
    );

    let (entrance_line, _, _, _) = world.line_window_for_tile(TilePos::new(0, 0)).unwrap();
    let (exit_line, _, _, _) = world.line_window_for_tile(TilePos::new(4, 0)).unwrap();
    assert_ne!(entrance_line, exit_line);
    assert_ne!(entrance_line, surface_line);
    assert_ne!(exit_line, surface_line);
}

#[test]
fn place_underground_pair_yields_surface_transport_lines_and_node() {
    let mut world = SimWorld::with_catalog(catalog_for_tests());
    world
        .apply_core_command_for_tests(SimCommand::PlaceUndergroundBelt {
            def_id: "basic_underground_belt".to_string(),
            entrance: TilePos::new(0, 0),
            exit: TilePos::new(4, 0),
            direction: Direction::East,
        })
        .unwrap();
    world.rebuild_transport_lines();

    let line_ids = world.transport.line_ids_sorted().collect::<Vec<_>>();
    assert_eq!(line_ids.len(), 2);
    for line_id in line_ids {
        let line = world.transport.line(line_id).unwrap();
        assert_eq!(line.path().tiles().len(), 1);
        assert!(line.path().tiles()[0].is_surface());
    }

    let (entrance_line, _, _, _) = world.line_window_for_tile(TilePos::new(0, 0)).unwrap();
    let (exit_line, _, _, _) = world.line_window_for_tile(TilePos::new(4, 0)).unwrap();
    assert_ne!(entrance_line, exit_line);
    assert_eq!(
        world
            .transport
            .nodes_sorted()
            .filter(|node| node.kind == TransportNodeKind::Underground)
            .count(),
        1
    );
}

#[test]
fn underground_rejects_invalid_distance() {
    let mut world = SimWorld::with_catalog(catalog_for_tests());
    let def_id = "basic_underground_belt".to_string();
    let entrance = TilePos::new(0, 0);

    assert!(matches!(
        world
            .apply_core_command_for_tests(SimCommand::PlaceUndergroundBelt {
                def_id: def_id.clone(),
                entrance,
                exit: entrance,
                direction: Direction::East,
            })
            .unwrap_err(),
        SimCommandError::InvalidPosition { .. }
    ));

    assert!(matches!(
        world
            .apply_core_command_for_tests(SimCommand::PlaceUndergroundBelt {
                def_id: def_id.clone(),
                entrance,
                exit: TilePos::new(1, 1),
                direction: Direction::East,
            })
            .unwrap_err(),
        SimCommandError::InvalidPosition { .. }
    ));

    assert!(matches!(
        world
            .apply_core_command_for_tests(SimCommand::PlaceUndergroundBelt {
                def_id: def_id.clone(),
                entrance,
                exit: TilePos::new(5, 0),
                direction: Direction::East,
            })
            .unwrap_err(),
        SimCommandError::InvalidPosition { .. }
    ));

    assert!(matches!(
        world
            .apply_core_command_for_tests(SimCommand::PlaceUndergroundBelt {
                def_id: def_id.clone(),
                entrance,
                exit: TilePos::new(10, 0),
                direction: Direction::East,
            })
            .unwrap_err(),
        SimCommandError::InvalidPosition { .. }
    ));
}

#[test]
fn underground_moves_item_entrance_to_exit() {
    let mut world = SimWorld::with_catalog(catalog_for_tests());
    place_catalog_belt(
        &mut world,
        "basic_belt",
        TilePos::new(-3, 0),
        Direction::East,
    );
    world
        .apply_core_command_for_tests(SimCommand::PlaceUndergroundBelt {
            def_id: "basic_underground_belt".to_string(),
            entrance: TilePos::new(-2, 0),
            exit: TilePos::new(2, 0),
            direction: Direction::East,
        })
        .unwrap();
    place_catalog_belt(
        &mut world,
        "basic_belt",
        TilePos::new(3, 0),
        Direction::East,
    );
    world.rebuild_transport_lines();

    world
        .apply_core_command_for_tests(SimCommand::DropItemOnBeltTile {
            pos: TilePos::new(-2, 0),
            lane: 0,
            distance_numerator: 128,
            distance_denominator: 128,
            item: TEST_IRON_ORE,
        })
        .unwrap();

    for _ in 0..600 {
        tick_world_for_tests(&mut world);
    }

    assert!(
        tile_has_item_in_line_window(&world, TilePos::new(2, 0))
            || belt_items_at_tile(&world, TilePos::new(3, 0)) > 0,
        "item should traverse underground tunnel to the exit side"
    );
}

#[test]
fn underground_remove_entrance_removes_exit() {
    let mut world = SimWorld::with_catalog(catalog_for_tests());
    world
        .apply_core_command_for_tests(SimCommand::PlaceUndergroundBelt {
            def_id: "basic_underground_belt".to_string(),
            entrance: TilePos::new(0, 0),
            exit: TilePos::new(3, 0),
            direction: Direction::East,
        })
        .unwrap();

    world
        .apply_core_command_for_tests(SimCommand::RemoveBuilding {
            pos: TilePos::new(0, 0),
        })
        .unwrap();

    assert!(world.building_at(TilePos::new(0, 0)).is_none());
    assert!(world.building_at(TilePos::new(3, 0)).is_none());
    assert!(world.line_window_for_tile(TilePos::new(0, 0)).is_none());
    assert!(world.line_window_for_tile(TilePos::new(3, 0)).is_none());
}

#[test]
fn removing_underground_pair_records_internal_item_drops() {
    let mut world = SimWorld::with_catalog(catalog_for_tests());
    world
        .apply_core_command_for_tests(SimCommand::PlaceUndergroundBelt {
            def_id: "basic_underground_belt".to_string(),
            entrance: TilePos::new(0, 0),
            exit: TilePos::new(3, 0),
            direction: Direction::East,
        })
        .unwrap();
    world.rebuild_transport_lines();
    world
        .apply_core_command_for_tests(SimCommand::DropItemOnBeltTile {
            pos: TilePos::new(0, 0),
            lane: 0,
            distance_numerator: 128,
            distance_denominator: 128,
            item: TEST_IRON_ORE,
        })
        .unwrap();
    tick_world_for_tests(&mut world);

    let (entrance_line, _, _, _) = world.line_window_for_tile(TilePos::new(0, 0)).unwrap();
    assert_eq!(line_lane_items(&world, entrance_line, 0), Vec::new());
    let runtime_items = underground_runtime_items(&world);
    assert_eq!(runtime_items.len(), 1);
    assert_eq!(runtime_items[0].item, TEST_IRON_ORE);
    assert_eq!(runtime_items[0].lane, 0);

    world
        .apply_core_command_for_tests(SimCommand::RemoveBuilding {
            pos: TilePos::new(0, 0),
        })
        .unwrap();

    let drops = world.removed_item_drop_records_for_tests();
    assert_eq!(drops.len(), 1);
    assert_eq!(drops[0].origin, TilePos::new(0, 0));
    assert_eq!(
        drops[0].stack,
        CoreItemStack {
            kind: TEST_IRON_ORE,
            amount: 1,
        }
    );
    assert_eq!(drops[0].instance, None);
}

#[test]
fn underground_save_roundtrip() {
    let mut world = SimWorld::with_catalog(catalog_for_tests());
    world
        .apply_core_command_for_tests(SimCommand::PlaceUndergroundBelt {
            def_id: "basic_underground_belt".to_string(),
            entrance: TilePos::new(0, 0),
            exit: TilePos::new(2, 0),
            direction: Direction::East,
        })
        .unwrap();

    let buildings = world.building_snapshots();
    let entrance = buildings
        .iter()
        .find(|building| building.origin == TilePos::new(0, 0))
        .unwrap();
    let SimBuildingState::Underground(entrance_runtime) = &entrance.state else {
        panic!("expected underground entrance");
    };
    let exit_partner = entrance_runtime.partner;

    let snapshot = world.snapshot();
    let restored = SimWorld::from_snapshot(catalog_for_tests(), snapshot).unwrap();
    let restored_entrance = restored.buildings.get(&entrance.id).unwrap();
    let SimBuildingState::Underground(restored_entrance_runtime) = &restored_entrance.state else {
        panic!("expected underground entrance");
    };
    let restored_exit = restored.buildings.get(&exit_partner).unwrap();
    let SimBuildingState::Underground(restored_exit_runtime) = &restored_exit.state else {
        panic!("expected underground exit");
    };

    assert_eq!(restored_entrance_runtime.role, UndergroundRole::Entrance);
    assert_eq!(restored_exit_runtime.role, UndergroundRole::Exit);
    assert_eq!(restored_entrance_runtime.partner, exit_partner);
    assert_eq!(restored_exit_runtime.partner, entrance.id);
}
