use std::time::{Duration, Instant};

use serde::{Deserialize, Serialize};

pub type TimerId = u64;

#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub enum TimerStyle {
    Bomb,
    Hourglass,
    Rocket,
    CodeRain,
    MachineCore,
    DragonOrb,
    CrescentWand,
}

impl TimerStyle {
    pub const ALL: [Self; 7] = [
        Self::Bomb,
        Self::Hourglass,
        Self::Rocket,
        Self::CodeRain,
        Self::MachineCore,
        Self::DragonOrb,
        Self::CrescentWand,
    ];

    pub fn label(self) -> &'static str {
        match self {
            Self::Bomb => "Bomb",
            Self::Hourglass => "Hourglass",
            Self::Rocket => "Rocket",
            Self::CodeRain => "Code Rain",
            Self::MachineCore => "Machine Core",
            Self::DragonOrb => "Dragon Orb",
            Self::CrescentWand => "Crescent Wand",
        }
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TimerSettings {
    pub label: String,
    pub duration: Duration,
    pub style: TimerStyle,
    pub size: f32,
    pub volume: f32,
    #[serde(default)]
    pub muted: bool,
    #[serde(default)]
    pub reduced_motion: bool,
}

impl Default for TimerSettings {
    fn default() -> Self {
        Self {
            label: "Focus".to_owned(),
            duration: Duration::from_secs(25 * 60),
            style: TimerStyle::Bomb,
            size: 300.0,
            volume: 0.6,
            muted: false,
            reduced_motion: false,
        }
    }
}

impl TimerSettings {
    pub fn validate(&self) -> Result<(), String> {
        if !(Duration::from_secs(1)..=Duration::from_secs(86_400)).contains(&self.duration)
            || self.duration.subsec_nanos() != 0
        {
            return Err("Duration must be whole seconds between 1 and 86400.".to_owned());
        }
        if !self.size.is_finite() || !(220.0..=460.0).contains(&self.size) {
            return Err("Widget size must be between 220 and 460 logical pixels.".to_owned());
        }
        if !self.volume.is_finite() || !(0.0..=1.0).contains(&self.volume) {
            return Err("Volume must be between 0 and 1.".to_owned());
        }
        if self.label.chars().count() > 80 {
            return Err("Label must contain at most 80 characters.".to_owned());
        }
        Ok(())
    }
}

/// A widget's outer position in physical desktop pixels, including negative monitor origins.
#[derive(Clone, Copy, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct WidgetPosition {
    pub x: f32,
    pub y: f32,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum TimerPhase {
    Idle,
    Running,
    Paused,
    Finished,
}

#[derive(Clone, Copy, Debug)]
pub enum TimerAction {
    Start,
    Pause,
    Resume,
    Reset,
    RestoreDefaults,
    AdjustRemaining(i64),
    SetRemaining(Duration),
}

#[derive(Clone, Debug)]
pub struct TimerSnapshot {
    pub phase: TimerPhase,
    pub remaining: Duration,
    pub remaining_fraction: f32,
    pub effect_elapsed: Option<Duration>,
    /// Running time only: unaffected by pauses or remaining-time edits.
    pub animation_elapsed: Duration,
}

#[derive(Clone, Copy, Debug)]
pub struct CompletionEvent {
    pub timer_id: TimerId,
    pub run: u64,
    pub style: TimerStyle,
    pub volume: f32,
}

#[derive(Clone, Copy, Debug)]
enum Runtime {
    Idle,
    Running {
        started: Instant,
        remaining: Duration,
    },
    Paused {
        remaining: Duration,
    },
    // Together these describe the actual deadline without overflowing Instant addition.
    Finished {
        started: Instant,
        remaining: Duration,
    },
}

#[derive(Debug)]
pub struct Timer {
    pub id: TimerId,
    pub position: Option<WidgetPosition>,
    pub locked: bool,
    settings: TimerSettings,
    runtime: Runtime,
    run: u64,
    elapsed_before_segment: Duration,
}

impl Timer {
    pub fn new(id: TimerId, settings: TimerSettings) -> Result<Self, String> {
        settings.validate()?;
        Ok(Self {
            id,
            position: None,
            locked: false,
            settings,
            runtime: Runtime::Idle,
            run: 0,
            elapsed_before_segment: Duration::ZERO,
        })
    }

    pub fn settings(&self) -> &TimerSettings {
        &self.settings
    }

    pub fn advance(&mut self, now: Instant) -> Option<CompletionEvent> {
        if let Runtime::Running { started, remaining } = self.runtime
            && now.saturating_duration_since(started) >= remaining
        {
            self.runtime = Runtime::Finished { started, remaining };
            return Some(CompletionEvent {
                timer_id: self.id,
                run: self.run,
                style: self.settings.style,
                volume: if self.settings.muted {
                    0.0
                } else {
                    self.settings.volume
                },
            });
        }
        None
    }

    pub fn apply(&mut self, action: TimerAction, now: Instant) -> Option<CompletionEvent> {
        // Cancellation wins over an unprocessed completion of the old run.
        match action {
            TimerAction::Reset => {
                self.runtime = Runtime::Idle;
                self.elapsed_before_segment = Duration::ZERO;
                return None;
            }
            TimerAction::RestoreDefaults => {
                self.settings = TimerSettings::default();
                self.runtime = Runtime::Idle;
                self.elapsed_before_segment = Duration::ZERO;
                return None;
            }
            _ => {}
        }

        let completion = self.advance(now);
        if let TimerAction::AdjustRemaining(_) | TimerAction::SetRemaining(_) = action {
            let current = match self.runtime {
                Runtime::Running { started, remaining } => {
                    remaining.saturating_sub(now.saturating_duration_since(started))
                }
                Runtime::Paused { remaining } => remaining,
                Runtime::Idle | Runtime::Finished { .. } => return completion,
            };
            let remaining = match action {
                TimerAction::AdjustRemaining(seconds) if seconds < 0 => {
                    current.saturating_sub(Duration::from_secs(seconds.unsigned_abs()))
                }
                TimerAction::AdjustRemaining(seconds) => {
                    current.saturating_add(Duration::from_secs(seconds as u64))
                }
                TimerAction::SetRemaining(remaining) => remaining,
                _ => unreachable!(),
            }
            .min(Duration::from_secs(86_400));
            if let Runtime::Running { started, remaining } = self.runtime {
                self.elapsed_before_segment +=
                    now.saturating_duration_since(started).min(remaining);
            }
            self.runtime = if remaining.is_zero() || matches!(self.runtime, Runtime::Running { .. })
            {
                Runtime::Running {
                    started: now,
                    remaining,
                }
            } else {
                Runtime::Paused { remaining }
            };
            return self.advance(now).or(completion);
        }
        match (action, self.runtime) {
            (TimerAction::Start, Runtime::Idle) => {
                if let Some(run) = self.run.checked_add(1) {
                    self.run = run;
                    self.runtime = Runtime::Running {
                        started: now,
                        remaining: self.settings.duration,
                    };
                }
            }
            (TimerAction::Pause, Runtime::Running { started, remaining }) => {
                self.elapsed_before_segment +=
                    now.saturating_duration_since(started).min(remaining);
                self.runtime = Runtime::Paused {
                    remaining: remaining.saturating_sub(now.saturating_duration_since(started)),
                };
            }
            (TimerAction::Resume, Runtime::Paused { remaining }) => {
                self.runtime = Runtime::Running {
                    started: now,
                    remaining,
                };
            }
            _ => {}
        }
        completion
    }

    pub fn update_settings(
        &mut self,
        settings: TimerSettings,
        now: Instant,
    ) -> Result<Option<CompletionEvent>, String> {
        settings.validate()?;
        if settings.duration != self.settings.duration {
            self.settings = settings;
            self.runtime = Runtime::Idle;
            self.elapsed_before_segment = Duration::ZERO;
            return Ok(None);
        }

        // A deadline reached before this edit belongs to the previous settings.
        let completion = self.advance(now);
        self.settings = settings;
        Ok(completion)
    }

    pub fn snapshot(&self, now: Instant) -> TimerSnapshot {
        let (phase, remaining, effect_elapsed) = match self.runtime {
            Runtime::Idle => (TimerPhase::Idle, self.settings.duration, None),
            Runtime::Paused { remaining } => (TimerPhase::Paused, remaining, None),
            Runtime::Running { started, remaining } => {
                let elapsed = now.saturating_duration_since(started);
                if elapsed >= remaining {
                    (
                        TimerPhase::Finished,
                        Duration::ZERO,
                        Some(elapsed.saturating_sub(remaining)),
                    )
                } else {
                    (TimerPhase::Running, remaining.saturating_sub(elapsed), None)
                }
            }
            Runtime::Finished { started, remaining } => (
                TimerPhase::Finished,
                Duration::ZERO,
                Some(
                    now.saturating_duration_since(started)
                        .saturating_sub(remaining),
                ),
            ),
        };
        TimerSnapshot {
            phase,
            remaining,
            remaining_fraction: (remaining.as_secs_f64() / self.settings.duration.as_secs_f64())
                .clamp(0.0, 1.0) as f32,
            effect_elapsed,
            animation_elapsed: self.elapsed_before_segment
                + match self.runtime {
                    Runtime::Running { started, remaining } => {
                        now.saturating_duration_since(started).min(remaining)
                    }
                    Runtime::Finished { remaining, .. } => remaining,
                    Runtime::Idle | Runtime::Paused { .. } => Duration::ZERO,
                },
        }
    }
}

pub fn next_timer_id(timers: &[Timer]) -> Option<TimerId> {
    match timers.iter().map(|timer| timer.id).max() {
        Some(maximum) => maximum.checked_add(1),
        None => Some(1),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn timer(seconds: u64) -> Timer {
        Timer::new(
            7,
            TimerSettings {
                duration: Duration::from_secs(seconds),
                ..TimerSettings::default()
            },
        )
        .unwrap()
    }

    #[test]
    fn animation_clock_preserves_pose_across_pause_and_time_edits() {
        let now = Instant::now();
        let at = |seconds| now + Duration::from_secs(seconds);
        let mut timer = timer(20);
        timer.apply(TimerAction::Start, now);
        timer.apply(TimerAction::Pause, at(3));
        assert_eq!(
            timer.snapshot(at(100)).animation_elapsed,
            Duration::from_secs(3)
        );
        timer.apply(TimerAction::AdjustRemaining(30), at(100));
        assert_eq!(
            timer.snapshot(at(110)).animation_elapsed,
            Duration::from_secs(3)
        );
        timer.apply(TimerAction::Resume, at(110));
        assert_eq!(
            timer.snapshot(at(112)).animation_elapsed,
            Duration::from_secs(5)
        );
        timer.apply(TimerAction::SetRemaining(Duration::from_secs(4)), at(112));
        assert_eq!(
            timer.snapshot(at(112)).animation_elapsed,
            Duration::from_secs(5)
        );
        let mut settings = timer.settings().clone();
        settings.style = TimerStyle::CodeRain;
        settings.reduced_motion = true;
        timer.update_settings(settings, at(113)).unwrap();
        assert_eq!(
            timer.snapshot(at(113)).animation_elapsed,
            Duration::from_secs(6)
        );
        assert_eq!(
            timer.snapshot(at(120)).animation_elapsed,
            Duration::from_secs(9)
        );
        assert_eq!(
            timer.snapshot(at(120)).effect_elapsed,
            Some(Duration::from_secs(4))
        );
        timer.advance(at(120)).unwrap();
        assert_eq!(
            timer.snapshot(at(130)).animation_elapsed,
            Duration::from_secs(9)
        );
        timer.apply(TimerAction::Reset, at(130));
        assert_eq!(timer.snapshot(at(130)).animation_elapsed, Duration::ZERO);
        timer.apply(TimerAction::Start, at(130));
        let mut settings = timer.settings().clone();
        settings.duration = Duration::from_secs(10);
        timer.update_settings(settings, at(132)).unwrap();
        assert_eq!(timer.snapshot(at(132)).animation_elapsed, Duration::ZERO);
    }

    #[test]
    fn pause_resume_excludes_multiple_paused_intervals() {
        let now = Instant::now();
        let mut timer = timer(10);
        timer.apply(TimerAction::Start, now);
        timer.apply(TimerAction::Pause, now + Duration::from_secs(3));
        assert_eq!(
            timer.snapshot(now + Duration::from_secs(100)).remaining,
            Duration::from_secs(7)
        );
        timer.apply(TimerAction::Resume, now + Duration::from_secs(100));
        timer.apply(TimerAction::Pause, now + Duration::from_secs(102));
        timer.apply(TimerAction::Resume, now + Duration::from_secs(200));
        assert!(timer.advance(now + Duration::from_secs(204)).is_none());
        assert_eq!(
            timer.snapshot(now + Duration::from_secs(204)).remaining,
            Duration::from_secs(1)
        );
        assert!(timer.advance(now + Duration::from_secs(205)).is_some());
    }

    #[test]
    fn pure_snapshots_and_large_jumps_do_not_consume_or_repeat_completion() {
        let now = Instant::now();
        let mut timer = timer(2);
        timer.apply(TimerAction::Start, now);
        let late = now + Duration::from_secs(500);
        for _ in 0..3 {
            let snapshot = timer.snapshot(late);
            assert_eq!(snapshot.phase, TimerPhase::Finished);
            assert_eq!(snapshot.effect_elapsed, Some(Duration::from_secs(498)));
        }
        assert_eq!(timer.advance(late).unwrap().run, 1);
        assert!(timer.advance(late).is_none());
        assert!(timer.apply(TimerAction::Start, late).is_none());
        assert!(timer.apply(TimerAction::Resume, late).is_none());
        assert_eq!(timer.snapshot(late).phase, TimerPhase::Finished);
    }

    #[test]
    fn pause_at_or_after_deadline_finishes_once() {
        for delay in [5, 9] {
            let now = Instant::now();
            let mut timer = timer(5);
            timer.apply(TimerAction::Start, now);
            let deadline = now + Duration::from_secs(delay);
            assert!(timer.apply(TimerAction::Pause, deadline).is_some());
            assert_eq!(timer.snapshot(deadline).phase, TimerPhase::Finished);
            assert!(timer.apply(TimerAction::Pause, deadline).is_none());
        }
    }

    #[test]
    fn simultaneous_timers_each_complete_independently() {
        let now = Instant::now();
        let mut first = timer(1);
        let mut second = Timer::new(8, first.settings().clone()).unwrap();
        first.apply(TimerAction::Start, now);
        second.apply(TimerAction::Start, now);
        let deadline = now + Duration::from_secs(1);
        assert_eq!(first.advance(deadline).unwrap().timer_id, 7);
        assert_eq!(second.advance(deadline).unwrap().timer_id, 8);
        assert!(first.advance(deadline).is_none());
        assert!(second.advance(deadline).is_none());
    }

    #[test]
    fn reset_restart_and_duration_edit_cancel_old_deadlines() {
        let now = Instant::now();
        let mut timer = timer(5);
        timer.apply(TimerAction::Start, now);
        assert!(
            timer
                .apply(TimerAction::Reset, now + Duration::from_secs(10))
                .is_none()
        );
        assert_eq!(timer.snapshot(now).effect_elapsed, None);
        timer.apply(TimerAction::Start, now + Duration::from_secs(10));
        assert!(timer.advance(now + Duration::from_secs(14)).is_none());
        assert_eq!(timer.advance(now + Duration::from_secs(15)).unwrap().run, 2);

        timer.apply(TimerAction::Reset, now + Duration::from_secs(16));
        timer.apply(TimerAction::Start, now + Duration::from_secs(16));
        let mut settings = timer.settings().clone();
        settings.duration = Duration::from_secs(20);
        assert!(
            timer
                .update_settings(settings, now + Duration::from_secs(30))
                .unwrap()
                .is_none()
        );
        assert_eq!(
            timer.snapshot(now + Duration::from_secs(30)).phase,
            TimerPhase::Idle
        );
        timer.apply(TimerAction::Start, now + Duration::from_secs(30));
        assert!(timer.advance(now + Duration::from_secs(35)).is_none());
        assert_eq!(timer.advance(now + Duration::from_secs(50)).unwrap().run, 4);
    }

    #[test]
    fn nonduration_edits_preserve_time_and_use_new_completion_sound() {
        let now = Instant::now();
        let mut timer = timer(10);
        timer.apply(TimerAction::Start, now);
        let mut settings = timer.settings().clone();
        settings.label = "Tea".to_owned();
        settings.style = TimerStyle::Hourglass;
        settings.size = 460.0;
        settings.volume = 0.25;
        timer
            .update_settings(settings, now + Duration::from_secs(4))
            .unwrap();
        assert_eq!(
            timer.snapshot(now + Duration::from_secs(4)).remaining,
            Duration::from_secs(6)
        );
        let event = timer.advance(now + Duration::from_secs(10)).unwrap();
        assert_eq!(event.style, TimerStyle::Hourglass);
        assert_eq!(event.volume, 0.25);
    }

    #[test]
    fn muting_live_completion_preserves_animation_time_and_unmuted_volume() {
        let now = Instant::now();
        let mut timer = timer(10);
        timer.apply(TimerAction::Start, now);
        let mut settings = timer.settings().clone();
        settings.muted = true;
        settings.style = TimerStyle::Rocket;
        timer
            .update_settings(settings, now + Duration::from_secs(3))
            .unwrap();
        assert_eq!(
            timer.snapshot(now + Duration::from_secs(3)).remaining,
            Duration::from_secs(7)
        );
        assert_eq!(timer.settings().volume, 0.6);
        let completion = timer.advance(now + Duration::from_secs(10)).unwrap();
        assert_eq!(completion.volume, 0.0);
        assert_eq!(completion.style, TimerStyle::Rocket);
        assert_eq!(
            timer.snapshot(now + Duration::from_secs(10)).effect_elapsed,
            Some(Duration::ZERO)
        );
        timer.apply(TimerAction::Reset, now);
        timer.apply(TimerAction::Start, now);
        let mut settings = timer.settings().clone();
        settings.muted = false;
        timer
            .update_settings(settings, now + Duration::from_secs(3))
            .unwrap();
        assert_eq!(
            timer.advance(now + Duration::from_secs(10)).unwrap().volume,
            0.6
        );
    }

    #[test]
    fn defaults_preserve_identity_position_and_cancel_effect() {
        let now = Instant::now();
        let mut timer = timer(1);
        timer.position = Some(WidgetPosition { x: -20.0, y: 400.0 });
        timer.apply(TimerAction::Start, now);
        timer.advance(now + Duration::from_secs(1));
        timer.apply(TimerAction::RestoreDefaults, now + Duration::from_secs(2));
        assert_eq!(timer.id, 7);
        assert_eq!(timer.position.unwrap().x, -20.0);
        assert_eq!(timer.position.unwrap().y, 400.0);
        assert_eq!(timer.settings(), &TimerSettings::default());
        let snapshot = timer.snapshot(now + Duration::from_secs(100));
        assert_eq!(snapshot.phase, TimerPhase::Idle);
        assert_eq!(snapshot.remaining_fraction, 1.0);
        assert_eq!(snapshot.effect_elapsed, None);
    }

    #[test]
    fn validation_rejects_invalid_data_without_mutating_running_timer() {
        let now = Instant::now();
        let mut timer = timer(10);
        timer.apply(TimerAction::Start, now);
        let invalid = [
            TimerSettings {
                duration: Duration::ZERO,
                ..TimerSettings::default()
            },
            TimerSettings {
                duration: Duration::from_millis(1500),
                ..TimerSettings::default()
            },
            TimerSettings {
                duration: Duration::from_secs(86401),
                ..TimerSettings::default()
            },
            TimerSettings {
                size: f32::NAN,
                ..TimerSettings::default()
            },
            TimerSettings {
                volume: f32::INFINITY,
                ..TimerSettings::default()
            },
            TimerSettings {
                label: "é".repeat(81),
                ..TimerSettings::default()
            },
        ];
        for settings in invalid {
            assert!(
                timer
                    .update_settings(settings, now + Duration::from_secs(20))
                    .is_err()
            );
        }
        assert_eq!(timer.settings().duration, Duration::from_secs(10));
        assert!(timer.advance(now + Duration::from_secs(20)).is_some());
        assert!(
            TimerSettings {
                duration: Duration::from_secs(86400),
                label: "é".repeat(80),
                ..TimerSettings::default()
            }
            .validate()
            .is_ok()
        );
    }

    #[test]
    fn live_adjustments_preserve_running_state_and_reset_duration() {
        let now = Instant::now();
        let mut timer = timer(120);
        timer.apply(TimerAction::Start, now);
        let edited = now + Duration::from_millis(10_500);
        timer.apply(TimerAction::AdjustRemaining(60), edited);
        assert_eq!(timer.snapshot(edited).phase, TimerPhase::Running);
        assert_eq!(
            timer.snapshot(edited).remaining,
            Duration::from_millis(169_500)
        );
        timer.apply(
            TimerAction::AdjustRemaining(-60),
            edited + Duration::from_secs(5),
        );
        assert_eq!(
            timer.snapshot(edited + Duration::from_secs(5)).remaining,
            Duration::from_millis(104_500)
        );
        timer.apply(
            TimerAction::SetRemaining(Duration::from_secs(3)),
            edited + Duration::from_secs(6),
        );
        assert!(timer.advance(edited + Duration::from_secs(8)).is_none());
        assert_eq!(
            timer.advance(edited + Duration::from_secs(9)).unwrap().run,
            1
        );
        assert!(timer.advance(edited + Duration::from_secs(10)).is_none());
        timer.apply(TimerAction::Reset, edited + Duration::from_secs(10));
        assert_eq!(
            timer.snapshot(edited + Duration::from_secs(10)).remaining,
            Duration::from_secs(120)
        );
    }

    #[test]
    fn live_adjustments_keep_paused_time_frozen_and_finish_once_at_zero() {
        let now = Instant::now();
        let mut timer = timer(120);
        timer.apply(TimerAction::Start, now);
        timer.apply(TimerAction::Pause, now + Duration::from_secs(10));
        let edited = now + Duration::from_secs(100);
        timer.apply(TimerAction::AdjustRemaining(60), edited);
        timer.apply(TimerAction::SetRemaining(Duration::from_secs(30)), edited);
        assert_eq!(
            timer.snapshot(edited + Duration::from_secs(100)).phase,
            TimerPhase::Paused
        );
        assert_eq!(
            timer.snapshot(edited + Duration::from_secs(100)).remaining,
            Duration::from_secs(30)
        );
        assert_eq!(
            timer
                .apply(TimerAction::AdjustRemaining(-60), edited)
                .unwrap()
                .run,
            1
        );
        assert_eq!(timer.snapshot(edited).phase, TimerPhase::Finished);
        assert_eq!(timer.snapshot(edited).effect_elapsed, Some(Duration::ZERO));
        assert!(
            timer
                .apply(TimerAction::SetRemaining(Duration::from_secs(30)), edited)
                .is_none()
        );
        assert!(timer.advance(edited + Duration::from_secs(200)).is_none());
    }

    #[test]
    fn live_adjustments_cannot_revive_expired_runs_and_saturate_at_bounds() {
        let now = Instant::now();
        let mut timer = timer(10);
        timer.apply(TimerAction::AdjustRemaining(60), now);
        assert_eq!(timer.snapshot(now).phase, TimerPhase::Idle);
        assert_eq!(timer.snapshot(now).remaining, Duration::from_secs(10));
        timer.apply(TimerAction::Start, now);
        assert_eq!(
            timer
                .apply(
                    TimerAction::SetRemaining(Duration::from_secs(60)),
                    now + Duration::from_secs(10)
                )
                .unwrap()
                .run,
            1
        );
        assert_eq!(
            timer.snapshot(now + Duration::from_secs(10)).phase,
            TimerPhase::Finished
        );
        timer.apply(TimerAction::Reset, now);
        timer.apply(TimerAction::Start, now);
        timer.apply(TimerAction::AdjustRemaining(i64::MAX), now);
        assert_eq!(timer.snapshot(now).remaining, Duration::from_secs(86_400));
        assert_eq!(
            timer
                .apply(TimerAction::AdjustRemaining(i64::MIN), now)
                .unwrap()
                .run,
            2
        );
        assert!(timer.advance(now).is_none());
    }

    #[test]
    fn backwards_observation_saturates_and_id_exhaustion_does_not_wrap() {
        let now = Instant::now();
        let mut timer = timer(10);
        timer.apply(TimerAction::Start, now + Duration::from_secs(1));
        assert_eq!(timer.snapshot(now).remaining, Duration::from_secs(10));
        assert!(timer.advance(now).is_none());
        assert_eq!(next_timer_id(&[]), Some(1));
        timer.id = u64::MAX;
        assert_eq!(next_timer_id(&[timer]), None);
    }
}
