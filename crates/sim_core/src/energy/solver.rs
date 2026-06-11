//! Priority-ordered power distribution across the energy graph each tick.

use std::collections::{BTreeMap, BTreeSet};

use crate::catalog::CoreCatalog;
use crate::ids::{BuildingId, EnergyEdgeId, EnergyNodeId};

use super::model::{
    EnergyConsumerState, EnergyEdge, EnergyNetwork, EnergySolveReport, GeneratorMode,
    PowerSensitivity,
};
use super::units::{EnergyAmount, PowerUnits, SuppliedRatio};

const RATIO_FULL_PPM: u32 = 1_000_000;

pub fn solve_energy(network: &mut EnergyNetwork, catalog: &CoreCatalog) -> EnergySolveReport {
    solve_energy_with_solar_factor(network, catalog, SuppliedRatio::FULL)
}

pub fn solve_energy_with_solar_factor(
    network: &mut EnergyNetwork,
    catalog: &CoreCatalog,
    solar_factor: SuppliedRatio,
) -> EnergySolveReport {
    reset_runtime(network);
    update_generator_availability(network, catalog, solar_factor);

    let mut residual_edges = network
        .edges
        .iter()
        .map(|(id, edge)| (*id, edge.capacity))
        .collect::<BTreeMap<_, _>>();
    let mut source_remaining = source_capacity(network, catalog);
    let mut report = EnergySolveReport::default();

    for priority in sorted_priorities(network, catalog) {
        let bucket = consumers_for_priority(network, catalog, priority);
        if bucket.is_empty() {
            continue;
        }

        let generator_ratio = max_bucket_ratio(
            network,
            &source_remaining,
            &residual_edges,
            &bucket,
            0,
            RATIO_FULL_PPM,
            SourceMode::GeneratorsOnly,
        );
        let generator_plan = plan_bucket_delivery(
            network,
            &source_remaining,
            &residual_edges,
            &bucket,
            0,
            generator_ratio,
            SourceMode::GeneratorsOnly,
        )
        .unwrap_or_default();

        for delivery in generator_plan {
            apply_path_flow(
                network,
                &mut source_remaining,
                &mut residual_edges,
                &delivery.path,
                delivery.amount,
                &mut report,
            );
        }

        let mut ratio = generator_ratio;
        if ratio < RATIO_FULL_PPM {
            ratio = max_bucket_ratio(
                network,
                &source_remaining,
                &residual_edges,
                &bucket,
                ratio,
                RATIO_FULL_PPM,
                SourceMode::BatteriesOnly,
            );
            let battery_plan = plan_bucket_delivery(
                network,
                &source_remaining,
                &residual_edges,
                &bucket,
                generator_ratio,
                ratio,
                SourceMode::BatteriesOnly,
            )
            .unwrap_or_default();

            for delivery in battery_plan {
                apply_path_flow(
                    network,
                    &mut source_remaining,
                    &mut residual_edges,
                    &delivery.path,
                    delivery.amount,
                    &mut report,
                );
            }
        }

        for building in bucket {
            let supplied = target_supply(network.consumers[&building].demand, ratio);
            let Some(definition) = consumer_power_def(network, catalog, building) else {
                continue;
            };
            if let Some(consumer) = network.consumers.get_mut(&building) {
                consumer.supplied = PowerUnits::new(supplied);
                consumer.supplied_ratio =
                    SuppliedRatio::from_parts(consumer.supplied, consumer.demand);
                consumer.state = consumer_state(consumer.supplied_ratio, definition.offline_below);
                consumer.effective_ratio = effective_ratio(
                    consumer.supplied_ratio,
                    consumer.state,
                    definition.power_sensitivity,
                );
                report.delivered =
                    PowerUnits::new(sat_add_i64(report.delivered.raw(), consumer.supplied.raw()));
            }
        }
    }

    charge_batteries(
        network,
        catalog,
        &mut source_remaining,
        &mut residual_edges,
        &mut report,
    );

    report.constrained_edges = network
        .edges
        .values()
        .filter(|edge| edge.constrained)
        .count();
    network.last_report = report.clone();
    report
}

fn update_generator_availability(
    network: &mut EnergyNetwork,
    catalog: &CoreCatalog,
    solar_factor: SuppliedRatio,
) {
    let source_buildings = network.sources.keys().copied().collect::<Vec<_>>();
    for building in source_buildings {
        let Some(definition) = building_def(network, catalog, building) else {
            continue;
        };
        let Some(generator) = definition.power.generator.as_ref() else {
            continue;
        };
        let available = match generator.mode {
            GeneratorMode::Constant => network.sources[&building]
                .max_output
                .min(generator.max_output),
            GeneratorMode::Solar => scale_power(generator.max_output, solar_factor),
        };
        if let Some(source) = network.sources.get_mut(&building) {
            source.max_output = available;
        }
    }
}

fn reset_runtime(network: &mut EnergyNetwork) {
    for edge in network.edges.values_mut() {
        edge.current_flow = PowerUnits::ZERO;
        edge.constrained = false;
    }
    for source in network.sources.values_mut() {
        source.used_output = PowerUnits::ZERO;
    }
    for consumer in network.consumers.values_mut() {
        consumer.supplied = PowerUnits::ZERO;
        consumer.supplied_ratio = SuppliedRatio::ZERO;
        consumer.effective_ratio = SuppliedRatio::ZERO;
        consumer.state = EnergyConsumerState::Offline;
    }
    network.last_report = EnergySolveReport::default();
}

fn source_capacity(
    network: &EnergyNetwork,
    catalog: &CoreCatalog,
) -> BTreeMap<BuildingId, PowerUnits> {
    let mut sources = network
        .sources
        .iter()
        .map(|(building, source)| (*building, source.max_output))
        .collect::<BTreeMap<_, _>>();

    for (building, storage) in &network.storages {
        let Some(def) = building_def(network, catalog, *building) else {
            continue;
        };
        let Some(storage_def) = &def.power.storage else {
            continue;
        };
        let available = storage
            .stored
            .raw()
            .min(storage_def.max_discharge.raw())
            .max(0);
        if available > 0 {
            sources.insert(*building, PowerUnits::new(available));
        }
    }

    sources
}

fn sorted_priorities(network: &EnergyNetwork, catalog: &CoreCatalog) -> Vec<u8> {
    let mut priorities = BTreeSet::new();
    for building in network.consumers.keys() {
        if let Some(priority) = consumer_priority(network, catalog, *building) {
            priorities.insert(priority);
        }
    }
    priorities.into_iter().collect()
}

fn consumers_for_priority(
    network: &EnergyNetwork,
    catalog: &CoreCatalog,
    priority: u8,
) -> Vec<BuildingId> {
    network
        .consumers
        .keys()
        .copied()
        .filter(|building| consumer_priority(network, catalog, *building) == Some(priority))
        .collect()
}

fn consumer_priority(
    network: &EnergyNetwork,
    catalog: &CoreCatalog,
    building: BuildingId,
) -> Option<u8> {
    building_def(network, catalog, building)
        .and_then(|def| def.power.consumer.as_ref())
        .map(|consumer| consumer.priority)
}

fn consumer_power_def(
    network: &EnergyNetwork,
    catalog: &CoreCatalog,
    building: BuildingId,
) -> Option<super::model::ConsumerPowerDef> {
    building_def(network, catalog, building)
        .and_then(|def| def.power.consumer.as_ref())
        .cloned()
}

fn consumer_state(ratio: SuppliedRatio, offline_below: SuppliedRatio) -> EnergyConsumerState {
    if ratio.is_zero() || ratio < offline_below {
        EnergyConsumerState::Offline
    } else if ratio.ppm() < RATIO_FULL_PPM {
        EnergyConsumerState::Degraded
    } else {
        EnergyConsumerState::Powered
    }
}

fn effective_ratio(
    ratio: SuppliedRatio,
    state: EnergyConsumerState,
    power_sensitivity: PowerSensitivity,
) -> SuppliedRatio {
    if state == EnergyConsumerState::Offline {
        return SuppliedRatio::ZERO;
    }

    match power_sensitivity {
        PowerSensitivity::Linear => ratio,
        PowerSensitivity::Threshold => SuppliedRatio::FULL,
    }
}

fn building_def<'a>(
    network: &EnergyNetwork,
    catalog: &'a CoreCatalog,
    building: BuildingId,
) -> Option<&'a crate::catalog::CoreBuildingDef> {
    catalog.building_by_id(network.building_def_by_id.get(&building)?)
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum SourceMode {
    #[allow(dead_code, reason = "reserved for combined source passes")]
    All,
    BatteriesOnly,
    GeneratorsOnly,
}

impl SourceMode {
    fn includes(self, network: &EnergyNetwork, building: BuildingId) -> bool {
        match self {
            Self::All => true,
            Self::BatteriesOnly => {
                network.storages.contains_key(&building) && !network.sources.contains_key(&building)
            }
            Self::GeneratorsOnly => network.sources.contains_key(&building),
        }
    }
}

fn max_bucket_ratio(
    network: &EnergyNetwork,
    source_remaining: &BTreeMap<BuildingId, PowerUnits>,
    residual_edges: &BTreeMap<EnergyEdgeId, PowerUnits>,
    bucket: &[BuildingId],
    base_ratio_ppm: u32,
    high_ratio_ppm: u32,
    source_mode: SourceMode,
) -> u32 {
    let mut low = base_ratio_ppm;
    let mut high = high_ratio_ppm;
    while low < high {
        let mid = low + (high - low).div_ceil(2);
        if plan_bucket_delivery(
            network,
            source_remaining,
            residual_edges,
            bucket,
            base_ratio_ppm,
            mid,
            source_mode,
        )
        .is_some()
        {
            low = mid;
        } else {
            high = mid - 1;
        }
    }
    low
}

fn plan_bucket_delivery(
    network: &EnergyNetwork,
    source_remaining: &BTreeMap<BuildingId, PowerUnits>,
    residual_edges: &BTreeMap<EnergyEdgeId, PowerUnits>,
    bucket: &[BuildingId],
    base_ratio_ppm: u32,
    ratio_ppm: u32,
    source_mode: SourceMode,
) -> Option<Vec<PlannedDelivery>> {
    let target_by_consumer = bucket
        .iter()
        .map(|building| {
            let demand = network.consumers[building].demand;
            (
                *building,
                target_supply(demand, ratio_ppm) - target_supply(demand, base_ratio_ppm),
            )
        })
        .filter(|(_, target)| *target > 0)
        .collect::<BTreeMap<_, _>>();
    let total_target = target_by_consumer.values().sum::<i64>();
    if total_target == 0 {
        return Some(Vec::new());
    }

    let mut flow = BucketFlowNetwork::from_energy_network(
        network,
        source_remaining,
        residual_edges,
        &target_by_consumer,
        source_mode,
    )?;
    if flow.send_min_cost_flow(total_target) != total_target {
        return None;
    }

    flow.into_planned_deliveries(source_remaining)
}

fn target_supply(demand: PowerUnits, ratio_ppm: u32) -> i64 {
    clamp_i128_to_i64(
        demand.raw().max(0) as i128 * i128::from(ratio_ppm) / i128::from(RATIO_FULL_PPM),
    )
}

fn scale_power(power: PowerUnits, ratio: SuppliedRatio) -> PowerUnits {
    PowerUnits::new(clamp_i128_to_i64(
        power.raw().max(0) as i128 * i128::from(ratio.ppm()) / i128::from(RATIO_FULL_PPM),
    ))
}

fn path_loss_amount(amount: i64, loss_cost: i64) -> i64 {
    sat_mul_i64(amount.max(0), loss_cost.max(0))
}

fn path_output_cost(amount: i64, loss_cost: i64) -> i64 {
    sat_add_i64(amount.max(0), path_loss_amount(amount, loss_cost))
}

fn max_deliverable_for_output(output: i64, loss_cost: i64) -> i64 {
    if output <= 0 {
        return 0;
    }
    if loss_cost <= 0 {
        return output;
    }
    let unit_cost = (1_i128 + i128::from(loss_cost)).min(i128::from(i64::MAX));
    clamp_i128_to_i64(i128::from(output) / unit_cost)
}

fn sat_add_i64(left: i64, right: i64) -> i64 {
    clamp_i128_to_i64(i128::from(left) + i128::from(right))
}

fn sat_mul_i64(left: i64, right: i64) -> i64 {
    clamp_i128_to_i64(i128::from(left) * i128::from(right))
}

fn clamp_i128_to_i64(value: i128) -> i64 {
    value.clamp(i128::from(i64::MIN), i128::from(i64::MAX)) as i64
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct PlannedDelivery {
    path: EnergyPath,
    amount: PowerUnits,
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct BucketFlowNetwork {
    graph: Vec<Vec<FlowArc>>,
    source: usize,
    sink: usize,
}

impl BucketFlowNetwork {
    fn from_energy_network(
        network: &EnergyNetwork,
        source_remaining: &BTreeMap<BuildingId, PowerUnits>,
        residual_edges: &BTreeMap<EnergyEdgeId, PowerUnits>,
        target_by_consumer: &BTreeMap<BuildingId, i64>,
        source_mode: SourceMode,
    ) -> Option<Self> {
        let source = 0;
        let mut flow = Self {
            graph: vec![Vec::new()],
            source,
            sink: source,
        };
        let mut node_index = BTreeMap::new();
        for node in network.nodes.keys() {
            node_index.insert(*node, flow.add_flow_node());
        }

        for (building, remaining) in source_remaining {
            if remaining.raw() <= 0 {
                continue;
            }
            if !source_mode.includes(network, *building) {
                continue;
            }
            let node = *network.node_by_building.get(building)?;
            flow.add_edge(
                source,
                node_index[&node],
                remaining.raw(),
                0,
                FlowArcMeta::Source(*building),
            );
        }

        for (edge_id, edge) in &network.edges {
            let capacity = residual_edges.get(edge_id)?.raw();
            if capacity <= 0 {
                continue;
            }
            let cost = edge_loss_cost(edge);
            let edge_in = flow.add_flow_node();
            let edge_out = flow.add_flow_node();
            if edge.allows_a_to_b {
                flow.add_edge(
                    node_index[&edge.a],
                    edge_in,
                    capacity,
                    cost,
                    FlowArcMeta::Connector,
                );
            }
            if edge.allows_b_to_a {
                flow.add_edge(
                    node_index[&edge.b],
                    edge_in,
                    capacity,
                    cost,
                    FlowArcMeta::Connector,
                );
            }
            flow.add_edge(
                edge_in,
                edge_out,
                capacity,
                0,
                FlowArcMeta::Physical(*edge_id),
            );
            if edge.allows_b_to_a {
                flow.add_edge(
                    edge_out,
                    node_index[&edge.a],
                    capacity,
                    0,
                    FlowArcMeta::Connector,
                );
            }
            if edge.allows_a_to_b {
                flow.add_edge(
                    edge_out,
                    node_index[&edge.b],
                    capacity,
                    0,
                    FlowArcMeta::Connector,
                );
            }
        }

        flow.sink = flow.add_flow_node();
        for (building, target) in target_by_consumer {
            let node = *network.node_by_building.get(building)?;
            flow.add_edge(
                node_index[&node],
                flow.sink,
                *target,
                0,
                FlowArcMeta::Consumer(*building),
            );
        }

        Some(flow)
    }

    fn add_flow_node(&mut self) -> usize {
        let node = self.graph.len();
        self.graph.push(Vec::new());
        node
    }

    fn add_edge(&mut self, from: usize, to: usize, capacity: i64, cost: i64, meta: FlowArcMeta) {
        let reverse_from = self.graph[to].len();
        let reverse_to = self.graph[from].len();
        let reverse_meta = match meta {
            FlowArcMeta::Source(_) => FlowArcMeta::SourceResidual,
            _ => FlowArcMeta::None,
        };
        self.graph[from].push(FlowArc {
            to,
            rev: reverse_from,
            cap: capacity,
            cost,
            meta,
        });
        self.graph[to].push(FlowArc {
            to: from,
            rev: reverse_to,
            cap: 0,
            cost: -cost,
            meta: reverse_meta,
        });
    }

    fn send_min_cost_flow(&mut self, target: i64) -> i64 {
        let mut sent = 0;
        while sent < target {
            let Some(path) = self.shortest_residual_path() else {
                break;
            };
            let loss_cost = self.path_loss_cost(&path);
            let amount = path
                .iter()
                .map(|(from, edge)| {
                    let arc = &self.graph[*from][*edge];
                    match arc.meta {
                        FlowArcMeta::Source(_) => max_deliverable_for_output(arc.cap, loss_cost),
                        _ => arc.cap,
                    }
                })
                .min()
                .unwrap_or(0)
                .min(target - sent);
            if amount <= 0 {
                break;
            }
            for (from, edge) in path {
                let to = self.graph[from][edge].to;
                let reverse = self.graph[from][edge].rev;
                let debit = match self.graph[from][edge].meta {
                    FlowArcMeta::Source(_) => path_output_cost(amount, loss_cost),
                    _ => amount,
                };
                self.graph[from][edge].cap = (self.graph[from][edge].cap - debit).max(0);
                self.graph[to][reverse].cap = sat_add_i64(self.graph[to][reverse].cap, amount);
            }
            sent += amount;
        }
        sent
    }

    fn path_loss_cost(&self, path: &[(usize, usize)]) -> i64 {
        path.iter().fold(0, |cost, (from, edge)| {
            sat_add_i64(cost, self.graph[*from][*edge].cost)
        })
    }

    fn shortest_residual_path(&self) -> Option<Vec<(usize, usize)>> {
        let node_count = self.graph.len();
        let mut dist = vec![i64::MAX; node_count];
        let mut hops = vec![usize::MAX; node_count];
        let mut previous = vec![None; node_count];
        dist[self.source] = 0;
        hops[self.source] = 0;

        for _ in 0..node_count.saturating_sub(1) {
            let mut changed = false;
            for from in 0..node_count {
                if dist[from] == i64::MAX {
                    continue;
                }
                for (edge_index, edge) in self.graph[from].iter().enumerate() {
                    if edge.cap <= 0 {
                        continue;
                    }
                    if edge.meta == FlowArcMeta::SourceResidual {
                        continue;
                    }
                    let to = edge.to;
                    let candidate = (
                        sat_add_i64(dist[from], edge.cost),
                        hops[from] + 1,
                        from,
                        edge_index,
                    );
                    let current = (
                        dist[to],
                        hops[to],
                        previous[to].map(|(node, _)| node).unwrap_or(usize::MAX),
                        previous[to].map(|(_, edge)| edge).unwrap_or(usize::MAX),
                    );
                    if candidate < current {
                        dist[to] = candidate.0;
                        hops[to] = candidate.1;
                        previous[to] = Some((from, edge_index));
                        changed = true;
                    }
                }
            }
            if !changed {
                break;
            }
        }

        previous[self.sink]?;
        let mut path = Vec::new();
        let mut node = self.sink;
        while node != self.source {
            let (from, edge) = previous[node]?;
            path.push((from, edge));
            node = from;
        }
        path.reverse();
        Some(path)
    }

    fn into_planned_deliveries(
        self,
        source_remaining: &BTreeMap<BuildingId, PowerUnits>,
    ) -> Option<Vec<PlannedDelivery>> {
        let mut source_flows = BTreeMap::<BuildingId, (usize, i64)>::new();
        let mut consumer_remaining = BTreeMap::<usize, i64>::new();
        let mut physical_flows = vec![Vec::<PositiveFlowArc>::new(); self.graph.len()];

        for (from, arcs) in self.graph.iter().enumerate() {
            for arc in arcs {
                let flow = self.graph[arc.to][arc.rev].cap;
                if flow <= 0 {
                    continue;
                }
                match arc.meta {
                    FlowArcMeta::Source(building) => {
                        source_flows
                            .entry(building)
                            .and_modify(|(_, existing)| *existing += flow)
                            .or_insert((arc.to, flow));
                    }
                    FlowArcMeta::Consumer(_) => {
                        *consumer_remaining.entry(from).or_default() += flow;
                    }
                    FlowArcMeta::Connector | FlowArcMeta::Physical(_) => {
                        physical_flows[from].push(PositiveFlowArc {
                            to: arc.to,
                            edge_id: match arc.meta {
                                FlowArcMeta::Physical(edge_id) => Some(edge_id),
                                _ => None,
                            },
                            remaining: flow,
                            cost: arc.cost,
                        });
                    }
                    FlowArcMeta::None | FlowArcMeta::SourceResidual => {}
                }
            }
        }
        for arcs in &mut physical_flows {
            arcs.sort_by_key(|arc| (arc.cost, arc.edge_id, arc.to));
        }

        let mut planned = Vec::new();
        let mut output_by_source = BTreeMap::<BuildingId, i64>::new();
        for (source, (node, mut remaining)) in source_flows {
            while remaining > 0 {
                let path = cheapest_positive_flow_path(&physical_flows, &consumer_remaining, node)?;
                let consumer_left = *consumer_remaining.get(&path.target_node)?;
                let edge_capacity = path
                    .edges
                    .iter()
                    .map(|(from, edge)| physical_flows[*from][*edge].remaining)
                    .min()
                    .unwrap_or(remaining);
                let amount = remaining.min(consumer_left).min(edge_capacity);
                if amount <= 0 {
                    return None;
                }

                let output_cost = path_output_cost(amount, path.loss_cost);
                let used = output_by_source.entry(source).or_default();
                *used = sat_add_i64(*used, output_cost);
                if *used > source_remaining.get(&source)?.raw() {
                    return None;
                }

                remaining -= amount;
                if let Some(target) = consumer_remaining.get_mut(&path.target_node) {
                    *target -= amount;
                    if *target == 0 {
                        consumer_remaining.remove(&path.target_node);
                    }
                }
                for (from, edge) in &path.edges {
                    physical_flows[*from][*edge].remaining -= amount;
                }

                planned.push(PlannedDelivery {
                    path: EnergyPath {
                        source,
                        edges: path
                            .edges
                            .iter()
                            .filter_map(|(from, edge)| physical_flows[*from][*edge].edge_id)
                            .collect(),
                        capacity: PowerUnits::new(amount),
                        loss_cost: path.loss_cost,
                    },
                    amount: PowerUnits::new(amount),
                });
            }
        }

        if consumer_remaining.values().any(|remaining| *remaining > 0) {
            return None;
        }
        Some(planned)
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct FlowArc {
    to: usize,
    rev: usize,
    cap: i64,
    cost: i64,
    meta: FlowArcMeta,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum FlowArcMeta {
    None,
    SourceResidual,
    Connector,
    Source(BuildingId),
    Consumer(BuildingId),
    Physical(EnergyEdgeId),
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct PositiveFlowArc {
    to: usize,
    edge_id: Option<EnergyEdgeId>,
    remaining: i64,
    cost: i64,
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct PositiveFlowPath {
    target_node: usize,
    edges: Vec<(usize, usize)>,
    loss_cost: i64,
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct PositivePathState {
    cost: i64,
    hops: usize,
    previous: Option<(usize, usize)>,
}

fn cheapest_positive_flow_path(
    graph: &[Vec<PositiveFlowArc>],
    consumer_remaining: &BTreeMap<usize, i64>,
    source: usize,
) -> Option<PositiveFlowPath> {
    if consumer_remaining
        .get(&source)
        .is_some_and(|amount| *amount > 0)
    {
        return Some(PositiveFlowPath {
            target_node: source,
            edges: Vec::new(),
            loss_cost: 0,
        });
    }

    let mut best = BTreeMap::<usize, PositivePathState>::new();
    let mut frontier = BTreeSet::new();
    best.insert(
        source,
        PositivePathState {
            cost: 0,
            hops: 0,
            previous: None,
        },
    );
    frontier.insert((0, 0_usize, source));

    while let Some((cost, hops, node)) = frontier.pop_first() {
        if best
            .get(&node)
            .is_some_and(|state| (state.cost, state.hops) != (cost, hops))
        {
            continue;
        }
        if consumer_remaining
            .get(&node)
            .is_some_and(|amount| *amount > 0)
        {
            let mut edges = Vec::new();
            let mut cursor = node;
            while let Some((previous, edge)) = best.get(&cursor).and_then(|state| state.previous) {
                edges.push((previous, edge));
                cursor = previous;
            }
            edges.reverse();
            return Some(PositiveFlowPath {
                target_node: node,
                edges,
                loss_cost: cost,
            });
        }

        for (edge_index, edge) in graph[node].iter().enumerate() {
            if edge.remaining <= 0 {
                continue;
            }
            let candidate = PositivePathState {
                cost: sat_add_i64(cost, edge.cost),
                hops: hops + 1,
                previous: Some((node, edge_index)),
            };
            let replace = best.get(&edge.to).is_none_or(|current| {
                (candidate.cost, candidate.hops, node, edge_index)
                    < (
                        current.cost,
                        current.hops,
                        current.previous.map(|(prev, _)| prev).unwrap_or(node),
                        current.previous.map(|(_, edge)| edge).unwrap_or(edge_index),
                    )
            });
            if replace {
                best.insert(edge.to, candidate.clone());
                frontier.insert((candidate.cost, candidate.hops, edge.to));
            }
        }
    }

    None
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct EnergyPath {
    source: BuildingId,
    edges: Vec<EnergyEdgeId>,
    capacity: PowerUnits,
    loss_cost: i64,
}

fn cheapest_path_to_building(
    network: &EnergyNetwork,
    source_remaining: &BTreeMap<BuildingId, PowerUnits>,
    residual_edges: &BTreeMap<EnergyEdgeId, PowerUnits>,
    target_building: BuildingId,
    source_mode: SourceMode,
) -> Option<EnergyPath> {
    let target = *network.node_by_building.get(&target_building)?;
    let mut best: Option<EnergyPath> = None;

    for (source, remaining) in source_remaining {
        if remaining.raw() <= 0 {
            continue;
        }
        if source_mode == SourceMode::GeneratorsOnly && !network.sources.contains_key(source) {
            continue;
        }
        let Some(source_node) = network.node_by_building.get(source).copied() else {
            continue;
        };
        let Some(candidate) = cheapest_node_path(
            network,
            residual_edges,
            source_node,
            target,
            *source,
            *remaining,
        ) else {
            continue;
        };

        best = match best {
            Some(current) if path_full_order_key(&current) <= path_full_order_key(&candidate) => {
                Some(current)
            }
            _ => Some(candidate),
        };
    }

    best
}

fn path_full_order_key(path: &EnergyPath) -> (i64, usize, BuildingId, Vec<EnergyEdgeId>) {
    (
        path.loss_cost,
        path.edges.len(),
        path.source,
        path.edges.clone(),
    )
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct NodePathState {
    cost: i64,
    hops: usize,
    previous: Option<(EnergyNodeId, EnergyEdgeId)>,
}

fn cheapest_node_path(
    network: &EnergyNetwork,
    residual_edges: &BTreeMap<EnergyEdgeId, PowerUnits>,
    source_node: EnergyNodeId,
    target_node: EnergyNodeId,
    source_building: BuildingId,
    source_capacity: PowerUnits,
) -> Option<EnergyPath> {
    if source_node == target_node {
        return Some(EnergyPath {
            source: source_building,
            edges: Vec::new(),
            capacity: source_capacity,
            loss_cost: 0,
        });
    }

    let adjacency = adjacency(network);
    let mut best = BTreeMap::new();
    let mut frontier = BTreeSet::new();
    best.insert(
        source_node,
        NodePathState {
            cost: 0,
            hops: 0,
            previous: None,
        },
    );
    frontier.insert((0, 0_usize, source_node));

    while let Some((cost, hops, node)) = frontier.pop_first() {
        if best
            .get(&node)
            .is_some_and(|state| (state.cost, state.hops) != (cost, hops))
        {
            continue;
        }
        if node == target_node {
            break;
        }

        for (next_node, edge_id) in adjacency.get(&node).into_iter().flatten() {
            if residual_edges
                .get(edge_id)
                .is_none_or(|capacity| capacity.raw() <= 0)
            {
                continue;
            }
            let edge = &network.edges[edge_id];
            let edge_cost = edge_loss_cost(edge);
            let candidate = NodePathState {
                cost: sat_add_i64(cost, edge_cost),
                hops: hops + 1,
                previous: Some((node, *edge_id)),
            };

            let replace = best.get(next_node).is_none_or(|current| {
                (candidate.cost, candidate.hops, node, *edge_id)
                    < (
                        current.cost,
                        current.hops,
                        current.previous.map(|(prev, _)| prev).unwrap_or(node),
                        current.previous.map(|(_, edge)| edge).unwrap_or(*edge_id),
                    )
            });
            if replace {
                best.insert(*next_node, candidate.clone());
                frontier.insert((candidate.cost, candidate.hops, *next_node));
            }
        }
    }

    let target = best.get(&target_node)?;
    let mut edge_ids = Vec::new();
    let mut node = target_node;
    let mut capacity = source_capacity.raw();
    while let Some((previous, edge_id)) = best.get(&node).and_then(|state| state.previous) {
        edge_ids.push(edge_id);
        capacity = capacity.min(residual_edges[&edge_id].raw());
        node = previous;
    }
    edge_ids.reverse();

    capacity = capacity.min(max_deliverable_for_output(
        source_capacity.raw(),
        target.cost,
    ));

    Some(EnergyPath {
        source: source_building,
        edges: edge_ids,
        capacity: PowerUnits::new(capacity.max(0)),
        loss_cost: target.cost,
    })
}

fn adjacency(network: &EnergyNetwork) -> BTreeMap<EnergyNodeId, Vec<(EnergyNodeId, EnergyEdgeId)>> {
    let mut adjacency = BTreeMap::<EnergyNodeId, Vec<(EnergyNodeId, EnergyEdgeId)>>::new();
    for (edge_id, edge) in &network.edges {
        adjacency
            .entry(edge.a)
            .or_default()
            .push((edge.b, *edge_id));
        adjacency
            .entry(edge.b)
            .or_default()
            .push((edge.a, *edge_id));
    }
    adjacency
}

fn edge_loss_cost(edge: &EnergyEdge) -> i64 {
    sat_mul_i64(
        i64::from(edge.length_tiles.max(1)),
        edge.loss_per_unit.raw().max(0),
    )
}

fn debit_planned_path(
    source_remaining: &mut BTreeMap<BuildingId, PowerUnits>,
    residual_edges: &mut BTreeMap<EnergyEdgeId, PowerUnits>,
    path: &EnergyPath,
    amount: i64,
) {
    let output_cost = path_output_cost(amount, path.loss_cost);
    if let Some(source) = source_remaining.get_mut(&path.source) {
        *source = PowerUnits::new((source.raw() - output_cost).max(0));
    }
    for edge_id in &path.edges {
        if let Some(residual) = residual_edges.get_mut(edge_id) {
            *residual = PowerUnits::new(residual.raw() - amount);
        }
    }
}

fn apply_path_flow(
    network: &mut EnergyNetwork,
    source_remaining: &mut BTreeMap<BuildingId, PowerUnits>,
    residual_edges: &mut BTreeMap<EnergyEdgeId, PowerUnits>,
    path: &EnergyPath,
    amount: PowerUnits,
    report: &mut EnergySolveReport,
) {
    let output_cost = path_output_cost(amount.raw(), path.loss_cost);
    let loss = path_loss_amount(amount.raw(), path.loss_cost);
    debit_planned_path(source_remaining, residual_edges, path, amount.raw());

    if let Some(source) = network.sources.get_mut(&path.source) {
        source.used_output = PowerUnits::new(sat_add_i64(source.used_output.raw(), output_cost));
    }
    if let Some(storage) = network.storages.get_mut(&path.source) {
        storage.stored = EnergyAmount::new((storage.stored.raw() - output_cost).max(0));
    }

    for edge_id in &path.edges {
        if let Some(edge) = network.edges.get_mut(edge_id) {
            edge.current_flow = PowerUnits::new(edge.current_flow.raw() + amount.raw());
            if residual_edges
                .get(edge_id)
                .is_some_and(|residual| residual.raw() == 0)
            {
                edge.constrained = true;
            }
        }
    }
    report.lost = PowerUnits::new(sat_add_i64(report.lost.raw(), loss));
}

fn charge_batteries(
    network: &mut EnergyNetwork,
    catalog: &CoreCatalog,
    source_remaining: &mut BTreeMap<BuildingId, PowerUnits>,
    residual_edges: &mut BTreeMap<EnergyEdgeId, PowerUnits>,
    report: &mut EnergySolveReport,
) {
    let batteries = network.storages.keys().copied().collect::<Vec<_>>();
    for battery in batteries {
        let Some(def) = building_def(network, catalog, battery) else {
            continue;
        };
        let Some(storage_def) = &def.power.storage else {
            continue;
        };
        let Some(storage) = network.storages.get(&battery) else {
            continue;
        };
        let mut remaining = storage_def
            .max_charge
            .raw()
            .min((storage_def.capacity.raw() - storage.stored.raw()).max(0));

        while remaining > 0 {
            let Some(path) = cheapest_path_to_building(
                network,
                source_remaining,
                residual_edges,
                battery,
                SourceMode::GeneratorsOnly,
            ) else {
                break;
            };
            let amount = remaining.min(path.capacity.raw()).max(0);
            if amount == 0 {
                break;
            }
            apply_charge_flow(
                network,
                source_remaining,
                residual_edges,
                &path,
                battery,
                PowerUnits::new(amount),
                report,
            );
            remaining -= amount;
        }
    }
}

fn apply_charge_flow(
    network: &mut EnergyNetwork,
    source_remaining: &mut BTreeMap<BuildingId, PowerUnits>,
    residual_edges: &mut BTreeMap<EnergyEdgeId, PowerUnits>,
    path: &EnergyPath,
    battery: BuildingId,
    amount: PowerUnits,
    report: &mut EnergySolveReport,
) {
    let output_cost = path_output_cost(amount.raw(), path.loss_cost);
    let loss = path_loss_amount(amount.raw(), path.loss_cost);
    debit_planned_path(source_remaining, residual_edges, path, amount.raw());

    if let Some(source) = network.sources.get_mut(&path.source) {
        source.used_output = PowerUnits::new(sat_add_i64(source.used_output.raw(), output_cost));
    }
    for edge_id in &path.edges {
        if let Some(edge) = network.edges.get_mut(edge_id) {
            edge.current_flow = PowerUnits::new(edge.current_flow.raw() + amount.raw());
            if residual_edges
                .get(edge_id)
                .is_some_and(|residual| residual.raw() == 0)
            {
                edge.constrained = true;
            }
        }
    }
    if let Some(storage) = network.storages.get_mut(&battery) {
        storage.stored = EnergyAmount::new(sat_add_i64(storage.stored.raw(), amount.raw()));
    }
    report.lost = PowerUnits::new(sat_add_i64(report.lost.raw(), loss));
}
