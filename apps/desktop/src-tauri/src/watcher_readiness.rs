//! Generation-scoped completion tracking for native watcher restarts.
//!
//! The native bridge starts asynchronously on the macOS main queue. A fixed
//! delay cannot prove that both watcher families published their terminal
//! status, so refresh commands wait on these explicit observations instead.

use std::{
    sync::{Condvar, Mutex, MutexGuard},
    time::{Duration, Instant},
};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum WatcherSource {
    Pasteboard,
    Selection,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct MissingWatcherSources {
    pub pasteboard: bool,
    pub selection: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum WatcherWaitOutcome {
    Ready,
    Superseded,
    TimedOut(MissingWatcherSources),
}

#[derive(Default)]
struct ReadinessState {
    generation: u64,
    pasteboard_observed: bool,
    selection_observed: bool,
}

#[derive(Default)]
pub struct WatcherReadiness {
    state: Mutex<ReadinessState>,
    changed: Condvar,
}

impl WatcherReadiness {
    /// Begin a fresh lifecycle. Any callback carrying an older generation is
    /// ignored and can never satisfy this lifecycle.
    pub fn begin(&self) -> Result<u64, String> {
        let mut state = self.lock()?;
        state.generation = state
            .generation
            .checked_add(1)
            .ok_or_else(|| "监听刷新代际计数已耗尽。".to_string())?;
        state.pasteboard_observed = false;
        state.selection_observed = false;
        let generation = state.generation;
        self.changed.notify_all();
        Ok(generation)
    }

    /// Record one terminal watcher status. Returns true only when this
    /// observation completes the current lifecycle.
    pub fn observe(&self, generation: u64, source: WatcherSource) -> Result<bool, String> {
        let mut state = self.lock()?;
        if generation == 0 || generation != state.generation {
            return Ok(false);
        }
        match source {
            WatcherSource::Pasteboard => state.pasteboard_observed = true,
            WatcherSource::Selection => state.selection_observed = true,
        }
        let ready = state.pasteboard_observed && state.selection_observed;
        self.changed.notify_all();
        Ok(ready)
    }

    pub fn is_current(&self, generation: u64) -> Result<bool, String> {
        let state = self.lock()?;
        Ok(generation != 0 && generation == state.generation)
    }

    pub fn wait_for(
        &self,
        generation: u64,
        timeout: Duration,
    ) -> Result<WatcherWaitOutcome, String> {
        let deadline = Instant::now()
            .checked_add(timeout)
            .ok_or_else(|| "监听刷新超时时间无效。".to_string())?;
        let mut state = self.lock()?;

        loop {
            if generation != state.generation {
                return Ok(WatcherWaitOutcome::Superseded);
            }
            if state.pasteboard_observed && state.selection_observed {
                return Ok(WatcherWaitOutcome::Ready);
            }

            let now = Instant::now();
            if now >= deadline {
                return Ok(WatcherWaitOutcome::TimedOut(MissingWatcherSources {
                    pasteboard: !state.pasteboard_observed,
                    selection: !state.selection_observed,
                }));
            }
            let remaining = deadline.saturating_duration_since(now);
            let (next_state, wait_result) = self
                .changed
                .wait_timeout(state, remaining)
                .map_err(|_| "监听刷新握手状态已损坏。".to_string())?;
            state = next_state;
            if wait_result.timed_out() {
                return Ok(WatcherWaitOutcome::TimedOut(MissingWatcherSources {
                    pasteboard: !state.pasteboard_observed,
                    selection: !state.selection_observed,
                }));
            }
        }
    }

    fn lock(&self) -> Result<MutexGuard<'_, ReadinessState>, String> {
        self.state
            .lock()
            .map_err(|_| "监听刷新握手状态已损坏。".to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::{sync::Arc, thread};

    #[test]
    fn waits_for_both_sources_in_any_order() {
        let readiness = Arc::new(WatcherReadiness::default());
        let generation = readiness.begin().expect("begin lifecycle");
        let worker = Arc::clone(&readiness);

        let handle = thread::spawn(move || {
            thread::sleep(Duration::from_millis(10));
            assert!(!worker
                .observe(generation, WatcherSource::Selection)
                .expect("observe selection"));
            thread::sleep(Duration::from_millis(10));
            assert!(worker
                .observe(generation, WatcherSource::Pasteboard)
                .expect("observe pasteboard"));
        });

        assert_eq!(
            readiness
                .wait_for(generation, Duration::from_secs(1))
                .expect("wait for watchers"),
            WatcherWaitOutcome::Ready
        );
        handle.join().expect("worker should finish");
    }

    #[test]
    fn old_generation_cannot_complete_new_refresh() {
        let readiness = WatcherReadiness::default();
        let old_generation = readiness.begin().expect("begin old lifecycle");
        let new_generation = readiness.begin().expect("begin new lifecycle");

        assert!(!readiness
            .is_current(old_generation)
            .expect("check old generation"));
        assert!(readiness
            .is_current(new_generation)
            .expect("check new generation"));
        assert!(!readiness
            .observe(old_generation, WatcherSource::Pasteboard)
            .expect("old pasteboard callback"));
        assert!(!readiness
            .observe(old_generation, WatcherSource::Selection)
            .expect("old selection callback"));
        assert_eq!(
            readiness
                .wait_for(new_generation, Duration::from_millis(1))
                .expect("wait for new lifecycle"),
            WatcherWaitOutcome::TimedOut(MissingWatcherSources {
                pasteboard: true,
                selection: true,
            })
        );
    }

    #[test]
    fn superseding_generation_wakes_existing_waiter() {
        let readiness = Arc::new(WatcherReadiness::default());
        let generation = readiness.begin().expect("begin lifecycle");
        let waiter = Arc::clone(&readiness);
        let handle = thread::spawn(move || {
            waiter
                .wait_for(generation, Duration::from_secs(5))
                .expect("wait outcome")
        });

        thread::sleep(Duration::from_millis(10));
        readiness.begin().expect("supersede lifecycle");
        assert_eq!(
            handle.join().expect("waiter should finish"),
            WatcherWaitOutcome::Superseded
        );
    }

    #[test]
    fn timeout_reports_only_missing_source() {
        let readiness = WatcherReadiness::default();
        let generation = readiness.begin().expect("begin lifecycle");
        readiness
            .observe(generation, WatcherSource::Selection)
            .expect("observe selection");

        assert_eq!(
            readiness
                .wait_for(generation, Duration::from_millis(1))
                .expect("wait outcome"),
            WatcherWaitOutcome::TimedOut(MissingWatcherSources {
                pasteboard: true,
                selection: false,
            })
        );
    }
}
