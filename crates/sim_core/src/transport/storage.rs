//! All transport lines and per-tile belt interactions in the world.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

use crate::ids::{LineId, TilePos};
use crate::transport::interaction::{BeltInteraction, BeltInteractionKey};
use crate::transport::line::{TransportLine, TransportLineSnapshot};
use crate::transport::node::{
    SplitterRuntime, TransportNode, TransportNodeId, TransportNodeKey, TransportNodeRuntime,
    UndergroundTransportRuntime,
};

#[derive(Clone, Copy, Debug, Deserialize, PartialEq, Eq, Serialize)]
pub enum BeltInteractionKindSnapshot {
    BlockedFront,
    EndTransfer,
    SideLoad { near_lane: usize },
}

impl From<crate::transport::interaction::BeltInteractionKind> for BeltInteractionKindSnapshot {
    fn from(kind: crate::transport::interaction::BeltInteractionKind) -> Self {
        match kind {
            crate::transport::interaction::BeltInteractionKind::BlockedFront => Self::BlockedFront,
            crate::transport::interaction::BeltInteractionKind::EndTransfer => Self::EndTransfer,
            crate::transport::interaction::BeltInteractionKind::SideLoad { near_lane } => {
                Self::SideLoad { near_lane }
            }
        }
    }
}

impl From<BeltInteractionKindSnapshot> for crate::transport::interaction::BeltInteractionKind {
    fn from(kind: BeltInteractionKindSnapshot) -> Self {
        match kind {
            BeltInteractionKindSnapshot::BlockedFront => Self::BlockedFront,
            BeltInteractionKindSnapshot::EndTransfer => Self::EndTransfer,
            BeltInteractionKindSnapshot::SideLoad { near_lane } => Self::SideLoad { near_lane },
        }
    }
}

#[derive(Clone, Debug, Deserialize, PartialEq, Eq, Serialize)]
pub struct BeltInteractionSnapshot {
    pub kind: BeltInteractionKindSnapshot,
    pub source_line: LineId,
    pub target_line: Option<LineId>,
    pub target_tile: Option<TilePos>,
    pub target_sort_tile: TilePos,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Eq, Serialize)]
pub struct TransportStorageSnapshot {
    pub interactions: Vec<BeltInteractionSnapshot>,
    pub lines: BTreeMap<LineId, TransportLineSnapshot>,
    #[serde(default)]
    pub nodes: Vec<TransportNode>,
}

#[derive(Debug, Default)]
pub struct TransportStorage {
    interactions: BTreeMap<BeltInteractionKey, BeltInteraction>,
    lines: BTreeMap<LineId, TransportLine>,
    nodes: BTreeMap<TransportNodeKey, TransportNode>,
}

impl TransportStorage {
    pub fn insert_interaction(&mut self, interaction: BeltInteraction) {
        self.interactions.insert(interaction.key(), interaction);
    }
    pub fn insert_node(&mut self, node: TransportNode) {
        if let Some(existing_key) = self
            .nodes
            .iter()
            .find(|(_, existing)| existing.id == node.id)
            .map(|(key, _)| *key)
        {
            self.nodes.remove(&existing_key);
        }
        self.nodes.insert(node.key(), node);
    }
    pub fn insert_line(&mut self, line: TransportLine) {
        self.lines.insert(line.id(), line);
    }
    pub fn line(&self, id: LineId) -> Option<&TransportLine> {
        self.lines.get(&id)
    }
    pub fn line_mut(&mut self, id: LineId) -> Option<&mut TransportLine> {
        self.lines.get_mut(&id)
    }
    pub fn line_ids_sorted(&self) -> impl Iterator<Item = LineId> + '_ {
        self.lines.keys().copied()
    }
    pub fn interactions_sorted(&self) -> impl Iterator<Item = &BeltInteraction> + '_ {
        self.interactions.values()
    }
    pub fn nodes_sorted(&self) -> impl Iterator<Item = &TransportNode> + '_ {
        self.nodes.values()
    }
    pub fn splitter_runtime_mut(&mut self, id: TransportNodeId) -> Option<&mut SplitterRuntime> {
        self.nodes
            .values_mut()
            .find(|node| node.id == id)
            .and_then(|node| match &mut node.runtime {
                TransportNodeRuntime::Splitter(runtime) => Some(runtime),
                TransportNodeRuntime::None | TransportNodeRuntime::Underground(_) => None,
            })
    }
    pub fn underground_runtime_mut(
        &mut self,
        id: TransportNodeId,
    ) -> Option<&mut UndergroundTransportRuntime> {
        self.nodes
            .values_mut()
            .find(|node| node.id == id)
            .and_then(|node| match &mut node.runtime {
                TransportNodeRuntime::Underground(runtime) => Some(runtime),
                TransportNodeRuntime::None | TransportNodeRuntime::Splitter(_) => None,
            })
    }
    pub fn snapshot(&self) -> TransportStorageSnapshot {
        TransportStorageSnapshot {
            interactions: self
                .interactions_sorted()
                .map(|interaction| BeltInteractionSnapshot {
                    kind: interaction.kind().into(),
                    source_line: interaction.source_line(),
                    target_line: interaction.target_line(),
                    target_tile: interaction.target_tile(),
                    target_sort_tile: interaction.target_sort_tile(),
                })
                .collect(),
            lines: self
                .lines
                .iter()
                .map(|(id, line)| (*id, line.snapshot()))
                .collect(),
            nodes: self.nodes_sorted().cloned().collect(),
        }
    }

    pub fn from_snapshot(snapshot: TransportStorageSnapshot) -> Result<Self, String> {
        let mut storage = Self::default();
        for (id, line) in snapshot.lines {
            if id != line.id {
                return Err(format!(
                    "transport line snapshot key {:?} does not match line id {:?}",
                    id, line.id
                ));
            }
            storage.insert_line(
                TransportLine::from_snapshot(line).map_err(|error| {
                    format!("failed to restore transport line {:?}: {error}", id)
                })?,
            );
        }
        for interaction in snapshot.interactions {
            storage.insert_interaction(BeltInteraction::new(
                interaction.kind.into(),
                interaction.source_line,
                interaction.target_line,
                interaction.target_tile,
                interaction.target_sort_tile,
            ));
        }
        for node in snapshot.nodes {
            storage.insert_node(node);
        }
        Ok(storage)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ids::{GroupId, ItemKindId, LineId, TilePos};
    use crate::topology::graph::Direction;
    use crate::transport::interaction::{BeltInteraction, BeltInteractionKind};
    use crate::transport::line::{LineEndpoint, LinePath, LineTile, TransportLine};
    use crate::transport::node::{
        SplitterRuntime, TransportNode, TransportNodeId, TransportNodeKind, TransportNodeRuntime,
        UndergroundTransportRuntime,
    };
    use crate::transport::stream::PackedItemStream;
    use crate::units::{DistanceUnits, UnitsPerTick};

    fn interaction(
        kind: BeltInteractionKind,
        source_line: LineId,
        target_line: Option<LineId>,
    ) -> BeltInteraction {
        BeltInteraction::new(
            kind,
            source_line,
            target_line,
            Some(TilePos::new(99, 99)),
            TilePos::new(4, 0),
        )
    }

    #[test]
    fn transport_storage_snapshot_has_no_ports_field() {
        let storage = TransportStorage::default();
        let snapshot = storage.snapshot();
        let TransportStorageSnapshot {
            interactions,
            lines,
            nodes,
        } = snapshot;
        assert!(interactions.is_empty());
        assert!(lines.is_empty());
        assert!(nodes.is_empty());
    }

    #[test]
    fn storage_iterates_lines_in_stable_id_order() {
        let mut storage = TransportStorage::default();
        storage.insert_line(TransportLine::new(
            LineId(7),
            GroupId(2),
            LinePath::new(vec![LineTile::new(TilePos::new(7, 0))]),
            UnitsPerTick::new(8),
            [PackedItemStream::default(), PackedItemStream::default()],
            LineEndpoint::Blocked,
            LineEndpoint::Open,
        ));
        storage.insert_line(TransportLine::new(
            LineId(3),
            GroupId(1),
            LinePath::new(vec![LineTile::new(TilePos::new(3, 0))]),
            UnitsPerTick::new(8),
            [PackedItemStream::default(), PackedItemStream::default()],
            LineEndpoint::Open,
            LineEndpoint::Open,
        ));

        assert_eq!(
            storage.line_ids_sorted().collect::<Vec<_>>(),
            vec![LineId(3), LineId(7)]
        );
    }

    #[test]
    fn storage_iterates_interactions_in_stable_key_order() {
        let mut storage = TransportStorage::default();
        storage.insert_interaction(BeltInteraction::new(
            BeltInteractionKind::SideLoad { near_lane: 1 },
            LineId(9),
            Some(LineId(3)),
            Some(TilePos::new(4, 0)),
            TilePos::new(4, 0),
        ));
        storage.insert_interaction(BeltInteraction::new(
            BeltInteractionKind::BlockedFront,
            LineId(2),
            None,
            None,
            TilePos::new(1, 0),
        ));

        let interactions = storage.interactions_sorted().collect::<Vec<_>>();

        assert_eq!(interactions.len(), 2);
        assert_eq!(interactions[0].source_line(), LineId(2));
        assert_eq!(interactions[1].source_line(), LineId(9));
    }

    #[test]
    fn storage_iterates_nodes_in_stable_key_order() {
        let mut storage = TransportStorage::default();
        for node in [
            TransportNode::side_load(
                TransportNodeId(9),
                TilePos::new(0, 0),
                LineId(9),
                LineId(8),
                1,
            ),
            TransportNode::blocked_front(TransportNodeId(2), TilePos::new(1, 0), LineId(4)),
            TransportNode::splitter_2x1(
                TransportNodeId(4),
                TilePos::new(0, 0),
                Direction::East,
                LineId(10),
                LineId(11),
                LineId(12),
                LineId(13),
            ),
            TransportNode::end_transfer(
                TransportNodeId(3),
                TilePos::new(0, 0),
                LineId(1),
                LineId(2),
            ),
            TransportNode::side_load(
                TransportNodeId(7),
                TilePos::new(0, 0),
                LineId(7),
                LineId(8),
                0,
            ),
        ] {
            storage.insert_node(node);
        }

        let keys = storage
            .nodes_sorted()
            .map(|node| node.key())
            .collect::<Vec<_>>();

        assert_eq!(
            keys.iter()
                .map(|key| (
                    key.sort_tile,
                    key.kind_order,
                    key.lane_order,
                    key.source_line
                ))
                .collect::<Vec<_>>(),
            vec![
                (
                    TilePos::new(0, 0),
                    TransportNodeKind::EndTransfer.sort_order(),
                    0,
                    Some(LineId(1)),
                ),
                (
                    TilePos::new(0, 0),
                    TransportNodeKind::SideLoad { near_lane: 0 }.sort_order(),
                    0,
                    Some(LineId(7)),
                ),
                (
                    TilePos::new(0, 0),
                    TransportNodeKind::SideLoad { near_lane: 1 }.sort_order(),
                    1,
                    Some(LineId(9)),
                ),
                (
                    TilePos::new(0, 0),
                    TransportNodeKind::Splitter2x1.sort_order(),
                    0,
                    Some(LineId(10)),
                ),
                (
                    TilePos::new(1, 0),
                    TransportNodeKind::BlockedFront.sort_order(),
                    0,
                    Some(LineId(4)),
                ),
            ]
        );
    }

    #[test]
    fn storage_snapshot_restores_nodes() {
        let mut storage = TransportStorage::default();
        storage.insert_node(TransportNode::side_load(
            TransportNodeId(5),
            TilePos::new(1, 0),
            LineId(1),
            LineId(2),
            1,
        ));
        storage.insert_node(TransportNode::end_transfer(
            TransportNodeId(4),
            TilePos::new(0, 0),
            LineId(3),
            LineId(4),
        ));

        let snapshot = storage.snapshot();

        assert_eq!(
            snapshot
                .nodes
                .iter()
                .map(|node| node.id)
                .collect::<Vec<_>>(),
            vec![TransportNodeId(4), TransportNodeId(5)]
        );

        let restored = TransportStorage::from_snapshot(snapshot).expect("snapshot should restore");

        assert_eq!(
            restored
                .nodes_sorted()
                .map(|node| node.id)
                .collect::<Vec<_>>(),
            vec![TransportNodeId(4), TransportNodeId(5)]
        );
    }

    #[test]
    fn storage_mutates_splitter_runtime_without_changing_node_key() {
        let mut storage = TransportStorage::default();
        let splitter_id = TransportNodeId(4);
        let transfer_id = TransportNodeId(5);
        storage.insert_node(TransportNode::splitter_2x1(
            splitter_id,
            TilePos::new(0, 0),
            Direction::East,
            LineId(1),
            LineId(2),
            LineId(3),
            LineId(4),
        ));
        storage.insert_node(TransportNode::end_transfer(
            transfer_id,
            TilePos::new(1, 0),
            LineId(5),
            LineId(6),
        ));
        let before_keys = storage
            .nodes_sorted()
            .map(|node| node.key())
            .collect::<Vec<_>>();

        assert!(storage.splitter_runtime_mut(transfer_id).is_none());
        storage
            .splitter_runtime_mut(splitter_id)
            .expect("splitter runtime should be mutable")
            .set_next_output_for_all_lanes(1);

        let after_keys = storage
            .nodes_sorted()
            .map(|node| node.key())
            .collect::<Vec<_>>();
        let splitter = storage
            .nodes_sorted()
            .find(|node| node.id == splitter_id)
            .expect("splitter should remain stored");

        assert_eq!(after_keys, before_keys);
        assert_eq!(
            splitter.runtime,
            TransportNodeRuntime::Splitter(SplitterRuntime::with_next_output(1))
        );
    }

    #[test]
    fn storage_mutates_underground_runtime_without_changing_node_key() {
        let mut storage = TransportStorage::default();
        let underground_id = TransportNodeId(4);
        let transfer_id = TransportNodeId(5);
        storage.insert_node(TransportNode::underground(
            underground_id,
            TilePos::new(0, 0),
            TilePos::new(4, 0),
            Direction::East,
            LineId(1),
            LineId(2),
            DistanceUnits::new(4 * DistanceUnits::UNITS_PER_TILE),
        ));
        storage.insert_node(TransportNode::end_transfer(
            transfer_id,
            TilePos::new(1, 0),
            LineId(5),
            LineId(6),
        ));
        let before_keys = storage
            .nodes_sorted()
            .map(|node| node.key())
            .collect::<Vec<_>>();

        assert!(storage.underground_runtime_mut(transfer_id).is_none());
        storage
            .underground_runtime_mut(underground_id)
            .expect("underground runtime should be mutable")
            .distance = DistanceUnits::new(5 * DistanceUnits::UNITS_PER_TILE);

        let after_keys = storage
            .nodes_sorted()
            .map(|node| node.key())
            .collect::<Vec<_>>();
        let underground = storage
            .nodes_sorted()
            .find(|node| node.id == underground_id)
            .expect("underground should remain stored");

        assert_eq!(after_keys, before_keys);
        assert_eq!(
            underground.runtime,
            TransportNodeRuntime::Underground(UndergroundTransportRuntime {
                distance: DistanceUnits::new(5 * DistanceUnits::UNITS_PER_TILE),
                items: Vec::new(),
            })
        );
    }

    #[test]
    fn storage_replaces_existing_node_with_same_id() {
        let mut storage = TransportStorage::default();
        let node_id = TransportNodeId(42);
        storage.insert_node(TransportNode::end_transfer(
            node_id,
            TilePos::new(0, 0),
            LineId(1),
            LineId(2),
        ));
        storage.insert_node(TransportNode::splitter_2x1(
            node_id,
            TilePos::new(9, 0),
            Direction::East,
            LineId(3),
            LineId(4),
            LineId(5),
            LineId(6),
        ));

        let nodes = storage.nodes_sorted().collect::<Vec<_>>();

        assert_eq!(nodes.len(), 1);
        assert_eq!(nodes[0].id, node_id);
        assert_eq!(nodes[0].kind, TransportNodeKind::Splitter2x1);
        assert_eq!(nodes[0].sort_tile, TilePos::new(9, 0));

        storage
            .splitter_runtime_mut(node_id)
            .expect("replacement splitter runtime should be unambiguous")
            .set_next_output_for_all_lanes(1);

        let node = storage.nodes_sorted().next().expect("node remains stored");
        assert_eq!(
            node.runtime,
            TransportNodeRuntime::Splitter(SplitterRuntime::with_next_output(1))
        );
    }

    #[test]
    fn storage_iterates_interactions_by_semantic_key_fields_for_same_target_sort_tile() {
        let mut storage = TransportStorage::default();
        for interaction in [
            interaction(
                BeltInteractionKind::SideLoad { near_lane: 1 },
                LineId(0),
                None,
            ),
            interaction(
                BeltInteractionKind::SideLoad { near_lane: 0 },
                LineId(3),
                Some(LineId(9)),
            ),
            interaction(
                BeltInteractionKind::SideLoad { near_lane: 0 },
                LineId(2),
                Some(LineId(8)),
            ),
            interaction(BeltInteractionKind::EndTransfer, LineId(9), Some(LineId(9))),
            interaction(BeltInteractionKind::BlockedFront, LineId(9), None),
            interaction(
                BeltInteractionKind::SideLoad { near_lane: 0 },
                LineId(9),
                None,
            ),
            interaction(
                BeltInteractionKind::SideLoad { near_lane: 0 },
                LineId(3),
                Some(LineId(4)),
            ),
        ] {
            storage.insert_interaction(interaction);
        }

        let interactions = storage
            .interactions_sorted()
            .map(|interaction| {
                (
                    interaction.kind(),
                    interaction.source_line(),
                    interaction.target_line(),
                )
            })
            .collect::<Vec<_>>();

        assert_eq!(
            interactions,
            vec![
                (BeltInteractionKind::BlockedFront, LineId(9), None),
                (BeltInteractionKind::EndTransfer, LineId(9), Some(LineId(9))),
                (
                    BeltInteractionKind::SideLoad { near_lane: 0 },
                    LineId(2),
                    Some(LineId(8)),
                ),
                (
                    BeltInteractionKind::SideLoad { near_lane: 0 },
                    LineId(3),
                    Some(LineId(4)),
                ),
                (
                    BeltInteractionKind::SideLoad { near_lane: 0 },
                    LineId(3),
                    Some(LineId(9)),
                ),
                (
                    BeltInteractionKind::SideLoad { near_lane: 0 },
                    LineId(9),
                    None
                ),
                (
                    BeltInteractionKind::SideLoad { near_lane: 1 },
                    LineId(0),
                    None
                ),
            ]
        );
    }

    #[test]
    fn line_revision_changes_when_lane_advances() {
        let mut line = TransportLine::new(
            LineId(1),
            GroupId(1),
            LinePath::new(vec![
                LineTile::new(TilePos::new(0, 0)),
                LineTile::new(TilePos::new(1, 0)),
            ]),
            UnitsPerTick::new(8),
            [
                PackedItemStream::from_gaps(
                    vec![ItemKindId(1)],
                    DistanceUnits::new(64),
                    vec![],
                    DistanceUnits::new(128),
                ),
                PackedItemStream::default(),
            ],
            LineEndpoint::Open,
            LineEndpoint::Open,
        );

        let report = line.advance();

        assert_eq!(report.items_scanned, 0);
        assert_eq!(line.revision(), 1);
        assert_eq!(line.lane(0).front_gap(), DistanceUnits::new(56));
    }

    #[test]
    fn blocked_line_stays_awake_until_all_lanes_are_compressed() {
        let mut line = TransportLine::new(
            LineId(1),
            GroupId(1),
            LinePath::new(vec![LineTile::new(TilePos::new(0, 0))]),
            UnitsPerTick::new(8),
            [
                PackedItemStream::from_gaps(
                    vec![ItemKindId(1), ItemKindId(1)],
                    DistanceUnits::ZERO,
                    vec![DistanceUnits::new(64)],
                    DistanceUnits::ZERO,
                ),
                PackedItemStream::from_gaps(
                    vec![ItemKindId(1), ItemKindId(1)],
                    DistanceUnits::ZERO,
                    vec![DistanceUnits::new(80)],
                    DistanceUnits::ZERO,
                ),
            ],
            LineEndpoint::Blocked,
            LineEndpoint::Open,
        );

        let report = line.advance();

        assert!(report.became_compressed);
        assert!(!line.sleeping());
        assert!(line.lane(0).is_fully_compressed());
        assert!(!line.lane(1).is_fully_compressed());

        let report = line.advance();

        assert!(report.became_compressed);
        assert!(line.sleeping());
        assert!(line.lane(1).is_fully_compressed());
    }
}
