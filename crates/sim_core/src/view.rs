//! Read-only queries for visible belt items and tile bounds (render adapter input).

use crate::ids::{ItemKindId, TilePos};
use crate::topology::graph::Direction;
use crate::transport::node::VisualRouteHint;
use crate::world::SimWorld;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct VisibleTileBounds {
    min: TilePos,
    max: TilePos,
}

impl VisibleTileBounds {
    pub const fn new(min: TilePos, max: TilePos) -> Self {
        Self { min, max }
    }

    pub const fn min(self) -> TilePos {
        self.min
    }

    pub const fn max(self) -> TilePos {
        self.max
    }

    pub fn contains(self, pos: TilePos) -> bool {
        pos.x >= self.min.x && pos.x <= self.max.x && pos.y >= self.min.y && pos.y <= self.max.y
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct VisibleItem {
    pub tile: TilePos,
    pub item: ItemKindId,
    pub lane: usize,
    pub entry_direction: Direction,
    pub direction: Direction,
    pub progress_numerator: u16,
    pub progress_denominator: u16,
    pub route_hint: Option<VisualRouteHint>,
    pub splitter_route_hint: Option<VisualSplitterRouteHint>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct VisualSplitterRouteHint {
    pub input_tile: TilePos,
    pub output_tile: TilePos,
    pub phase: VisibleSplitterItemPhase,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum VisibleSplitterItemPhase {
    Ingress,
    Egress,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct VisibleSplitterItem {
    pub origin: TilePos,
    pub item: ItemKindId,
    pub direction: Direction,
    pub input_channel: usize,
    pub output_channel: usize,
    pub lane: usize,
    pub phase: VisibleSplitterItemPhase,
    pub progress_numerator: u16,
    pub progress_denominator: u16,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct SimRenderView {
    pub visible_items: Vec<VisibleItem>,
    pub visible_splitter_items: Vec<VisibleSplitterItem>,
}

impl SimRenderView {
    pub fn extract(world: &SimWorld, bounds: VisibleTileBounds) -> Self {
        let mut visible_items = Vec::new();
        for item in world.visible_items_for_bounds(bounds) {
            visible_items.push(item);
        }
        let visible_splitter_items = world.visible_splitter_items_for_bounds(bounds).collect();
        Self {
            visible_items,
            visible_splitter_items,
        }
    }
}
