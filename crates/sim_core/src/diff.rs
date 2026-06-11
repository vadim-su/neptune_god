//! Per-tick change set for render and chunk systems (lines, chunks, topology revision).

use crate::ids::{ChunkPos, LineId};
use crate::transport::node::VisualRouteHint;

#[derive(Clone, Debug, Default, PartialEq, Eq)]
/// Lines and chunks that changed during the last tick (deduped before publish).
pub struct SimDiff {
    pub changed_lines: Vec<LineId>,
    pub changed_chunks: Vec<ChunkPos>,
    pub topology_revision: u64,
    pub route_hints: Vec<VisualRouteHint>,
}

impl SimDiff {
    pub fn sort_and_dedup(&mut self) {
        self.changed_lines.sort();
        self.changed_lines.dedup();
        self.changed_chunks.sort();
        self.changed_chunks.dedup();
    }
}
