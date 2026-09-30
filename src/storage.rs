use std::collections::HashSet;
use std::fmt;

use serde::de::{self, SeqAccess, Visitor};
use serde::{Deserialize, Deserializer, Serialize};

use crate::timer::{Timer, TimerId, TimerSettings, WidgetPosition};

pub const MAX_TIMERS: usize = 64;
pub const MAX_SAVED_BYTES: usize = 256 * 1024;
pub const STORAGE_KEY: &str = "useful-timer.state";
const FORMAT_VERSION: u32 = 1;

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SavedTimer {
    pub id: TimerId,
    pub settings: TimerSettings,
    pub position: Option<WidgetPosition>,
    #[serde(default)]
    pub locked: bool,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SavedState {
    version: u32,
    #[serde(deserialize_with = "deserialize_timers")]
    timers: Vec<SavedTimer>,
}

impl Default for SavedState {
    fn default() -> Self {
        Self {
            version: FORMAT_VERSION,
            timers: Vec::new(),
        }
    }
}

impl SavedState {
    pub fn load(storage: Option<&dyn eframe::Storage>) -> (Self, Option<String>) {
        let Some(storage) = storage else {
            return (
                Self::default(),
                Some(
                    "Local persistence is unavailable; timer settings will not survive restart."
                        .to_owned(),
                ),
            );
        };
        let Some(serialized) = storage.get_string(STORAGE_KEY) else {
            return (Self::default(), None);
        };

        let decoded = Self::decode(&serialized);
        match decoded {
            Ok(state) => (state, None),
            Err(error) => (
                Self::default(),
                Some(format!("Saved timers could not be restored: {error}")),
            ),
        }
    }

    pub fn from_timers(timers: &[Timer]) -> Self {
        Self {
            version: FORMAT_VERSION,
            timers: timers
                .iter()
                .map(|timer| SavedTimer {
                    id: timer.id,
                    settings: timer.settings().clone(),
                    position: timer.position,
                    locked: timer.locked,
                })
                .collect(),
        }
    }

    pub fn save(&self, storage: &mut dyn eframe::Storage) -> Result<(), String> {
        self.validate()?;
        let serialized = ron::to_string(self)
            .map_err(|error| format!("Timer settings could not be encoded: {error}"))?;
        if serialized.len() > MAX_SAVED_BYTES {
            return Err(format!("Saved timer data exceeds {MAX_SAVED_BYTES} bytes."));
        }
        // eframe owns flushing through its save lifecycle. Do not create another file.
        storage.set_string(STORAGE_KEY, serialized);
        Ok(())
    }

    pub fn into_timers(self) -> Result<Vec<Timer>, String> {
        self.validate()?;
        self.timers
            .into_iter()
            .map(|saved| {
                let mut timer = Timer::new(saved.id, saved.settings)?;
                timer.position = saved.position;
                timer.locked = saved.locked;
                Ok(timer)
            })
            .collect()
    }

    fn decode(serialized: &str) -> Result<Self, String> {
        if serialized.len() > MAX_SAVED_BYTES {
            return Err(format!("Data exceeds the {MAX_SAVED_BYTES}-byte limit."));
        }
        let state: Self =
            ron::from_str(serialized).map_err(|error| format!("Invalid timer data: {error}"))?;
        state.validate()?;
        Ok(state)
    }

    fn validate(&self) -> Result<(), String> {
        if self.version != FORMAT_VERSION {
            return Err(format!(
                "Unsupported timer data version {} (expected {FORMAT_VERSION}).",
                self.version
            ));
        }
        if self.timers.len() > MAX_TIMERS {
            return Err(format!("At most {MAX_TIMERS} timers can be saved."));
        }
        let mut identities = HashSet::with_capacity(self.timers.len());
        for timer in &self.timers {
            if !identities.insert(timer.id) {
                return Err(format!("Duplicate timer identity {}.", timer.id));
            }
            timer
                .settings
                .validate()
                .map_err(|error| format!("Timer {}: {error}", timer.id))?;
            if let Some(position) = timer.position
                && (!position.x.is_finite() || !position.y.is_finite())
            {
                return Err(format!(
                    "Timer {} has a non-finite widget position.",
                    timer.id
                ));
            }
        }
        Ok(())
    }
}

// Enforce the count during decoding, not after an unbounded Vec allocation.
fn deserialize_timers<'de, D>(deserializer: D) -> Result<Vec<SavedTimer>, D::Error>
where
    D: Deserializer<'de>,
{
    struct TimersVisitor;

    impl<'de> Visitor<'de> for TimersVisitor {
        type Value = Vec<SavedTimer>;

        fn expecting(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
            write!(formatter, "a list of at most {MAX_TIMERS} timers")
        }

        fn visit_seq<A>(self, mut sequence: A) -> Result<Self::Value, A::Error>
        where
            A: SeqAccess<'de>,
        {
            let mut timers = Vec::with_capacity(sequence.size_hint().unwrap_or(0).min(MAX_TIMERS));
            while let Some(timer) = sequence.next_element::<SavedTimer>()? {
                if timers.len() == MAX_TIMERS {
                    return Err(de::Error::custom(format!(
                        "At most {MAX_TIMERS} timers can be restored."
                    )));
                }
                timers.push(timer);
            }
            Ok(timers)
        }
    }

    deserializer.deserialize_seq(TimersVisitor)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::timer::{TimerAction, TimerPhase, TimerStyle};
    use eframe::Storage;
    use std::collections::HashMap;
    use std::time::{Duration, Instant};

    #[derive(Default)]
    struct MemoryStorage(HashMap<String, String>);

    impl eframe::Storage for MemoryStorage {
        fn get_string(&self, key: &str) -> Option<String> {
            self.0.get(key).cloned()
        }

        fn set_string(&mut self, key: &str, value: String) {
            self.0.insert(key.to_owned(), value);
        }

        fn remove_string(&mut self, key: &str) {
            self.0.remove(key);
        }

        fn flush(&mut self) {}
    }

    fn configured_timer(id: TimerId) -> Timer {
        let mut timer = Timer::new(
            id,
            TimerSettings {
                label: "Tea — 休憩".to_owned(),
                duration: Duration::from_secs(12),
                style: TimerStyle::Rocket,
                size: 240.0,
                volume: 0.3,
                muted: true,
            },
        )
        .unwrap();
        timer.position = Some(WidgetPosition {
            x: -125.0,
            y: 300.5,
        });
        timer
    }

    #[test]
    fn roundtrip_restores_config_and_position_but_never_runtime() {
        let now = Instant::now();
        let mut running = configured_timer(5);
        let mut finished = configured_timer(6);
        running.locked = true;
        running.apply(TimerAction::Start, now);
        finished.apply(TimerAction::Start, now);
        finished.advance(now + Duration::from_secs(12));
        let mut storage = MemoryStorage::default();
        SavedState::from_timers(&[running, finished])
            .save(&mut storage)
            .unwrap();
        let (saved, warning) = SavedState::load(Some(&storage));
        assert_eq!(warning, None);
        let restored = saved.into_timers().unwrap();
        for (offset, mut timer) in restored.into_iter().enumerate() {
            assert_eq!(timer.id, 5 + offset as u64);
            assert_eq!(timer.settings(), configured_timer(timer.id).settings());
            assert_eq!(timer.locked, offset == 0);
            assert_eq!(timer.position.unwrap().x, -125.0);
            assert_eq!(timer.position.unwrap().y, 300.5);
            let snapshot = timer.snapshot(now + Duration::from_secs(500));
            assert_eq!(snapshot.phase, TimerPhase::Idle);
            assert_eq!(snapshot.remaining, Duration::from_secs(12));
            assert_eq!(snapshot.effect_elapsed, None);
            assert!(timer.advance(now + Duration::from_secs(500)).is_none());
            timer.apply(TimerAction::Start, now + Duration::from_secs(500));
            assert_eq!(
                timer.advance(now + Duration::from_secs(512)).unwrap().run,
                1
            );
        }
    }

    #[test]
    fn saved_settings_without_mute_field_keep_their_sound_and_countdown() {
        let legacy = r#"(version:1,timers:[(id:1,settings:(label:"Tea",duration:(secs:90,nanos:0),style:Hourglass,size:300.0,volume:0.4),position:None)])"#;
        let saved = SavedState::decode(legacy).unwrap();
        let mut timer = saved.into_timers().unwrap().remove(0);
        assert!(!timer.settings().muted);
        assert!(!timer.locked);
        assert_eq!(timer.settings().duration, Duration::from_secs(90));
        let now = Instant::now();
        timer.apply(TimerAction::Start, now);
        assert_eq!(
            timer.advance(now + Duration::from_secs(90)).unwrap().volume,
            0.4
        );
    }

    fn assert_rejected(serialized: String) {
        let mut storage = MemoryStorage::default();
        storage.set_string(STORAGE_KEY, serialized.clone());
        let (saved, warning) = SavedState::load(Some(&storage));
        assert!(warning.is_some());
        assert!(saved.into_timers().unwrap().is_empty());
        assert_eq!(storage.get_string(STORAGE_KEY).unwrap(), serialized);
    }

    #[test]
    fn malformed_unsupported_and_duplicate_data_are_explicitly_rejected() {
        assert_rejected("not valid RON".to_owned());
        let mut unsupported = SavedState::from_timers(&[configured_timer(1)]);
        unsupported.version = 999;
        assert_rejected(ron::to_string(&unsupported).unwrap());
        let duplicate = SavedState::from_timers(&[configured_timer(1), configured_timer(1)]);
        assert_rejected(ron::to_string(&duplicate).unwrap());
    }

    #[test]
    fn invalid_settings_and_nonfinite_positions_are_not_normalized() {
        let mut invalid_duration = SavedState::from_timers(&[configured_timer(1)]);
        invalid_duration.timers[0].settings.duration = Duration::ZERO;
        assert_rejected(ron::to_string(&invalid_duration).unwrap());
        let mut invalid_volume = SavedState::from_timers(&[configured_timer(1)]);
        invalid_volume.timers[0].settings.volume = f32::NAN;
        assert_rejected(ron::to_string(&invalid_volume).unwrap());
        let mut invalid_position = SavedState::from_timers(&[configured_timer(1)]);
        invalid_position.timers[0].position = Some(WidgetPosition {
            x: f32::INFINITY,
            y: 0.0,
        });
        assert_rejected(ron::to_string(&invalid_position).unwrap());
    }

    #[test]
    fn oversized_input_and_timer_lists_are_bounded() {
        assert_rejected(" ".repeat(MAX_SAVED_BYTES + 1));
        let timers: Vec<_> = (0..=MAX_TIMERS)
            .map(|id| configured_timer(id as u64))
            .collect();
        let oversized = SavedState::from_timers(&timers);
        assert_rejected(ron::to_string(&oversized).unwrap());
        assert!(oversized.into_timers().is_err());
    }

    #[test]
    fn rejected_save_preserves_previous_valid_data() {
        let mut storage = MemoryStorage::default();
        SavedState::from_timers(&[configured_timer(1)])
            .save(&mut storage)
            .unwrap();
        let previous = storage.get_string(STORAGE_KEY).unwrap();
        let duplicate = SavedState::from_timers(&[configured_timer(2), configured_timer(2)]);
        assert!(duplicate.save(&mut storage).is_err());
        assert_eq!(storage.get_string(STORAGE_KEY).unwrap(), previous);
        assert!(duplicate.into_timers().is_err());
    }

    #[test]
    fn fresh_storage_is_empty_without_corruption_warning() {
        let storage = MemoryStorage::default();
        let (saved, warning) = SavedState::load(Some(&storage));
        assert_eq!(warning, None);
        assert!(saved.into_timers().unwrap().is_empty());
        let (_, unavailable) = SavedState::load(None);
        assert!(unavailable.is_some());
    }
}
