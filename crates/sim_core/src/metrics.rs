//! Per-tick simulation counters exposed to perf overlays and tests.

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
/// Counters accumulated during one tick (transport, behaviors, inserters, render).
pub struct SimMetricsSnapshot {
    pub sim_ticks: u64,
    pub active_lines: usize,
    pub active_interactions: usize,
    pub sleeping_lines: usize,
    pub dirty_chunks: usize,
    pub simulated_items: usize,
    pub items_scanned: usize,
    pub active_behaviors: usize,
    pub active_inserters: usize,
    pub fuel_starved_behaviors: usize,
    pub blocked_outputs: usize,
    pub inventory_transfers: usize,
    pub inserter_pickups: usize,
    pub inserter_drops: usize,
    pub behavior_effect_batches: usize,
    pub behavior_effects_applied: usize,
    pub behavior_effects_rejected: usize,
    pub behavior_instances_quarantined: usize,
    pub behavior_ticks_skipped: usize,
    pub behavior_host_errors: usize,
    pub visible_items: usize,
    pub render_entities_changed: usize,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn metrics_snapshot_has_explicit_render_boundary_fields() {
        let metrics = SimMetricsSnapshot {
            sim_ticks: 5,
            active_lines: 2,
            active_interactions: 1,
            sleeping_lines: 9,
            dirty_chunks: 3,
            simulated_items: 100_000,
            items_scanned: 0,
            active_behaviors: 0,
            active_inserters: 0,
            fuel_starved_behaviors: 0,
            blocked_outputs: 0,
            inventory_transfers: 0,
            inserter_pickups: 0,
            inserter_drops: 0,
            behavior_effect_batches: 2,
            behavior_effects_applied: 7,
            behavior_effects_rejected: 1,
            behavior_instances_quarantined: 1,
            behavior_ticks_skipped: 3,
            behavior_host_errors: 4,
            visible_items: 128,
            render_entities_changed: 4,
        };

        assert_eq!(metrics.simulated_items, 100_000);
        assert_eq!(metrics.behavior_effect_batches, 2);
        assert_eq!(metrics.behavior_effects_applied, 7);
        assert_eq!(metrics.behavior_effects_rejected, 1);
        assert_eq!(metrics.behavior_instances_quarantined, 1);
        assert_eq!(metrics.behavior_ticks_skipped, 3);
        assert_eq!(metrics.behavior_host_errors, 4);
        assert_eq!(metrics.visible_items, 128);
        assert_eq!(metrics.render_entities_changed, 4);
    }
}
