//! Energy network solver and consumer priority regression tests.

use crate::catalog::{CoreBuildingBehavior, CoreBuildingDef, CoreBuildingKind, CoreCatalog};
use crate::command::SimCommand;
use crate::energy::solver::solve_energy;
use crate::energy::{
    ConsumerPowerDef, DEFAULT_POWER_CLASS, EnergyAmount, EnergyConsumerRuntime,
    EnergyConsumerState, EnergyEdge, EnergyNetwork, EnergyNode, EnergySourceRuntime, GeneratorMode,
    GeneratorPowerDef, PowerConnectionDef, PowerDef, PowerSensitivity, PowerUnits, StoragePowerDef,
    SuppliedRatio,
};
use crate::ids::{BuildingId, EnergyEdgeId, EnergyNodeId, TilePos};
use crate::topology::graph::Direction;
use crate::world::SimWorld;
use serde::Deserialize;

#[test]
fn power_units_are_integer_and_clamped_ratios_are_stable() {
    assert_eq!(PowerUnits::new(250).raw(), 250);
    assert_eq!(EnergyAmount::new(10_000).raw(), 10_000);
    assert_eq!(
        SuppliedRatio::from_parts(PowerUnits::new(25), PowerUnits::new(100)).ppm(),
        250_000
    );
    assert_eq!(
        SuppliedRatio::from_parts(PowerUnits::new(150), PowerUnits::new(100)).ppm(),
        1_000_000
    );
    assert_eq!(
        SuppliedRatio::from_parts(PowerUnits::ZERO, PowerUnits::ZERO).ppm(),
        1_000_000
    );
}

#[test]
fn deserialized_supplied_ratio_preserves_clamp_invariant() {
    let ratio = SuppliedRatio::deserialize(serde::de::value::SeqDeserializer::<
        _,
        serde::de::value::Error,
    >::new([1_000_001_u32].into_iter()))
    .unwrap();

    assert_eq!(ratio.ppm(), 1_000_000);
}

#[test]
fn building_def_can_describe_each_power_role() {
    let building = CoreBuildingDef {
        id: "electric_assembler".to_string(),
        kind: CoreBuildingKind::Machine,
        footprint: vec![(0, 0)],
        rotate_footprint: false,
        inputs: Vec::new(),
        outputs: Vec::new(),
        inventories: Vec::new(),
        inserter_deposit_limits: Vec::new(),
        behavior: crate::catalog::CoreBuildingBehavior::noop(""),
        power: PowerDef {
            connection: Some(PowerConnectionDef {
                coverage_radius_tiles: 6,
                connection_range_tiles: 12,
                edge_capacity: PowerUnits::new(1_000),
                loss_per_tile: PowerUnits::new(2),
                power_class: DEFAULT_POWER_CLASS.to_string(),
                input_power_classes: Vec::new(),
            }),
            generator: Some(GeneratorPowerDef {
                max_output: PowerUnits::new(600),
                initial_output: PowerUnits::new(600),
                mode: GeneratorMode::Constant,
            }),
            storage: Some(StoragePowerDef {
                capacity: EnergyAmount::new(10_000),
                max_charge: PowerUnits::new(200),
                max_discharge: PowerUnits::new(300),
                initial_charge: EnergyAmount::new(1_000),
            }),
            consumer: Some(ConsumerPowerDef {
                demand: PowerUnits::new(100),
                priority: 3,
                offline_below: SuppliedRatio::from_ppm(1),
                power_sensitivity: PowerSensitivity::Linear,
            }),
        },
    };

    assert!(building.power.is_consumer());
    assert!(building.power.is_electric());
    assert!(building.power.connection.is_some());
    assert!(building.power.generator.is_some());
    assert!(building.power.storage.is_some());
    assert!(building.power.consumer.is_some());
}

fn energy_catalog() -> CoreCatalog {
    CoreCatalog::new(
        Vec::new(),
        Vec::new(),
        vec![
            CoreBuildingDef {
                id: "pole".to_string(),
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
                        coverage_radius_tiles: 3,
                        connection_range_tiles: 8,
                        edge_capacity: PowerUnits::new(500),
                        loss_per_tile: PowerUnits::new(1),
                        power_class: DEFAULT_POWER_CLASS.to_string(),
                        input_power_classes: Vec::new(),
                    }),
                    generator: None,
                    storage: None,
                    consumer: None,
                },
            },
            CoreBuildingDef {
                id: "generator".to_string(),
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
                        coverage_radius_tiles: 0,
                        connection_range_tiles: 0,
                        edge_capacity: PowerUnits::new(500),
                        loss_per_tile: PowerUnits::new(1),
                        power_class: DEFAULT_POWER_CLASS.to_string(),
                        input_power_classes: Vec::new(),
                    }),
                    generator: Some(GeneratorPowerDef {
                        max_output: PowerUnits::new(300),
                        initial_output: PowerUnits::new(300),
                        mode: GeneratorMode::Constant,
                    }),
                    storage: None,
                    consumer: None,
                },
            },
            CoreBuildingDef {
                id: "consumer".to_string(),
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
                        edge_capacity: PowerUnits::new(500),
                        loss_per_tile: PowerUnits::new(1),
                        power_class: DEFAULT_POWER_CLASS.to_string(),
                        input_power_classes: Vec::new(),
                    }),
                    generator: None,
                    storage: None,
                    consumer: Some(ConsumerPowerDef {
                        demand: PowerUnits::new(100),
                        priority: 2,
                        offline_below: SuppliedRatio::ZERO,
                        power_sensitivity: PowerSensitivity::Linear,
                    }),
                },
            },
        ],
    )
}

fn pole_def(id: &str, edge_capacity: i64, loss_per_tile: i64) -> CoreBuildingDef {
    CoreBuildingDef {
        id: id.to_string(),
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
                coverage_radius_tiles: 5,
                connection_range_tiles: 8,
                edge_capacity: PowerUnits::new(edge_capacity),
                loss_per_tile: PowerUnits::new(loss_per_tile),
                power_class: DEFAULT_POWER_CLASS.to_string(),
                input_power_classes: Vec::new(),
            }),
            generator: None,
            storage: None,
            consumer: None,
        },
    }
}

fn pole_def_with_power_class(
    id: &str,
    edge_capacity: i64,
    loss_per_tile: i64,
    power_class: &str,
) -> CoreBuildingDef {
    let mut def = pole_def(id, edge_capacity, loss_per_tile);
    def.power.connection.as_mut().unwrap().power_class = power_class.to_string();
    def
}

fn pole_def_transforming_from_power_class(
    id: &str,
    own_class: &str,
    input_class: &str,
) -> CoreBuildingDef {
    let mut def = pole_def_with_power_class(id, 500, 0, own_class);
    def.power
        .connection
        .as_mut()
        .unwrap()
        .input_power_classes
        .push(input_class.to_string());
    def
}

fn generator_def(max_output: i64) -> CoreBuildingDef {
    CoreBuildingDef {
        id: "generator".to_string(),
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
                coverage_radius_tiles: 0,
                connection_range_tiles: 0,
                edge_capacity: PowerUnits::new(500),
                loss_per_tile: PowerUnits::ZERO,
                power_class: DEFAULT_POWER_CLASS.to_string(),
                input_power_classes: Vec::new(),
            }),
            generator: Some(GeneratorPowerDef {
                max_output: PowerUnits::new(max_output),
                initial_output: PowerUnits::new(max_output),
                mode: GeneratorMode::Constant,
            }),
            storage: None,
            consumer: None,
        },
    }
}

fn solar_generator_def(max_output: i64) -> CoreBuildingDef {
    let mut def = generator_def(max_output);
    def.id = "solar".to_string();
    let generator = def
        .power
        .generator
        .as_mut()
        .expect("generator_def creates generator power");
    generator.initial_output = PowerUnits::ZERO;
    generator.mode = GeneratorMode::Solar;
    def
}

fn generator_def_with_power_class(max_output: i64, power_class: &str) -> CoreBuildingDef {
    let mut def = generator_def(max_output);
    def.power.connection.as_mut().unwrap().power_class = power_class.to_string();
    def
}

fn consumer_def(id: &str, demand: i64, priority: u8) -> CoreBuildingDef {
    CoreBuildingDef {
        id: id.to_string(),
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
                edge_capacity: PowerUnits::new(500),
                loss_per_tile: PowerUnits::ZERO,
                power_class: DEFAULT_POWER_CLASS.to_string(),
                input_power_classes: Vec::new(),
            }),
            generator: None,
            storage: None,
            consumer: Some(ConsumerPowerDef {
                demand: PowerUnits::new(demand),
                priority,
                offline_below: SuppliedRatio::ZERO,
                power_sensitivity: PowerSensitivity::Linear,
            }),
        },
    }
}

fn consumer_def_with_power_class(
    id: &str,
    demand: i64,
    priority: u8,
    power_class: &str,
) -> CoreBuildingDef {
    let mut def = consumer_def(id, demand, priority);
    def.power.connection.as_mut().unwrap().power_class = power_class.to_string();
    def
}

fn sensitive_consumer_def(
    id: &str,
    demand: i64,
    priority: u8,
    offline_below: SuppliedRatio,
    power_sensitivity: PowerSensitivity,
) -> CoreBuildingDef {
    let mut def = consumer_def(id, demand, priority);
    let consumer = def
        .power
        .consumer
        .as_mut()
        .expect("consumer_def creates a consumer");
    consumer.offline_below = offline_below;
    consumer.power_sensitivity = power_sensitivity;
    def
}

fn energy_catalog_with_two_consumers(
    p1_demand: i64,
    p2_demand: i64,
    generator_output: i64,
) -> CoreCatalog {
    CoreCatalog::new(
        Vec::new(),
        Vec::new(),
        vec![
            pole_def("pole", 500, 0),
            generator_def(generator_output),
            consumer_def("consumer_p1", p1_demand, 1),
            consumer_def("consumer_p2", p2_demand, 2),
        ],
    )
}

fn energy_catalog_for_route_choice() -> CoreCatalog {
    CoreCatalog::new(
        Vec::new(),
        Vec::new(),
        vec![
            pole_def("pole_low_loss", 500, 1),
            pole_def("pole_high_loss", 500, 20),
            generator_def(100),
            consumer_def("consumer", 100, 1),
        ],
    )
}

fn energy_catalog_with_battery() -> CoreCatalog {
    CoreCatalog::new(
        Vec::new(),
        Vec::new(),
        vec![
            pole_def("pole", 1_000, 0),
            generator_def(700),
            CoreBuildingDef {
                id: "battery".to_string(),
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
                        coverage_radius_tiles: 0,
                        connection_range_tiles: 0,
                        edge_capacity: PowerUnits::new(1_000),
                        loss_per_tile: PowerUnits::ZERO,
                        power_class: DEFAULT_POWER_CLASS.to_string(),
                        input_power_classes: Vec::new(),
                    }),
                    generator: None,
                    storage: Some(StoragePowerDef {
                        capacity: EnergyAmount::new(1_000),
                        max_charge: PowerUnits::new(500),
                        max_discharge: PowerUnits::new(100),
                        initial_charge: EnergyAmount::new(100),
                    }),
                    consumer: None,
                },
            },
            consumer_def("consumer", 100, 1),
        ],
    )
}

fn place(world: &mut SimWorld, def_id: &str, x: i32, y: i32) {
    world
        .apply_core_command_for_tests(SimCommand::PlaceBuilding {
            def_id: def_id.to_string(),
            origin: TilePos::new(x, y),
            direction: Direction::East,
            inserter_drop_direction: None,
        })
        .unwrap();
}

#[test]
fn poles_connect_within_range_and_attach_covered_buildings() {
    let mut world = SimWorld::with_catalog(energy_catalog());
    place(&mut world, "pole", 0, 0);
    place(&mut world, "pole", 6, 0);
    place(&mut world, "generator", 0, 1);
    place(&mut world, "consumer", 6, 1);

    world.rebuild_energy_topology_for_tests();
    let view = world.energy_view_for_tests();

    assert_eq!(view.nodes.len(), 4);
    assert_eq!(view.edges.len(), 3);
    assert_eq!(view.unconnected_consumers.len(), 0);
}

#[test]
fn generator_outside_supply_coverage_does_not_attach_by_connection_range() {
    let catalog = CoreCatalog::new(
        Vec::new(),
        Vec::new(),
        vec![
            pole_def("pole", 500, 0),
            generator_def(300),
            consumer_def("consumer", 100, 1),
        ],
    );
    let mut world = SimWorld::with_catalog(catalog);
    place(&mut world, "pole", 0, 0);
    place(&mut world, "generator", 7, 0);
    place(&mut world, "consumer", 0, 1);

    world.tick_core_only_for_tests();
    let view = world.energy_view_for_tests();
    let consumer = view.consumer_for_def("consumer").unwrap();

    assert_eq!(view.edges.len(), 1);
    assert_eq!(consumer.supplied.raw(), 0);
    assert_eq!(consumer.state, EnergyConsumerState::Offline);
}

#[test]
fn pole_connection_range_matches_square_preview_distance() {
    let mut world = SimWorld::with_catalog(CoreCatalog::new(
        Vec::new(),
        Vec::new(),
        vec![pole_def("pole", 500, 0)],
    ));
    place(&mut world, "pole", 0, 0);
    place(&mut world, "pole", 8, 8);

    world.rebuild_energy_topology_for_tests();
    let view = world.energy_view_for_tests();

    assert_eq!(view.edges.len(), 1);
}

#[test]
fn poles_with_different_power_classes_do_not_connect_directly() {
    let mut world = SimWorld::with_catalog(CoreCatalog::new(
        Vec::new(),
        Vec::new(),
        vec![
            pole_def_with_power_class("lv_pole", 500, 0, "lv"),
            pole_def_with_power_class("hv_pole", 500, 0, "hv"),
        ],
    ));
    place(&mut world, "lv_pole", 0, 0);
    place(&mut world, "hv_pole", 1, 0);

    world.rebuild_energy_topology_for_tests();
    let view = world.energy_view_for_tests();

    assert_eq!(view.edges.len(), 0);
}

#[test]
fn bridge_connection_can_join_input_power_classes() {
    let mut world = SimWorld::with_catalog(CoreCatalog::new(
        Vec::new(),
        Vec::new(),
        vec![
            pole_def_with_power_class("hv_pole", 500, 0, "hv"),
            pole_def_transforming_from_power_class("lv_substation", "lv", "hv"),
            pole_def_with_power_class("lv_pole", 500, 0, "lv"),
        ],
    ));
    place(&mut world, "hv_pole", 0, 0);
    place(&mut world, "lv_substation", 1, 0);
    place(&mut world, "lv_pole", 2, 0);

    world.rebuild_energy_topology_for_tests();
    let view = world.energy_view_for_tests();

    assert_eq!(view.edges.len(), 2);
}

#[test]
fn bridge_connection_only_transforms_from_input_class_to_own_class() {
    let mut world = SimWorld::with_catalog(CoreCatalog::new(
        Vec::new(),
        Vec::new(),
        vec![
            pole_def_with_power_class("hv_pole", 500, 0, "hv"),
            pole_def_transforming_from_power_class("lv_substation", "lv", "hv"),
            pole_def_with_power_class("lv_pole", 500, 0, "lv"),
            generator_def_with_power_class(300, "lv"),
            consumer_def_with_power_class("hv_consumer", 100, 1, "hv"),
        ],
    ));
    place(&mut world, "hv_pole", 0, 0);
    place(&mut world, "lv_substation", 1, 0);
    place(&mut world, "lv_pole", 2, 0);
    place(&mut world, "generator", 2, 1);
    place(&mut world, "hv_consumer", 0, 1);

    world.tick_core_only_for_tests();
    let view = world.energy_view_for_tests();
    let consumer = view.consumer_for_def("hv_consumer").unwrap();

    assert_eq!(consumer.supplied.raw(), 0);
    assert_eq!(consumer.state, EnergyConsumerState::Offline);
}

#[test]
fn bridge_connection_transforms_from_input_class_to_own_class() {
    let mut world = SimWorld::with_catalog(CoreCatalog::new(
        Vec::new(),
        Vec::new(),
        vec![
            pole_def_with_power_class("hv_pole", 500, 0, "hv"),
            pole_def_transforming_from_power_class("lv_substation", "lv", "hv"),
            pole_def_with_power_class("lv_pole", 500, 0, "lv"),
            generator_def_with_power_class(300, "hv"),
            consumer_def_with_power_class("lv_consumer", 100, 1, "lv"),
        ],
    ));
    place(&mut world, "hv_pole", 0, 0);
    place(&mut world, "lv_substation", 1, 0);
    place(&mut world, "lv_pole", 2, 0);
    place(&mut world, "generator", 0, 1);
    place(&mut world, "lv_consumer", 2, 1);

    world.tick_core_only_for_tests();
    let view = world.energy_view_for_tests();
    let consumer = view.consumer_for_def("lv_consumer").unwrap();

    assert_eq!(consumer.supplied.raw(), 100);
    assert_eq!(consumer.state, EnergyConsumerState::Powered);
}

#[test]
fn uncovered_consumer_is_reported_unconnected() {
    let mut world = SimWorld::with_catalog(energy_catalog());
    place(&mut world, "pole", 0, 0);
    place(&mut world, "consumer", 5, 0);

    world.rebuild_energy_topology_for_tests();
    let view = world.energy_view_for_tests();

    assert_eq!(view.unconnected_consumers.len(), 1);
    assert_eq!(view.consumers[0].state, EnergyConsumerState::Offline);
}

#[test]
fn topology_rebuild_preserves_surviving_node_ids_and_does_not_reuse_edge_ids() {
    let mut world = SimWorld::with_catalog(energy_catalog());
    place(&mut world, "pole", 0, 0);
    place(&mut world, "pole", 6, 0);
    place(&mut world, "consumer", 6, 1);

    world.rebuild_energy_topology_for_tests();
    let first_view = world.energy_view_for_tests();
    let survivor_pole_node = first_view
        .nodes
        .iter()
        .find(|node| node.pos == TilePos::new(6, 0))
        .unwrap()
        .id;
    let survivor_consumer_node = first_view
        .nodes
        .iter()
        .find(|node| node.pos == TilePos::new(6, 1))
        .unwrap()
        .id;
    let max_first_edge_id = first_view.edges.iter().map(|edge| edge.id.0).max().unwrap();

    world
        .apply_core_command_for_tests(SimCommand::RemoveBuilding {
            pos: TilePos::new(0, 0),
        })
        .unwrap();
    world.rebuild_energy_topology_for_tests();
    let second_view = world.energy_view_for_tests();

    assert_eq!(
        second_view
            .nodes
            .iter()
            .find(|node| node.pos == TilePos::new(6, 0))
            .unwrap()
            .id,
        survivor_pole_node
    );
    assert_eq!(
        second_view
            .nodes
            .iter()
            .find(|node| node.pos == TilePos::new(6, 1))
            .unwrap()
            .id,
        survivor_consumer_node
    );
    assert!(
        second_view
            .edges
            .iter()
            .all(|edge| edge.id.0 > max_first_edge_id)
    );
}

#[test]
fn solver_serves_priority_one_before_priority_two() {
    let mut world = SimWorld::with_catalog(energy_catalog_with_two_consumers(100, 100, 150));
    place(&mut world, "pole", 0, 0);
    place(&mut world, "generator", 0, 1);
    place(&mut world, "consumer_p1", 1, 0);
    place(&mut world, "consumer_p2", 2, 0);

    world.tick_core_only_for_tests();
    let view = world.energy_view_for_tests();
    let p1 = view.consumer_for_def("consumer_p1").unwrap();
    let p2 = view.consumer_for_def("consumer_p2").unwrap();

    assert_eq!(p1.supplied.raw(), 100);
    assert_eq!(p1.state, EnergyConsumerState::Powered);
    assert_eq!(p2.supplied.raw(), 50);
    assert_eq!(p2.supplied_ratio.ppm(), 500_000);
}

#[test]
fn threshold_sensitive_consumer_runs_at_full_effective_power_above_cutoff() {
    let generator = BuildingId(1);
    let lab = BuildingId(2);
    let generator_node = EnergyNodeId(1);
    let lab_node = EnergyNodeId(2);
    let catalog = CoreCatalog::new(
        Vec::new(),
        Vec::new(),
        vec![
            generator_def(80),
            sensitive_consumer_def(
                "lab",
                100,
                1,
                SuppliedRatio::from_ppm(800_000),
                PowerSensitivity::Threshold,
            ),
        ],
    );
    let mut network = EnergyNetwork::default();
    for (id, building) in [(generator_node, generator), (lab_node, lab)] {
        network.nodes.insert(
            id,
            EnergyNode {
                id,
                pos: TilePos::new(id.0 as i32, 0),
                building: Some(building),
            },
        );
        network.node_by_building.insert(building, id);
    }
    network
        .building_def_by_id
        .insert(generator, "generator".to_string());
    network.building_def_by_id.insert(lab, "lab".to_string());
    network.sources.insert(
        generator,
        EnergySourceRuntime {
            building: generator,
            def_id: "generator".to_string(),
            max_output: PowerUnits::new(80),
            used_output: PowerUnits::ZERO,
        },
    );
    network.consumers.insert(
        lab,
        EnergyConsumerRuntime {
            building: lab,
            def_id: "lab".to_string(),
            demand: PowerUnits::new(100),
            supplied: PowerUnits::ZERO,
            supplied_ratio: SuppliedRatio::ZERO,
            effective_ratio: SuppliedRatio::ZERO,
            state: EnergyConsumerState::Offline,
        },
    );
    insert_test_edge(
        &mut network,
        EnergyEdgeId(1),
        generator_node,
        lab_node,
        100,
        0,
    );

    solve_energy(&mut network, &catalog);

    let consumer = &network.consumers[&lab];
    assert_eq!(consumer.supplied_ratio, SuppliedRatio::from_ppm(800_000));
    assert_eq!(consumer.effective_ratio, SuppliedRatio::FULL);
    assert_eq!(consumer.state, EnergyConsumerState::Degraded);
    let behavior_power = network.behavior_power_input(lab);
    assert_eq!(behavior_power.supplied_ratio_ppm, 1_000_000);
    assert!(!behavior_power.offline);
}

#[test]
fn solver_prefers_lower_loss_route_when_capacity_allows() {
    let mut world = SimWorld::with_catalog(energy_catalog_for_route_choice());
    place(&mut world, "pole_low_loss", 0, 0);
    place(&mut world, "pole_low_loss", 2, 0);
    place(&mut world, "pole_high_loss", 0, 4);
    place(&mut world, "pole_high_loss", 2, 4);
    place(&mut world, "generator", 0, 1);
    place(&mut world, "consumer", 2, 1);

    world.tick_core_only_for_tests();
    let view = world.energy_view_for_tests();

    assert!(view.total_losses.raw() > 0);
    assert!(
        view.edges
            .iter()
            .any(|edge| edge.current_flow.raw() > 0 && edge.loss_per_unit.raw() == 1)
    );
    assert!(
        view.edges
            .iter()
            .all(|edge| edge.loss_per_unit.raw() != 20 || edge.current_flow.raw() == 0)
    );
}

#[test]
fn battery_charges_from_surplus_and_discharges_during_deficit() {
    let mut world = SimWorld::with_catalog(energy_catalog_with_battery());
    place(&mut world, "pole", 0, 0);
    place(&mut world, "generator", 0, 1);
    place(&mut world, "battery", 1, 0);
    place(&mut world, "consumer", 2, 0);

    world.tick_core_only_for_tests();
    let charge_view = world.energy_view_for_tests();
    let after_charge = charge_view.battery_for_def("battery").unwrap();
    assert_eq!(after_charge.stored.raw(), 600);

    world
        .apply_core_command_for_tests(SimCommand::RemoveBuilding {
            pos: TilePos::new(0, 1),
        })
        .unwrap();
    world.tick_core_only_for_tests();
    let view = world.energy_view_for_tests();
    let consumer = view.consumer_for_def("consumer").unwrap();
    let battery = view.battery_for_def("battery").unwrap();

    assert_eq!(consumer.supplied.raw(), 100);
    assert_eq!(battery.stored.raw(), 500);
}

#[test]
fn solar_generator_uses_world_time_of_day() {
    let mut world = SimWorld::with_catalog(CoreCatalog::new(
        Vec::new(),
        Vec::new(),
        vec![
            pole_def("pole", 1_000, 0),
            solar_generator_def(100),
            consumer_def("consumer", 100, 1),
        ],
    ));
    place(&mut world, "pole", 0, 0);
    place(&mut world, "solar", 0, 1);
    place(&mut world, "consumer", 1, 0);

    world.set_time_of_day(crate::world::TimeOfDay::from_normalized(0.10).unwrap());
    world.tick_core_only_for_tests();
    let night_view = world.energy_view_for_tests();
    assert_eq!(
        night_view.source_for_def("solar").unwrap().max_output,
        PowerUnits::ZERO
    );
    assert_eq!(
        night_view.consumer_for_def("consumer").unwrap().state,
        EnergyConsumerState::Offline
    );

    world.set_time_of_day(crate::world::TimeOfDay::from_normalized(0.50).unwrap());
    world.tick_core_only_for_tests();
    let day_view = world.energy_view_for_tests();
    assert_eq!(
        day_view.source_for_def("solar").unwrap().max_output,
        PowerUnits::new(100)
    );
    assert_eq!(
        day_view.consumer_for_def("consumer").unwrap().state,
        EnergyConsumerState::Powered
    );
}

#[test]
fn snapshot_preserves_battery_charge_but_rebuilds_edge_flow() {
    let catalog = energy_catalog_with_battery();
    let mut world = SimWorld::with_catalog(catalog.clone());
    place(&mut world, "pole", 0, 0);
    place(&mut world, "generator", 0, 1);
    place(&mut world, "battery", 1, 0);
    place(&mut world, "consumer", 2, 0);

    world.tick_core_only_for_tests();
    let solved_view = world.energy_view_for_tests();
    assert_eq!(
        solved_view.battery_for_def("battery").unwrap().stored.raw(),
        600
    );
    assert!(
        solved_view
            .edges
            .iter()
            .any(|edge| edge.current_flow.raw() > 0)
    );

    let restored = SimWorld::from_snapshot(catalog, world.snapshot()).unwrap();
    let restored_view = restored.energy_view_for_tests();

    assert_eq!(
        restored_view
            .battery_for_def("battery")
            .unwrap()
            .stored
            .raw(),
        600
    );
    assert!(
        restored_view
            .edges
            .iter()
            .all(|edge| edge.current_flow.raw() == 0)
    );
}

#[test]
fn snapshot_digest_matches_restore_immediately_after_electric_placement() {
    let catalog = energy_catalog_with_battery();
    let mut world = SimWorld::with_catalog(catalog.clone());
    place(&mut world, "pole", 0, 0);
    place(&mut world, "battery", 1, 0);

    let source_digest = world.digest();
    let restored = SimWorld::from_snapshot(catalog, world.snapshot()).unwrap();

    assert_eq!(restored.digest(), source_digest);
    assert!(
        restored
            .energy_view_for_tests()
            .battery_for_def("battery")
            .is_some()
    );
}

#[test]
fn snapshot_digest_matches_restore_immediately_after_electric_removal() {
    let catalog = energy_catalog_with_battery();
    let mut world = SimWorld::with_catalog(catalog.clone());
    place(&mut world, "pole", 0, 0);
    place(&mut world, "battery", 1, 0);
    world.rebuild_energy_topology_for_tests();

    world
        .apply_core_command_for_tests(SimCommand::RemoveBuilding {
            pos: TilePos::new(1, 0),
        })
        .unwrap();

    let source_digest = world.digest();
    let restored = SimWorld::from_snapshot(catalog, world.snapshot()).unwrap();

    assert_eq!(restored.digest(), source_digest);
    assert!(
        restored
            .energy_view_for_tests()
            .battery_for_def("battery")
            .is_none()
    );
}

#[test]
fn digest_changes_when_authoritative_battery_charge_changes() {
    let catalog = energy_catalog_with_battery();
    let mut world = SimWorld::with_catalog(catalog.clone());
    place(&mut world, "pole", 0, 0);
    place(&mut world, "generator", 0, 1);
    place(&mut world, "battery", 1, 0);
    place(&mut world, "consumer", 2, 0);
    world.rebuild_energy_topology_for_tests();

    let base_snapshot = world.snapshot();
    let mut charged_snapshot = base_snapshot.clone();
    charged_snapshot
        .energy
        .storages
        .values_mut()
        .next()
        .unwrap()
        .stored = EnergyAmount::new(600);

    let base = SimWorld::from_snapshot(catalog.clone(), base_snapshot).unwrap();
    let charged = SimWorld::from_snapshot(catalog, charged_snapshot).unwrap();

    assert_ne!(
        base.energy_view_for_tests()
            .battery_for_def("battery")
            .unwrap()
            .stored,
        charged
            .energy_view_for_tests()
            .battery_for_def("battery")
            .unwrap()
            .stored
    );
    assert_ne!(base.digest(), charged.digest());
}

#[test]
fn digest_changes_when_authoritative_consumer_state_changes() {
    let catalog = energy_catalog_with_battery();
    let mut world = SimWorld::with_catalog(catalog.clone());
    place(&mut world, "pole", 0, 0);
    place(&mut world, "generator", 0, 1);
    place(&mut world, "battery", 1, 0);
    place(&mut world, "consumer", 2, 0);
    world.rebuild_energy_topology_for_tests();

    let base_snapshot = world.snapshot();
    let mut powered_snapshot = base_snapshot.clone();
    let consumer = powered_snapshot
        .energy
        .consumers
        .values_mut()
        .next()
        .unwrap();
    consumer.supplied = PowerUnits::new(100);
    consumer.supplied_ratio = SuppliedRatio::FULL;
    consumer.state = EnergyConsumerState::Powered;

    let base = SimWorld::from_snapshot(catalog.clone(), base_snapshot).unwrap();
    let powered = SimWorld::from_snapshot(catalog, powered_snapshot).unwrap();

    assert_ne!(
        base.energy_view_for_tests()
            .consumer_for_def("consumer")
            .unwrap()
            .state,
        powered
            .energy_view_for_tests()
            .consumer_for_def("consumer")
            .unwrap()
            .state
    );
    assert_ne!(base.digest(), powered.digest());
}

#[test]
fn digest_changes_when_battery_charge_changes_after_tick() {
    let mut world = SimWorld::with_catalog(energy_catalog_with_battery());
    place(&mut world, "pole", 0, 0);
    place(&mut world, "generator", 0, 1);
    place(&mut world, "battery", 1, 0);
    place(&mut world, "consumer", 2, 0);
    world.rebuild_energy_topology_for_tests();

    let before_digest = world.digest();
    let before_charge = world
        .energy_view_for_tests()
        .battery_for_def("battery")
        .unwrap()
        .stored;

    world.tick_core_only_for_tests();

    assert_ne!(
        before_charge,
        world
            .energy_view_for_tests()
            .battery_for_def("battery")
            .unwrap()
            .stored
    );
    assert_ne!(before_digest, world.digest());
}

#[test]
fn same_priority_bucket_reroutes_flexible_consumer_around_shared_bottleneck() {
    let source = BuildingId(1);
    let c1 = BuildingId(2);
    let c2 = BuildingId(3);
    let source_node = EnergyNodeId(1);
    let shared_node = EnergyNodeId(2);
    let alternate_node = EnergyNodeId(3);
    let c1_node = EnergyNodeId(4);
    let c2_node = EnergyNodeId(5);

    let catalog = CoreCatalog::new(
        Vec::new(),
        Vec::new(),
        vec![
            generator_def(200),
            consumer_def("consumer_c1", 100, 1),
            consumer_def("consumer_c2", 100, 1),
        ],
    );
    let mut network = EnergyNetwork::default();
    for (id, building) in [
        (source_node, Some(source)),
        (shared_node, None),
        (alternate_node, None),
        (c1_node, Some(c1)),
        (c2_node, Some(c2)),
    ] {
        network.nodes.insert(
            id,
            EnergyNode {
                id,
                pos: TilePos::new(id.0 as i32, 0),
                building,
            },
        );
    }
    network.node_by_building.insert(source, source_node);
    network.node_by_building.insert(c1, c1_node);
    network.node_by_building.insert(c2, c2_node);
    network
        .building_def_by_id
        .insert(source, "generator".to_string());
    network
        .building_def_by_id
        .insert(c1, "consumer_c1".to_string());
    network
        .building_def_by_id
        .insert(c2, "consumer_c2".to_string());
    network.sources.insert(
        source,
        EnergySourceRuntime {
            building: source,
            def_id: "generator".to_string(),
            max_output: PowerUnits::new(200),
            used_output: PowerUnits::ZERO,
        },
    );
    for (building, def_id) in [(c1, "consumer_c1"), (c2, "consumer_c2")] {
        network.consumers.insert(
            building,
            EnergyConsumerRuntime {
                building,
                def_id: def_id.to_string(),
                demand: PowerUnits::new(100),
                supplied: PowerUnits::ZERO,
                supplied_ratio: SuppliedRatio::ZERO,
                effective_ratio: SuppliedRatio::ZERO,
                state: EnergyConsumerState::Offline,
            },
        );
    }

    insert_test_edge(
        &mut network,
        EnergyEdgeId(1),
        source_node,
        shared_node,
        100,
        0,
    );
    insert_test_edge(&mut network, EnergyEdgeId(2), shared_node, c1_node, 100, 0);
    insert_test_edge(&mut network, EnergyEdgeId(3), shared_node, c2_node, 100, 0);
    insert_test_edge(
        &mut network,
        EnergyEdgeId(4),
        source_node,
        alternate_node,
        100,
        0,
    );
    insert_test_edge(
        &mut network,
        EnergyEdgeId(5),
        alternate_node,
        c1_node,
        100,
        0,
    );

    crate::energy::solver::solve_energy(&mut network, &catalog);

    let c1_runtime = network.consumers.get(&c1).unwrap();
    let c2_runtime = network.consumers.get(&c2).unwrap();
    assert_eq!(c1_runtime.supplied.raw(), 100);
    assert_eq!(c2_runtime.supplied.raw(), 100);
    assert_eq!(network.edges[&EnergyEdgeId(1)].current_flow.raw(), 100);
    assert!(network.edges[&EnergyEdgeId(4)].current_flow.raw() > 0);
}

#[test]
fn shared_physical_edge_capacity_limits_bucket_delivery() {
    let source = BuildingId(1);
    let left_consumer = BuildingId(2);
    let right_consumer = BuildingId(3);
    let left_source_node = EnergyNodeId(1);
    let left_bus = EnergyNodeId(2);
    let right_bus = EnergyNodeId(3);
    let left_consumer_node = EnergyNodeId(4);
    let right_consumer_node = EnergyNodeId(5);
    let shared_edge = EnergyEdgeId(2);

    let catalog = CoreCatalog::new(
        Vec::new(),
        Vec::new(),
        vec![
            generator_def(200),
            consumer_def("left_consumer", 100, 1),
            consumer_def("right_consumer", 100, 1),
        ],
    );
    let mut network = EnergyNetwork::default();
    insert_test_node(&mut network, left_source_node, Some(source));
    insert_test_node(&mut network, left_bus, None);
    insert_test_node(&mut network, right_bus, None);
    insert_test_node(&mut network, left_consumer_node, Some(left_consumer));
    insert_test_node(&mut network, right_consumer_node, Some(right_consumer));
    insert_test_source(&mut network, source, left_source_node, 200);
    insert_test_consumer(
        &mut network,
        left_consumer,
        left_consumer_node,
        "left_consumer",
        100,
    );
    insert_test_consumer(
        &mut network,
        right_consumer,
        right_consumer_node,
        "right_consumer",
        100,
    );
    insert_test_edge(
        &mut network,
        EnergyEdgeId(1),
        left_source_node,
        left_bus,
        200,
        0,
    );
    insert_test_edge(&mut network, shared_edge, left_bus, right_bus, 100, 0);
    insert_test_edge(
        &mut network,
        EnergyEdgeId(3),
        right_bus,
        left_consumer_node,
        100,
        0,
    );
    insert_test_edge(
        &mut network,
        EnergyEdgeId(4),
        right_bus,
        right_consumer_node,
        100,
        0,
    );

    crate::energy::solver::solve_energy(&mut network, &catalog);

    // Energy is modeled as one fungible commodity, so this regression avoids
    // artificial opposite-direction source identity and instead proves the
    // shared physical edge resource cannot be over-allocated.
    assert_eq!(network.consumers[&left_consumer].supplied.raw(), 50);
    assert_eq!(network.consumers[&right_consumer].supplied.raw(), 50);
    assert_eq!(network.edges[&shared_edge].current_flow.raw(), 100);
    assert!(network.edges[&shared_edge].current_flow <= network.edges[&shared_edge].capacity);
    assert!(network.edges[&shared_edge].constrained);
}

#[test]
fn lossy_path_spends_source_output_on_delivered_power_and_losses() {
    let source = BuildingId(1);
    let consumer = BuildingId(2);
    let source_node = EnergyNodeId(1);
    let consumer_node = EnergyNodeId(2);

    let catalog = CoreCatalog::new(
        Vec::new(),
        Vec::new(),
        vec![generator_def(100), consumer_def("consumer", 100, 1)],
    );
    let mut network = EnergyNetwork::default();
    insert_test_node(&mut network, source_node, Some(source));
    insert_test_node(&mut network, consumer_node, Some(consumer));
    insert_test_source(&mut network, source, source_node, 100);
    insert_test_consumer(&mut network, consumer, consumer_node, "consumer", 100);
    insert_test_edge(
        &mut network,
        EnergyEdgeId(1),
        source_node,
        consumer_node,
        100,
        1,
    );

    let report = crate::energy::solver::solve_energy(&mut network, &catalog);

    assert_eq!(network.consumers[&consumer].supplied.raw(), 50);
    assert_eq!(network.sources[&source].used_output.raw(), 100);
    assert_eq!(report.lost.raw(), 50);
}

#[test]
fn sufficient_generator_supply_does_not_discharge_battery() {
    let battery = BuildingId(1);
    let generator = BuildingId(2);
    let consumer = BuildingId(3);
    let battery_node = EnergyNodeId(1);
    let generator_node = EnergyNodeId(2);
    let consumer_node = EnergyNodeId(3);

    let catalog = CoreCatalog::new(
        Vec::new(),
        Vec::new(),
        vec![
            generator_def(100),
            CoreBuildingDef {
                id: "battery".to_string(),
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
                        coverage_radius_tiles: 0,
                        connection_range_tiles: 0,
                        edge_capacity: PowerUnits::new(100),
                        loss_per_tile: PowerUnits::ZERO,
                        power_class: DEFAULT_POWER_CLASS.to_string(),
                        input_power_classes: Vec::new(),
                    }),
                    generator: None,
                    storage: Some(StoragePowerDef {
                        capacity: EnergyAmount::new(100),
                        max_charge: PowerUnits::ZERO,
                        max_discharge: PowerUnits::new(100),
                        initial_charge: EnergyAmount::new(100),
                    }),
                    consumer: None,
                },
            },
            consumer_def("consumer", 100, 1),
        ],
    );
    let mut network = EnergyNetwork::default();
    insert_test_node(&mut network, battery_node, Some(battery));
    insert_test_node(&mut network, generator_node, Some(generator));
    insert_test_node(&mut network, consumer_node, Some(consumer));
    insert_test_source(&mut network, generator, generator_node, 100);
    network.node_by_building.insert(battery, battery_node);
    network
        .building_def_by_id
        .insert(battery, "battery".to_string());
    network.storages.insert(
        battery,
        crate::energy::EnergyStorageRuntime {
            building: battery,
            def_id: "battery".to_string(),
            stored: EnergyAmount::new(100),
        },
    );
    insert_test_consumer(&mut network, consumer, consumer_node, "consumer", 100);
    insert_test_edge(
        &mut network,
        EnergyEdgeId(1),
        battery_node,
        consumer_node,
        100,
        0,
    );
    insert_test_edge(
        &mut network,
        EnergyEdgeId(2),
        generator_node,
        consumer_node,
        100,
        0,
    );

    crate::energy::solver::solve_energy(&mut network, &catalog);

    assert_eq!(network.consumers[&consumer].supplied.raw(), 100);
    assert_eq!(network.storages[&battery].stored.raw(), 100);
    assert_eq!(network.sources[&generator].used_output.raw(), 100);
}

#[test]
fn large_loss_arithmetic_saturates_without_overflowing() {
    let source = BuildingId(1);
    let consumer = BuildingId(2);
    let source_node = EnergyNodeId(1);
    let consumer_node = EnergyNodeId(2);

    let catalog = CoreCatalog::new(
        Vec::new(),
        Vec::new(),
        vec![generator_def(i64::MAX), consumer_def("consumer", 1, 1)],
    );
    let mut network = EnergyNetwork::default();
    insert_test_node(&mut network, source_node, Some(source));
    insert_test_node(&mut network, consumer_node, Some(consumer));
    insert_test_source(&mut network, source, source_node, i64::MAX);
    insert_test_consumer(&mut network, consumer, consumer_node, "consumer", 1);
    insert_test_edge_with_length(
        &mut network,
        EnergyEdgeId(1),
        source_node,
        consumer_node,
        1,
        i32::MAX,
        i64::MAX / 2,
    );

    let report = crate::energy::solver::solve_energy(&mut network, &catalog);

    assert!(network.consumers[&consumer].supplied.raw() >= 0);
    assert!(report.lost.raw() >= 0);
    assert!(network.sources[&source].used_output.raw() >= 0);
}

#[test]
fn source_output_budget_is_consumed_by_delivery_plus_route_loss_during_planning() {
    let source_a = BuildingId(1);
    let source_b = BuildingId(2);
    let consumer = BuildingId(3);
    let source_a_node = EnergyNodeId(1);
    let source_b_node = EnergyNodeId(2);
    let consumer_node = EnergyNodeId(3);

    let catalog = CoreCatalog::new(
        Vec::new(),
        Vec::new(),
        vec![generator_def(200), consumer_def("consumer", 100, 1)],
    );
    let mut network = EnergyNetwork::default();
    insert_test_node(&mut network, source_a_node, Some(source_a));
    insert_test_node(&mut network, source_b_node, Some(source_b));
    insert_test_node(&mut network, consumer_node, Some(consumer));
    insert_test_source(&mut network, source_a, source_a_node, 100);
    insert_test_source(&mut network, source_b, source_b_node, 200);
    insert_test_consumer(&mut network, consumer, consumer_node, "consumer", 100);
    insert_test_edge(
        &mut network,
        EnergyEdgeId(1),
        source_a_node,
        consumer_node,
        100,
        1,
    );
    insert_test_edge(
        &mut network,
        EnergyEdgeId(2),
        source_b_node,
        consumer_node,
        100,
        2,
    );

    let report = crate::energy::solver::solve_energy(&mut network, &catalog);

    assert_eq!(network.consumers[&consumer].supplied.raw(), 100);
    assert_eq!(network.sources[&source_a].used_output.raw(), 100);
    assert_eq!(network.sources[&source_b].used_output.raw(), 150);
    assert_eq!(report.lost.raw(), 150);
}

fn insert_test_node(network: &mut EnergyNetwork, id: EnergyNodeId, building: Option<BuildingId>) {
    network.nodes.insert(
        id,
        EnergyNode {
            id,
            pos: TilePos::new(id.0 as i32, 0),
            building,
        },
    );
}

fn insert_test_source(
    network: &mut EnergyNetwork,
    building: BuildingId,
    node: EnergyNodeId,
    max_output: i64,
) {
    network.node_by_building.insert(building, node);
    network
        .building_def_by_id
        .insert(building, "generator".to_string());
    network.sources.insert(
        building,
        EnergySourceRuntime {
            building,
            def_id: "generator".to_string(),
            max_output: PowerUnits::new(max_output),
            used_output: PowerUnits::ZERO,
        },
    );
}

fn insert_test_consumer(
    network: &mut EnergyNetwork,
    building: BuildingId,
    node: EnergyNodeId,
    def_id: &str,
    demand: i64,
) {
    network.node_by_building.insert(building, node);
    network
        .building_def_by_id
        .insert(building, def_id.to_string());
    network.consumers.insert(
        building,
        EnergyConsumerRuntime {
            building,
            def_id: def_id.to_string(),
            demand: PowerUnits::new(demand),
            supplied: PowerUnits::ZERO,
            supplied_ratio: SuppliedRatio::ZERO,
            effective_ratio: SuppliedRatio::ZERO,
            state: EnergyConsumerState::Offline,
        },
    );
}

fn insert_test_edge(
    network: &mut EnergyNetwork,
    id: EnergyEdgeId,
    a: EnergyNodeId,
    b: EnergyNodeId,
    capacity: i64,
    loss_per_unit: i64,
) {
    insert_test_edge_with_length(network, id, a, b, capacity, 1, loss_per_unit);
}

fn insert_test_edge_with_length(
    network: &mut EnergyNetwork,
    id: EnergyEdgeId,
    a: EnergyNodeId,
    b: EnergyNodeId,
    capacity: i64,
    length_tiles: i32,
    loss_per_unit: i64,
) {
    network.edges.insert(
        id,
        EnergyEdge {
            id,
            a,
            b,
            length_tiles,
            capacity: PowerUnits::new(capacity),
            loss_per_unit: PowerUnits::new(loss_per_unit),
            allows_a_to_b: true,
            allows_b_to_a: true,
            current_flow: PowerUnits::ZERO,
            constrained: false,
        },
    );
}
