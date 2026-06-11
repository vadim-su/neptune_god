//! Inserter pickup/drop candidate search and nearest-target selection.

use crate::catalog::{CoreInventoryRole, CoreItemStack};
use crate::ids::{BuildingId, ItemKindId, TilePos};
use crate::transport::node::TransportNodeId;
use crate::view::VisibleSplitterItemPhase;

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum CandidateRoleOrder {
    UndergroundBelt = 0,
    SplitterBelt = 1,
    Belt = 2,
    Input = 3,
    Fuel = 4,
    Output = 5,
    Storage = 6,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum InserterCandidateKind {
    Belt {
        lane: usize,
        slot: usize,
    },
    Splitter {
        node: TransportNodeId,
        phase: VisibleSplitterItemPhase,
        channel: usize,
        lane: usize,
    },
    Underground {
        node: TransportNodeId,
        lane: usize,
    },
    Inventory {
        role: CoreInventoryRole,
        slot: usize,
    },
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct InserterCandidate {
    pub tile: TilePos,
    pub owner: BuildingId,
    pub role_order: CandidateRoleOrder,
    pub lane_or_slot: usize,
    pub kind: InserterCandidateKind,
}

pub fn choose_nearest_candidate(
    origin: TilePos,
    mut candidates: Vec<InserterCandidate>,
) -> Option<InserterCandidate> {
    candidates.sort_by_key(|candidate| {
        let dx = candidate.tile.x - origin.x;
        let dy = candidate.tile.y - origin.y;
        let distance_sq = dx * dx + dy * dy;
        (
            distance_sq,
            candidate.tile.y,
            candidate.tile.x,
            candidate.role_order,
            candidate.lane_or_slot,
            candidate.owner,
        )
    });
    candidates.into_iter().next()
}

pub fn carried_stack(kind: ItemKindId) -> CoreItemStack {
    CoreItemStack { kind, amount: 1 }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::catalog::{CoreInventoryRole, TEST_IRON_ORE};
    use crate::ids::{BuildingId, TilePos};

    #[test]
    fn nearest_candidate_uses_distance_then_stable_tie_breaker() {
        let candidates = vec![
            InserterCandidate {
                tile: TilePos::new(2, 1),
                owner: BuildingId(7),
                role_order: CandidateRoleOrder::Storage,
                lane_or_slot: 0,
                kind: InserterCandidateKind::Inventory {
                    role: CoreInventoryRole::Storage,
                    slot: 0,
                },
            },
            InserterCandidate {
                tile: TilePos::new(1, 2),
                owner: BuildingId(8),
                role_order: CandidateRoleOrder::Storage,
                lane_or_slot: 0,
                kind: InserterCandidateKind::Inventory {
                    role: CoreInventoryRole::Storage,
                    slot: 0,
                },
            },
        ];

        assert_eq!(
            choose_nearest_candidate(TilePos::new(1, 1), candidates)
                .unwrap()
                .tile,
            TilePos::new(2, 1)
        );
    }

    #[test]
    fn nearest_candidate_uses_role_lane_and_owner_for_exact_ties() {
        let candidates = vec![
            InserterCandidate {
                tile: TilePos::new(1, 0),
                owner: BuildingId(9),
                role_order: CandidateRoleOrder::Storage,
                lane_or_slot: 2,
                kind: InserterCandidateKind::Inventory {
                    role: CoreInventoryRole::Storage,
                    slot: 2,
                },
            },
            InserterCandidate {
                tile: TilePos::new(1, 0),
                owner: BuildingId(4),
                role_order: CandidateRoleOrder::Input,
                lane_or_slot: 0,
                kind: InserterCandidateKind::Inventory {
                    role: CoreInventoryRole::Input,
                    slot: 0,
                },
            },
            InserterCandidate {
                tile: TilePos::new(1, 0),
                owner: BuildingId(2),
                role_order: CandidateRoleOrder::Belt,
                lane_or_slot: 1,
                kind: InserterCandidateKind::Belt { lane: 1, slot: 0 },
            },
            InserterCandidate {
                tile: TilePos::new(1, 0),
                owner: BuildingId(1),
                role_order: CandidateRoleOrder::Belt,
                lane_or_slot: 0,
                kind: InserterCandidateKind::Belt { lane: 0, slot: 0 },
            },
        ];

        let chosen = choose_nearest_candidate(TilePos::new(0, 0), candidates).unwrap();
        assert_eq!(chosen.owner, BuildingId(1));
        assert_eq!(
            chosen.kind,
            InserterCandidateKind::Belt { lane: 0, slot: 0 }
        );
    }

    #[test]
    fn nearest_candidate_prioritizes_transport_runtime_sources_before_surface_belts() {
        let candidates = vec![
            InserterCandidate {
                tile: TilePos::new(1, 0),
                owner: BuildingId(1),
                role_order: CandidateRoleOrder::Belt,
                lane_or_slot: 0,
                kind: InserterCandidateKind::Belt { lane: 0, slot: 0 },
            },
            InserterCandidate {
                tile: TilePos::new(1, 0),
                owner: BuildingId(2),
                role_order: CandidateRoleOrder::SplitterBelt,
                lane_or_slot: 0,
                kind: InserterCandidateKind::Splitter {
                    node: TransportNodeId(2),
                    phase: VisibleSplitterItemPhase::Ingress,
                    channel: 0,
                    lane: 0,
                },
            },
            InserterCandidate {
                tile: TilePos::new(1, 0),
                owner: BuildingId(3),
                role_order: CandidateRoleOrder::UndergroundBelt,
                lane_or_slot: 2,
                kind: InserterCandidateKind::Underground {
                    node: TransportNodeId(3),
                    lane: 1,
                },
            },
        ];

        let chosen = choose_nearest_candidate(TilePos::new(0, 0), candidates).unwrap();
        assert_eq!(
            chosen.kind,
            InserterCandidateKind::Underground {
                node: TransportNodeId(3),
                lane: 1,
            }
        );
    }

    #[test]
    fn carried_stack_is_one_item() {
        assert_eq!(carried_stack(TEST_IRON_ORE).amount, 1);
        assert_eq!(carried_stack(TEST_IRON_ORE).kind, TEST_IRON_ORE);
    }
}
