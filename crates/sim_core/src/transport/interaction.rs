//! Belt interaction rules at tile boundaries (end transfer, side load, blocked front).

use crate::ids::{LineId, TilePos};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
/// How two lines meet at a belt tile (ordering matches Factorio-style priority).
pub enum BeltInteractionKind {
    BlockedFront,
    EndTransfer,
    SideLoad { near_lane: usize },
}

impl BeltInteractionKind {
    pub const fn sort_order(self) -> u8 {
        match self {
            Self::BlockedFront => 0,
            Self::EndTransfer => 1,
            Self::SideLoad { .. } => 2,
        }
    }

    pub const fn lane_sort_key(self) -> usize {
        match self {
            Self::SideLoad { near_lane } => near_lane,
            Self::BlockedFront | Self::EndTransfer => 0,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub struct BeltInteractionKey {
    target_sort_tile: TilePos,
    kind_order: u8,
    lane_order: usize,
    source_line: LineId,
    target_line: Option<LineId>,
}

impl BeltInteractionKey {
    pub const fn new(
        target_sort_tile: TilePos,
        kind: BeltInteractionKind,
        source_line: LineId,
        target_line: Option<LineId>,
    ) -> Self {
        Self {
            target_sort_tile,
            kind_order: kind.sort_order(),
            lane_order: kind.lane_sort_key(),
            source_line,
            target_line,
        }
    }

    pub const fn target_sort_tile(&self) -> TilePos {
        self.target_sort_tile
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct BeltInteraction {
    kind: BeltInteractionKind,
    source_line: LineId,
    target_line: Option<LineId>,
    target_tile: Option<TilePos>,
    key: BeltInteractionKey,
}

impl BeltInteraction {
    pub const fn new(
        kind: BeltInteractionKind,
        source_line: LineId,
        target_line: Option<LineId>,
        target_tile: Option<TilePos>,
        target_sort_tile: TilePos,
    ) -> Self {
        let key = BeltInteractionKey::new(target_sort_tile, kind, source_line, target_line);
        Self {
            kind,
            source_line,
            target_line,
            target_tile,
            key,
        }
    }

    pub const fn kind(&self) -> BeltInteractionKind {
        self.kind
    }

    pub const fn source_line(&self) -> LineId {
        self.source_line
    }

    pub const fn target_line(&self) -> Option<LineId> {
        self.target_line
    }

    pub const fn target_tile(&self) -> Option<TilePos> {
        self.target_tile
    }

    pub const fn target_sort_tile(&self) -> TilePos {
        self.key.target_sort_tile()
    }

    pub const fn key(&self) -> BeltInteractionKey {
        self.key
    }
}
