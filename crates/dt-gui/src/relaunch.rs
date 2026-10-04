//! "Apply + relaunch": wait for the killed game to exit, launch it, wait for the new process.

use std::time::{Duration, Instant, SystemTime};

const TIMEOUT: Duration = Duration::from_secs(120);

#[derive(Clone, Debug, PartialEq, Eq, Default)]
pub enum Relaunch {
    #[default]
    Idle,
    WaitingExit {
        since: Instant,
    },
    WaitingStart {
        since: Instant,
        launched_at: SystemTime,
    },
    Failed(String),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Action {
    Launch,
}

/// What the app sees on a poll.
#[derive(Clone, Copy, Debug)]
pub struct Observation {
    pub running: bool,
    pub started_at: Option<SystemTime>,
    pub now: Instant,
    pub wall: SystemTime,
}

impl Relaunch {
    pub fn is_active(&self) -> bool {
        matches!(
            self,
            Relaunch::WaitingExit { .. } | Relaunch::WaitingStart { .. }
        )
    }

    pub fn elapsed(&self, now: Instant) -> Option<Duration> {
        match self {
            Relaunch::WaitingExit { since } | Relaunch::WaitingStart { since, .. } => {
                Some(now.duration_since(*since))
            }
            _ => None,
        }
    }

    pub fn step(self, obs: Observation) -> (Relaunch, Option<Action>) {
        if self.elapsed(obs.now).is_some_and(|e| e > TIMEOUT) {
            return (
                Relaunch::Failed("game did not restart within 2 minutes".into()),
                None,
            );
        }
        match self {
            Relaunch::WaitingExit { since } if !obs.running => (
                Relaunch::WaitingStart {
                    since,
                    launched_at: obs.wall,
                },
                Some(Action::Launch),
            ),
            // Process start times have one-second resolution.
            Relaunch::WaitingStart { launched_at, .. }
                if obs
                    .started_at
                    .is_some_and(|t| t + Duration::from_secs(1) >= launched_at) =>
            {
                (Relaunch::Idle, None)
            }
            other => (other, None),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn obs(
        running: bool,
        started_at: Option<SystemTime>,
        now: Instant,
        wall: SystemTime,
    ) -> Observation {
        Observation {
            running,
            started_at,
            now,
            wall,
        }
    }

    #[test]
    fn exit_then_launch_then_done() {
        let t0 = Instant::now();
        let w0 = SystemTime::now();
        let state = Relaunch::WaitingExit { since: t0 };
        let (state, action) = state.step(obs(true, Some(w0 - Duration::from_secs(600)), t0, w0));
        assert_eq!(action, None, "old process still alive");
        let (state, action) = state.step(obs(
            false,
            None,
            t0 + Duration::from_secs(2),
            w0 + Duration::from_secs(2),
        ));
        assert_eq!(action, Some(Action::Launch));
        assert!(matches!(state, Relaunch::WaitingStart { .. }));
        let (state, _) = state.step(obs(false, None, t0 + Duration::from_secs(5), w0));
        assert!(state.is_active(), "steam still starting the game");
        let (state, _) = state.step(obs(
            true,
            Some(w0 + Duration::from_secs(9)),
            t0 + Duration::from_secs(10),
            w0 + Duration::from_secs(10),
        ));
        assert_eq!(state, Relaunch::Idle);
    }

    #[test]
    fn gives_up_after_the_timeout() {
        let t0 = Instant::now();
        let (state, _) = Relaunch::WaitingExit { since: t0 }.step(obs(
            true,
            None,
            t0 + Duration::from_secs(121),
            SystemTime::now(),
        ));
        assert!(matches!(state, Relaunch::Failed(_)));
    }
}
