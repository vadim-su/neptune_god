//! Behavior-host building tick and effect application into the world.

use super::*;
use crate::behavior_host::{
    BehaviorHost, BehaviorRuntimePolicy, BehaviorTickInput, apply_behavior_metrics,
    behavior_building, behavior_inventories, behavior_resources,
};
use crate::tick::{BehaviorEffectReport, BehaviorHostFailurePhase, CoreResourceDepletion};
use behavior_api::BehaviorCatalog;

impl SimWorld {
    #[allow(
        clippy::too_many_arguments,
        reason = "behavior tick bundles host context"
    )]
    pub(super) fn tick_behaviors(
        &mut self,
        metrics: &mut SimMetricsSnapshot,
        diff: &mut SimDiff,
        resource_depletions: &mut Vec<CoreResourceDepletion>,
        behavior_effect_reports: &mut Vec<BehaviorEffectReport>,
        behavior_host: &(impl BehaviorHost + ?Sized),
        behavior_catalog: &BehaviorCatalog,
        behavior_policy: BehaviorRuntimePolicy,
    ) {
        for building_id in self.buildings.keys().copied().collect::<Vec<_>>() {
            let Some(building) = self.buildings.get(&building_id).cloned() else {
                continue;
            };
            let SimBuildingState::Behavior(state) = building.state.clone() else {
                continue;
            };
            let behavior_id = Some(state.behavior_id.clone());
            if self.behavior_quarantine.contains_key(&building_id) {
                behavior_effect_reports.push(BehaviorEffectReport {
                    building: building_id,
                    origin: building.origin,
                    behavior_id,
                    application: crate::tick::BehaviorEffectApplication::Skipped {
                        reason: crate::tick::BehaviorTickSkipReason::Quarantined,
                    },
                });
                continue;
            }
            let Some(def) = self.catalog.building_by_id(&building.def_id) else {
                continue;
            };
            if !def.behavior.requires_behavior_host() {
                continue;
            }
            let config = def.behavior.config.clone();
            let inventories = self.take_building_inventories(building_id);
            let behavior_building = behavior_building(&building);
            let behavior_resources = behavior_resources(&self.resources);
            let power = self.energy.behavior_power_input(building_id);
            let result = match behavior_host.tick_behavior(BehaviorTickInput {
                catalog: behavior_catalog,
                building: &behavior_building,
                config: &config,
                state,
                inventories: behavior_inventories(inventories),
                resources: &behavior_resources,
                power,
                tick: self.tick.raw(),
            }) {
                Ok(result) => result,
                Err(error) => {
                    self.behavior_quarantine.insert(
                        building_id,
                        crate::tick::BehaviorEffectRejectionReason::HostError {
                            phase: BehaviorHostFailurePhase::Tick,
                            message: error.message.clone(),
                        },
                    );
                    behavior_effect_reports.push(BehaviorEffectReport {
                        building: building_id,
                        origin: building.origin,
                        behavior_id,
                        application: crate::tick::BehaviorEffectApplication::HostFailed {
                            phase: BehaviorHostFailurePhase::Tick,
                            error,
                        },
                    });
                    continue;
                }
            };
            apply_behavior_metrics(result.metrics, metrics);

            let effect_result = self.apply_behavior_effects(
                building_id,
                building.origin,
                behavior_id,
                result.effects,
                behavior_policy,
            );
            behavior_effect_reports.push(effect_result.report);
            for depletion in effect_result.resource_depletions {
                resource_depletions.push(depletion);
                diff.changed_chunks.push(depletion.pos.chunk_pos());
            }
            if result.output_changed {
                diff.changed_chunks.push(building.origin.chunk_pos());
            }
        }
    }

    pub(super) fn decrement_resource(&mut self, pos: TilePos) -> Option<CoreResourceDepletion> {
        if let Some((_, amount)) = self.resources.get_mut(&pos)
            && *amount > 0
        {
            *amount -= 1;
            return Some(CoreResourceDepletion {
                pos,
                remaining: *amount,
            });
        }

        None
    }

    pub(super) fn replace_behavior_state(
        &mut self,
        building: BuildingId,
        state: behavior_api::BehaviorInstanceState,
    ) {
        let Some(building) = self.buildings.get_mut(&building) else {
            return;
        };
        building.state = SimBuildingState::Behavior(state);
    }
}
