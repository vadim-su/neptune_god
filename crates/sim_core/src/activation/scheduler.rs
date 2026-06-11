//! Tracks which transport lines are active for the current tick.

use std::collections::BTreeSet;

use serde::{Deserialize, Serialize};

use crate::ids::LineId;

#[derive(Clone, Debug, Deserialize, PartialEq, Eq, Serialize)]
pub struct ActivationSnapshot {
    pub active_lines: Vec<LineId>,
}

#[derive(Debug, Default)]
pub struct ActivationScheduler {
    active_lines: BTreeSet<LineId>,
}

impl ActivationScheduler {
    pub fn wake_line(&mut self, line: LineId) {
        self.active_lines.insert(line);
    }

    pub fn sleep_line(&mut self, line: LineId) {
        self.active_lines.remove(&line);
    }

    pub fn replace_active_lines(&mut self, lines: impl IntoIterator<Item = LineId>) {
        self.active_lines = lines.into_iter().collect();
    }

    pub fn active_lines(&self) -> impl Iterator<Item = LineId> + '_ {
        self.active_lines.iter().copied()
    }

    pub fn snapshot(&self) -> ActivationSnapshot {
        ActivationSnapshot {
            active_lines: self.active_lines.iter().copied().collect(),
        }
    }

    pub fn from_snapshot(snapshot: ActivationSnapshot) -> Self {
        Self {
            active_lines: snapshot.active_lines.into_iter().collect(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ids::LineId;

    #[test]
    fn scheduler_returns_active_lines_in_stable_order() {
        let mut scheduler = ActivationScheduler::default();
        scheduler.wake_line(LineId(9));
        scheduler.wake_line(LineId(3));
        scheduler.wake_line(LineId(3));

        assert_eq!(
            scheduler.active_lines().collect::<Vec<_>>(),
            vec![LineId(3), LineId(9)]
        );
    }

    #[test]
    fn replacing_active_lines_removes_stale_ids() {
        let mut scheduler = ActivationScheduler::default();
        scheduler.wake_line(LineId(1));
        scheduler.wake_line(LineId(2));

        scheduler.replace_active_lines([LineId(7), LineId(9)]);

        assert_eq!(
            scheduler.active_lines().collect::<Vec<_>>(),
            vec![LineId(7), LineId(9)]
        );
    }
}
