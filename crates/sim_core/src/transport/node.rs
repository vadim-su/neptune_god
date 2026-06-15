//! Transport routing nodes between packed belt lines.

use serde::{Deserialize, Serialize};

use crate::ids::{ItemKindId, LineId, TilePos};
use crate::topology::graph::Direction;
use crate::units::DistanceUnits;

#[derive(Clone, Copy, Debug, Deserialize, Hash, PartialEq, Eq, PartialOrd, Ord, Serialize)]
pub struct TransportNodeId(pub u64);

#[derive(Clone, Copy, Debug, Deserialize, PartialEq, Eq, PartialOrd, Ord, Serialize)]
pub enum TransportPortRole {
    Input,
    Output,
}

#[derive(Clone, Copy, Debug, Deserialize, PartialEq, Eq, Serialize)]
pub enum TransportNodeKind {
    BlockedFront,
    EndTransfer,
    SideLoad { near_lane: usize },
    Splitter2x1,
    Underground,
    ConveyorLift,
}

impl TransportNodeKind {
    pub const fn sort_order(self) -> u8 {
        match self {
            Self::BlockedFront => 0,
            Self::EndTransfer => 1,
            Self::SideLoad { .. } => 2,
            Self::Splitter2x1 => 3,
            Self::Underground => 4,
            Self::ConveyorLift => 5,
        }
    }
}

#[derive(Clone, Copy, Debug, Deserialize, PartialEq, Eq, Serialize)]
pub struct TransportPortRef {
    pub node: TransportNodeId,
    pub line: LineId,
    pub lane: usize,
    pub role: TransportPortRole,
}

#[derive(Clone, Copy, Debug, Deserialize, PartialEq, Eq, Serialize)]
pub struct TransportPort {
    pub node: TransportNodeId,
    pub role: TransportPortRole,
    pub tile: TilePos,
    pub side: Option<Direction>,
    pub lane: usize,
    pub line: LineId,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Eq, Serialize)]
pub struct SplitterIngressItem {
    pub item: ItemKindId,
    pub input_channel: usize,
    pub lane: usize,
    pub progress: DistanceUnits,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Eq, Serialize)]
pub struct SplitterBufferedItem {
    pub item: ItemKindId,
    pub source_channel: usize,
    pub lane: usize,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Eq, Serialize)]
pub struct SplitterEgressItem {
    pub item: ItemKindId,
    pub source_channel: usize,
    pub output_channel: usize,
    pub lane: usize,
    pub progress: DistanceUnits,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Eq, Serialize)]
pub struct SplitterRuntime {
    pub next_output: usize,
    pub next_output_by_lane: [usize; 2],
    pub ingress_items: Vec<SplitterIngressItem>,
    pub buffered_items: Vec<SplitterBufferedItem>,
    pub egress_items: Vec<SplitterEgressItem>,
}

impl Default for SplitterRuntime {
    fn default() -> Self {
        Self::with_next_output(0)
    }
}

impl SplitterRuntime {
    pub fn with_next_output(next_output: usize) -> Self {
        let next_output = next_output % 2;
        Self {
            next_output,
            next_output_by_lane: [next_output; 2],
            ingress_items: Vec::new(),
            buffered_items: Vec::new(),
            egress_items: Vec::new(),
        }
    }

    pub fn next_output_for_lane(&self, lane: usize) -> usize {
        let lane_output = self
            .next_output_by_lane
            .get(lane)
            .copied()
            .unwrap_or(usize::MAX);
        if lane_output < 2 {
            lane_output
        } else {
            self.next_output % 2
        }
    }

    pub fn set_next_output_for_lane(&mut self, lane: usize, next_output: usize) {
        let next_output = next_output % 2;
        self.ensure_lane_outputs_initialized();
        if let Some(lane_output) = self.next_output_by_lane.get_mut(lane) {
            *lane_output = next_output;
        }
        self.next_output = next_output;
    }

    pub fn set_next_output_for_all_lanes(&mut self, next_output: usize) {
        let next_output = next_output % 2;
        self.next_output = next_output;
        self.next_output_by_lane = [next_output; 2];
    }

    fn ensure_lane_outputs_initialized(&mut self) {
        if self.next_output_by_lane.iter().any(|output| *output >= 2) {
            self.next_output_by_lane = [self.next_output % 2; 2];
        }
    }
}

#[derive(Clone, Debug, Deserialize, PartialEq, Eq, Serialize)]
pub struct UndergroundTransportItem {
    pub item: ItemKindId,
    pub lane: usize,
    pub progress: DistanceUnits,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Eq, Serialize)]
pub struct UndergroundTransportRuntime {
    pub distance: DistanceUnits,
    pub items: Vec<UndergroundTransportItem>,
}

impl UndergroundTransportRuntime {
    pub fn empty(distance: DistanceUnits) -> Self {
        Self {
            distance,
            items: Vec::new(),
        }
    }
}

#[derive(Clone, Debug, Default, Deserialize, PartialEq, Eq, Serialize)]
pub enum TransportNodeRuntime {
    #[default]
    None,
    Splitter(SplitterRuntime),
    Underground(UndergroundTransportRuntime),
}

#[derive(Clone, Copy, Debug, Deserialize, PartialEq, Eq, PartialOrd, Ord, Serialize)]
pub struct TransportNodeKey {
    pub sort_tile: TilePos,
    pub kind_order: u8,
    pub lane_order: usize,
    pub source_line: Option<LineId>,
    pub target_line: Option<LineId>,
    pub node: TransportNodeId,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Eq, Serialize)]
pub struct TransportNode {
    pub id: TransportNodeId,
    pub kind: TransportNodeKind,
    pub sort_tile: TilePos,
    pub direction: Option<Direction>,
    pub ports: Vec<TransportPort>,
    pub runtime: TransportNodeRuntime,
}

impl TransportNode {
    pub fn blocked_front(id: TransportNodeId, sort_tile: TilePos, source: LineId) -> Self {
        Self {
            id,
            kind: TransportNodeKind::BlockedFront,
            sort_tile,
            direction: None,
            ports: vec![TransportPort {
                node: id,
                role: TransportPortRole::Input,
                tile: sort_tile,
                side: None,
                lane: 0,
                line: source,
            }],
            runtime: TransportNodeRuntime::None,
        }
    }

    pub fn end_transfer(
        id: TransportNodeId,
        sort_tile: TilePos,
        source: LineId,
        target: LineId,
    ) -> Self {
        let ports = [TransportPortRole::Input, TransportPortRole::Output]
            .into_iter()
            .enumerate()
            .map(|(index, role)| TransportPort {
                node: id,
                role,
                tile: sort_tile,
                side: None,
                lane: 0,
                line: if index == 0 { source } else { target },
            })
            .collect();
        Self {
            id,
            kind: TransportNodeKind::EndTransfer,
            sort_tile,
            direction: None,
            ports,
            runtime: TransportNodeRuntime::None,
        }
    }

    /// Same-tile compatibility helper; use `side_load_to` when source and target tiles differ.
    pub fn side_load(
        id: TransportNodeId,
        sort_tile: TilePos,
        source: LineId,
        target: LineId,
        near_lane: usize,
    ) -> Self {
        Self::side_load_to(id, sort_tile, sort_tile, source, target, near_lane)
    }

    pub fn side_load_to(
        id: TransportNodeId,
        source_sort_tile: TilePos,
        target_tile: TilePos,
        source: LineId,
        target: LineId,
        near_lane: usize,
    ) -> Self {
        Self {
            id,
            kind: TransportNodeKind::SideLoad { near_lane },
            sort_tile: source_sort_tile,
            direction: None,
            ports: vec![
                TransportPort {
                    node: id,
                    role: TransportPortRole::Input,
                    tile: source_sort_tile,
                    side: None,
                    lane: 0,
                    line: source,
                },
                TransportPort {
                    node: id,
                    role: TransportPortRole::Output,
                    tile: target_tile,
                    side: None,
                    lane: near_lane,
                    line: target,
                },
            ],
            runtime: TransportNodeRuntime::None,
        }
    }

    pub fn splitter_2x1(
        id: TransportNodeId,
        origin: TilePos,
        direction: Direction,
        input_left: LineId,
        input_right: LineId,
        output_left: LineId,
        output_right: LineId,
    ) -> Self {
        Self::splitter_2x1_with_channel_tiles(
            id,
            origin,
            direction,
            splitter_channel_tiles(origin, direction),
            splitter_output_tiles(origin, direction),
            [input_left, input_right],
            [output_left, output_right],
        )
    }

    pub fn splitter_2x1_with_channel_tiles(
        id: TransportNodeId,
        sort_tile: TilePos,
        direction: Direction,
        input_tiles: [TilePos; 2],
        output_tiles: [TilePos; 2],
        input_lines: [LineId; 2],
        output_lines: [LineId; 2],
    ) -> Self {
        let mut ports = Vec::with_capacity(8);
        for (channel, input_tile) in input_tiles.into_iter().enumerate() {
            for lane in 0..2 {
                ports.push(TransportPort {
                    node: id,
                    role: TransportPortRole::Input,
                    tile: input_tile,
                    side: Some(direction.opposite()),
                    lane,
                    line: input_lines[channel],
                });
            }
        }
        for (channel, output_tile) in output_tiles.into_iter().enumerate() {
            for lane in 0..2 {
                ports.push(TransportPort {
                    node: id,
                    role: TransportPortRole::Output,
                    tile: output_tile,
                    side: Some(direction),
                    lane,
                    line: output_lines[channel],
                });
            }
        }

        Self {
            id,
            kind: TransportNodeKind::Splitter2x1,
            sort_tile,
            direction: Some(direction),
            ports,
            runtime: TransportNodeRuntime::Splitter(SplitterRuntime::default()),
        }
    }

    pub fn underground(
        id: TransportNodeId,
        entrance: TilePos,
        exit: TilePos,
        direction: Direction,
        input_line: LineId,
        output_line: LineId,
        distance: DistanceUnits,
    ) -> Self {
        let mut ports = Vec::with_capacity(4);
        for lane in 0..2 {
            ports.push(TransportPort {
                node: id,
                role: TransportPortRole::Input,
                tile: entrance,
                side: Some(direction.opposite()),
                lane,
                line: input_line,
            });
        }
        for lane in 0..2 {
            ports.push(TransportPort {
                node: id,
                role: TransportPortRole::Output,
                tile: exit,
                side: Some(direction),
                lane,
                line: output_line,
            });
        }
        Self {
            id,
            kind: TransportNodeKind::Underground,
            sort_tile: entrance,
            direction: Some(direction),
            ports,
            runtime: TransportNodeRuntime::Underground(UndergroundTransportRuntime::empty(
                distance,
            )),
        }
    }

    pub fn conveyor_lift(
        id: TransportNodeId,
        origin: TilePos,
        output: TilePos,
        direction: Direction,
        input_line: LineId,
        output_line: LineId,
        distance: DistanceUnits,
    ) -> Self {
        let mut ports = Vec::with_capacity(4);
        for lane in 0..2 {
            ports.push(TransportPort {
                node: id,
                role: TransportPortRole::Input,
                tile: origin,
                side: Some(direction.opposite()),
                lane,
                line: input_line,
            });
        }
        for lane in 0..2 {
            ports.push(TransportPort {
                node: id,
                role: TransportPortRole::Output,
                tile: output,
                side: Some(direction),
                lane,
                line: output_line,
            });
        }
        Self {
            id,
            kind: TransportNodeKind::ConveyorLift,
            sort_tile: origin,
            direction: Some(direction),
            ports,
            runtime: TransportNodeRuntime::Underground(UndergroundTransportRuntime::empty(
                distance,
            )),
        }
    }

    pub fn key(&self) -> TransportNodeKey {
        TransportNodeKey {
            sort_tile: self.sort_tile,
            kind_order: self.kind.sort_order(),
            lane_order: self.lane_order(),
            source_line: self.first_port_line(TransportPortRole::Input),
            target_line: self.first_port_line(TransportPortRole::Output),
            node: self.id,
        }
    }

    fn lane_order(&self) -> usize {
        match self.kind {
            TransportNodeKind::SideLoad { near_lane } => near_lane,
            TransportNodeKind::BlockedFront
            | TransportNodeKind::EndTransfer
            | TransportNodeKind::Splitter2x1
            | TransportNodeKind::Underground
            | TransportNodeKind::ConveyorLift => self
                .ports
                .iter()
                .find(|port| port.role == TransportPortRole::Input)
                .map_or(0, |port| port.lane),
        }
    }

    fn first_port_line(&self, role: TransportPortRole) -> Option<LineId> {
        self.ports
            .iter()
            .find(|port| port.role == role)
            .map(|port| port.line)
    }

    pub fn input_ports(&self) -> impl Iterator<Item = &TransportPort> {
        self.ports
            .iter()
            .filter(|port| port.role == TransportPortRole::Input)
    }

    pub fn output_ports(&self) -> impl Iterator<Item = &TransportPort> {
        self.ports
            .iter()
            .filter(|port| port.role == TransportPortRole::Output)
    }
}

fn splitter_channel_tiles(origin: TilePos, direction: Direction) -> [TilePos; 2] {
    match direction {
        Direction::East | Direction::West => [origin, TilePos::new(origin.x, origin.y + 1)],
        Direction::North | Direction::South => [origin, TilePos::new(origin.x + 1, origin.y)],
    }
}

fn splitter_output_tiles(origin: TilePos, direction: Direction) -> [TilePos; 2] {
    splitter_channel_tiles(origin, direction).map(|tile| direction.output_pos(tile))
}

impl TransportPort {
    pub const fn as_ref(self) -> TransportPortRef {
        TransportPortRef {
            node: self.node,
            line: self.line,
            lane: self.lane,
            role: self.role,
        }
    }
}

#[derive(Clone, Copy, Debug, Deserialize, PartialEq, Eq, Serialize)]
pub struct VisualRouteHint {
    pub item: ItemKindId,
    pub from: TransportPortRef,
    pub to: TransportPortRef,
    pub center: TilePos,
    pub target_tile: TilePos,
    pub progress_numerator: u16,
    pub progress_denominator: u16,
    pub start_tick: u64,
    pub end_tick: u64,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ids::{ItemKindId, LineId, TilePos};
    use crate::topology::graph::Direction;

    #[test]
    fn transport_node_key_sorts_by_tile_kind_semantics_and_id() {
        let nodes = [
            TransportNode::blocked_front(TransportNodeId(3), TilePos::new(1, 0), LineId(10)),
            TransportNode::end_transfer(
                TransportNodeId(2),
                TilePos::new(0, 0),
                LineId(1),
                LineId(2),
            ),
            TransportNode::side_load(
                TransportNodeId(1),
                TilePos::new(0, 0),
                LineId(3),
                LineId(4),
                1,
            ),
            TransportNode::side_load(
                TransportNodeId(90),
                TilePos::new(0, 0),
                LineId(9),
                LineId(5),
                0,
            ),
            TransportNode::side_load(
                TransportNodeId(80),
                TilePos::new(0, 0),
                LineId(9),
                LineId(4),
                0,
            ),
            TransportNode::side_load(
                TransportNodeId(70),
                TilePos::new(0, 0),
                LineId(8),
                LineId(5),
                0,
            ),
        ];
        let mut keys = nodes.map(|node| node.key());
        keys.sort();

        assert_eq!(keys[0].sort_tile, TilePos::new(0, 0));
        assert_eq!(
            keys[0].kind_order,
            TransportNodeKind::EndTransfer.sort_order()
        );
        assert_eq!(
            keys[1].kind_order,
            TransportNodeKind::SideLoad { near_lane: 1 }.sort_order()
        );
        assert_eq!(keys[1].lane_order, 0);
        assert_eq!(keys[1].source_line, Some(LineId(8)));
        assert_eq!(keys[1].target_line, Some(LineId(5)));
        assert_eq!(keys[2].lane_order, 0);
        assert_eq!(keys[2].source_line, Some(LineId(9)));
        assert_eq!(keys[2].target_line, Some(LineId(4)));
        assert_eq!(keys[3].lane_order, 0);
        assert_eq!(keys[3].source_line, Some(LineId(9)));
        assert_eq!(keys[3].target_line, Some(LineId(5)));
        assert_eq!(keys[4].lane_order, 1);
        assert_eq!(keys[5].sort_tile, TilePos::new(1, 0));
    }

    #[test]
    fn compatibility_nodes_do_not_claim_geometric_port_sides() {
        let nodes = [
            TransportNode::blocked_front(TransportNodeId(1), TilePos::new(0, 0), LineId(1)),
            TransportNode::end_transfer(
                TransportNodeId(2),
                TilePos::new(0, 0),
                LineId(2),
                LineId(3),
            ),
            TransportNode::side_load(
                TransportNodeId(3),
                TilePos::new(0, 0),
                LineId(4),
                LineId(5),
                1,
            ),
        ];

        assert!(
            nodes
                .iter()
                .flat_map(|node| node.ports.iter())
                .all(|port| port.side.is_none())
        );
    }

    #[test]
    fn side_load_to_preserves_source_sort_tile_and_target_tile() {
        let node = TransportNode::side_load_to(
            TransportNodeId(3),
            TilePos::new(-1, 0),
            TilePos::new(0, 0),
            LineId(4),
            LineId(5),
            1,
        );

        let inputs = node.input_ports().collect::<Vec<_>>();
        let outputs = node.output_ports().collect::<Vec<_>>();

        assert_eq!(node.sort_tile, TilePos::new(-1, 0));
        assert_eq!(inputs[0].tile, TilePos::new(-1, 0));
        assert_eq!(outputs[0].tile, TilePos::new(0, 0));
        assert_eq!(outputs[0].lane, 1);
    }

    #[test]
    fn splitter_ports_are_direction_relative() {
        assert_splitter_geometry(
            TransportNodeId(9),
            TilePos::new(4, 5),
            Direction::East,
            LineId(1),
            LineId(2),
            LineId(3),
            LineId(4),
            Direction::West,
            Direction::East,
            [TilePos::new(4, 5), TilePos::new(4, 6)],
            [TilePos::new(5, 5), TilePos::new(5, 6)],
        );
        assert_splitter_geometry(
            TransportNodeId(10),
            TilePos::new(4, 5),
            Direction::North,
            LineId(11),
            LineId(12),
            LineId(13),
            LineId(14),
            Direction::South,
            Direction::North,
            [TilePos::new(4, 5), TilePos::new(5, 5)],
            [TilePos::new(4, 6), TilePos::new(5, 6)],
        );
    }

    #[test]
    fn underground_ports_are_lane_preserving_endpoint_ports() {
        let node = TransportNode::underground(
            TransportNodeId(9),
            TilePos::new(0, 0),
            TilePos::new(4, 0),
            Direction::East,
            LineId(1),
            LineId(2),
            DistanceUnits::new(4 * DistanceUnits::UNITS_PER_TILE),
        );

        assert_eq!(node.kind, TransportNodeKind::Underground);
        assert_eq!(node.sort_tile, TilePos::new(0, 0));
        assert_eq!(node.direction, Some(Direction::East));

        let inputs = node.input_ports().collect::<Vec<_>>();
        let outputs = node.output_ports().collect::<Vec<_>>();
        assert_eq!(inputs.len(), 2);
        assert_eq!(outputs.len(), 2);

        assert_eq!(inputs[0].tile, TilePos::new(0, 0));
        assert_eq!(inputs[0].side, Some(Direction::West));
        assert_eq!(inputs[0].lane, 0);
        assert_eq!(inputs[0].line, LineId(1));
        assert_eq!(inputs[1].lane, 1);

        assert_eq!(outputs[0].tile, TilePos::new(4, 0));
        assert_eq!(outputs[0].side, Some(Direction::East));
        assert_eq!(outputs[0].lane, 0);
        assert_eq!(outputs[0].line, LineId(2));
        assert_eq!(outputs[1].lane, 1);

        assert_eq!(
            node.runtime,
            TransportNodeRuntime::Underground(UndergroundTransportRuntime {
                distance: DistanceUnits::new(4 * DistanceUnits::UNITS_PER_TILE),
                items: Vec::new(),
            })
        );
    }

    #[allow(clippy::too_many_arguments)]
    fn assert_splitter_geometry(
        id: TransportNodeId,
        origin: TilePos,
        direction: Direction,
        input_left: LineId,
        input_right: LineId,
        output_left: LineId,
        output_right: LineId,
        input_side: Direction,
        output_side: Direction,
        input_tiles: [TilePos; 2],
        output_tiles: [TilePos; 2],
    ) {
        let node = TransportNode::splitter_2x1(
            id,
            origin,
            direction,
            input_left,
            input_right,
            output_left,
            output_right,
        );

        let inputs = node.input_ports().collect::<Vec<_>>();
        let outputs = node.output_ports().collect::<Vec<_>>();

        assert_eq!(inputs.len(), 4);
        assert_eq!(outputs.len(), 4);
        assert!(
            inputs
                .iter()
                .all(|port| port.role == TransportPortRole::Input)
        );
        assert!(
            outputs
                .iter()
                .all(|port| port.role == TransportPortRole::Output)
        );
        assert_eq!(inputs[0].tile, input_tiles[0]);
        assert_eq!(inputs[0].side, Some(input_side));
        assert_eq!(inputs[0].lane, 0);
        assert_eq!(inputs[0].line, input_left);
        assert_eq!(inputs[1].tile, input_tiles[0]);
        assert_eq!(inputs[1].side, Some(input_side));
        assert_eq!(inputs[1].lane, 1);
        assert_eq!(inputs[1].line, input_left);
        assert_eq!(inputs[2].tile, input_tiles[1]);
        assert_eq!(inputs[2].side, Some(input_side));
        assert_eq!(inputs[2].lane, 0);
        assert_eq!(inputs[2].line, input_right);
        assert_eq!(inputs[3].tile, input_tiles[1]);
        assert_eq!(inputs[3].side, Some(input_side));
        assert_eq!(inputs[3].lane, 1);
        assert_eq!(inputs[3].line, input_right);
        assert_eq!(outputs[0].tile, output_tiles[0]);
        assert_eq!(outputs[0].side, Some(output_side));
        assert_eq!(outputs[0].lane, 0);
        assert_eq!(outputs[0].line, output_left);
        assert_eq!(outputs[1].tile, output_tiles[0]);
        assert_eq!(outputs[1].side, Some(output_side));
        assert_eq!(outputs[1].lane, 1);
        assert_eq!(outputs[1].line, output_left);
        assert_eq!(outputs[2].tile, output_tiles[1]);
        assert_eq!(outputs[2].side, Some(output_side));
        assert_eq!(outputs[2].lane, 0);
        assert_eq!(outputs[2].line, output_right);
        assert_eq!(outputs[3].tile, output_tiles[1]);
        assert_eq!(outputs[3].side, Some(output_side));
        assert_eq!(outputs[3].lane, 1);
        assert_eq!(outputs[3].line, output_right);
        assert_eq!(
            node.runtime,
            TransportNodeRuntime::Splitter(SplitterRuntime::default())
        );
    }

    #[test]
    fn route_hint_is_plain_render_data() {
        let hint = VisualRouteHint {
            item: ItemKindId(3),
            from: TransportPortRef {
                node: TransportNodeId(1),
                line: LineId(2),
                lane: 0,
                role: TransportPortRole::Input,
            },
            to: TransportPortRef {
                node: TransportNodeId(1),
                line: LineId(3),
                lane: 1,
                role: TransportPortRole::Output,
            },
            center: TilePos::new(0, 0),
            target_tile: TilePos::new(1, 0),
            progress_numerator: 32,
            progress_denominator: 128,
            start_tick: 10,
            end_tick: 11,
        };

        assert_eq!(hint.item, ItemKindId(3));
        assert_eq!(hint.from.lane, 0);
        assert_eq!(hint.to.lane, 1);
        assert_eq!(hint.target_tile, TilePos::new(1, 0));
        assert_eq!(hint.progress_numerator, 32);
        assert_eq!(hint.progress_denominator, 128);
    }
}
