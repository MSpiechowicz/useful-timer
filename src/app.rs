use std::{
    collections::VecDeque,
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, Ordering},
    },
    time::{Duration, Instant},
};

use eframe::egui::{self, Color32, RichText, Vec2, ViewportCommand, ViewportId};
use useful_timer::{
    audio::AudioEngine,
    storage::{MAX_TIMERS, SavedState},
    timer::{
        CompletionEvent, Timer, TimerAction, TimerId, TimerPhase, TimerSettings, TimerSnapshot,
        TimerStyle, WidgetPosition, next_timer_id,
    },
    update::{RELEASES_URL, UpdateState, Updater},
};

use crate::{
    branding,
    theme::{self, Palette, Theme},
    visuals,
};

const FRAME_INTERVAL: Duration = Duration::from_millis(33);
const LOGIC_INTERVAL: Duration = Duration::from_millis(100);
const WIDGET_TITLE_HEIGHT: f32 = 50.0;
const DEFAULT_DURATION_KEY: &str = "useful-timer.default-duration-seconds";

struct UpdateToast {
    message: String,
    success: bool,
    expires: Instant,
}

enum Command {
    Action(TimerId, TimerAction),
    Settings(TimerId, TimerSettings),
    Locked(TimerId, bool),
    Remove(TimerId),
}

struct SharedState {
    timers: Vec<Timer>,
    commands: VecDeque<Command>,
}

struct TimerView {
    id: TimerId,
    settings: TimerSettings,
    snapshot: TimerSnapshot,
    position: Option<WidgetPosition>,
    locked: bool,
}

impl TimerView {
    fn from_timer(timer: &Timer, now: Instant) -> Self {
        Self {
            id: timer.id,
            settings: timer.settings().clone(),
            snapshot: timer.snapshot(now),
            position: timer.position,
            locked: timer.locked,
        }
    }
}

struct Monitor {
    // All monitor bounds share physical desktop coordinates, even at different DPIs.
    bounds: egui::Rect,
    scale_factor: f32,
}

struct InitialPosition {
    physical: Option<egui::Pos2>,
    applied_frame: Option<u64>,
}

struct WidgetWindow {
    timer_id: TimerId,
    viewport_id: ViewportId,
    settings: TimerSettings,
    builder: egui::ViewportBuilder,
    initial_position: Arc<Mutex<InitialPosition>>,
}

impl WidgetWindow {
    fn new(
        view: &TimerView,
        monitors: &[Monitor],
        root_position: Option<egui::Pos2>,
        zoom_factor: f32,
    ) -> Self {
        let size = widget_size(view.settings.size);
        let initial_position = match view.position {
            Some(position) => Some(clamp_position(
                egui::pos2(position.x, position.y),
                size,
                monitors,
                zoom_factor,
            )),
            None => bottom_left_position(size, monitors, root_position, zoom_factor),
        };
        let builder = egui::ViewportBuilder::default()
            .with_title(widget_title(view.id, &view.settings))
            .with_app_id("useful-timer")
            .with_icon(branding::icon())
            .with_inner_size(size)
            .with_decorations(false)
            .with_resizable(false)
            .with_transparent(true)
            .with_has_shadow(false)
            .with_always_on_top()
            .with_active(false)
            .with_taskbar(false);
        Self {
            timer_id: view.id,
            viewport_id: ViewportId::from_hash_of(("useful-timer-widget", view.id)),
            settings: view.settings.clone(),
            builder,
            initial_position: Arc::new(Mutex::new(InitialPosition {
                physical: initial_position,
                applied_frame: None,
            })),
        }
    }

    fn update(&mut self, settings: &TimerSettings) {
        if self.settings.label != settings.label || self.settings.style != settings.style {
            self.builder.title = Some(widget_title(self.timer_id, settings));
        }
        let size = widget_size(settings.size);
        if self.builder.inner_size != Some(size) {
            self.builder.inner_size = Some(size);
        }
        if self.settings != *settings {
            self.settings = settings.clone();
        }
    }
}

struct TimeInput {
    hours: u64,
    minutes: u64,
    seconds: u64,
}

impl TimeInput {
    fn from_duration(duration: Duration) -> Self {
        let seconds = duration.as_secs() + u64::from(duration.subsec_nanos() > 0);
        Self {
            hours: seconds / 3600,
            minutes: seconds / 60 % 60,
            seconds: seconds % 60,
        }
    }

    fn duration(&self) -> Duration {
        Duration::from_secs(self.hours * 3600 + self.minutes * 60 + self.seconds)
    }

    fn component(ui: &mut egui::Ui, value: &mut u64, maximum: u64) -> egui::Response {
        let id = ui.next_auto_id();
        let buffer_id = id.with("fixed-time-input");
        let buffer = ui.data_mut(|data| data.remove_temp::<String>(buffer_id));
        if !ui.memory(|memory| memory.has_focus(id)) && buffer.is_none() {
            return ui.add(
                egui::DragValue::new(value)
                    .range(0..=maximum)
                    .speed(1.0)
                    .update_while_editing(false),
            );
        }

        let newly_editing = buffer.is_none();
        let mut text = buffer.unwrap_or_else(|| value.to_string());
        let step = ui.input_mut(|input| {
            input.count_and_consume_key(egui::Modifiers::NONE, egui::Key::ArrowUp) as i64
                - input.count_and_consume_key(egui::Modifiers::NONE, egui::Key::ArrowDown) as i64
        });
        if step != 0 {
            *value = value.saturating_add_signed(step).min(maximum);
            text = value.to_string();
        }
        let padding = ui.spacing().button_padding;
        let mut output = egui::TextEdit::singleline(&mut text)
            .id(id)
            .desired_width(64.0 - 2.0 * padding.x)
            .min_size(egui::vec2(0.0, 36.0 - 2.0 * padding.y))
            .margin(padding)
            .font(ui.style().drag_value_text_style.clone())
            .horizontal_align(egui::Align::Center)
            .vertical_align(egui::Align::Center)
            .clip_text(true)
            .show(ui);
        if newly_editing {
            output
                .state
                .cursor
                .set_char_range(Some(egui::text::CCursorRange::two(
                    egui::text::CCursor::default(),
                    egui::text::CCursor::new(text.chars().count()),
                )));
            output.state.store(ui.ctx(), id);
        }
        let cancelled = ui.input(|input| input.key_pressed(egui::Key::Escape));
        if cancelled {
            output.response.surrender_focus();
        }
        if output.response.has_focus() {
            ui.data_mut(|data| data.insert_temp(buffer_id, text));
        } else if !cancelled {
            // Match DragValue's parser and commit-on-blur behavior.
            let normalized: String = text
                .chars()
                .filter(|character| !character.is_whitespace())
                .map(|character| if character == '−' { '-' } else { character })
                .collect();
            if let Ok(parsed) = normalized.parse::<f64>() {
                *value = parsed.clamp(0.0, maximum as f64) as u64;
            }
        }
        output.response.response
    }

    fn ui(&mut self, ui: &mut egui::Ui) -> (bool, bool) {
        let mut changed = false;
        let mut editing = false;
        ui.horizontal(|ui| {
            for (label, value, maximum) in [
                ("Hours", &mut self.hours, 24),
                ("Minutes", &mut self.minutes, 59),
                ("Seconds", &mut self.seconds, 59),
            ] {
                ui.vertical(|ui| {
                    let label = ui.label(RichText::new(label).small().weak());
                    let previous = *value;
                    let response = ui
                        .add_sized([64.0, 36.0], |ui: &mut egui::Ui| {
                            Self::component(ui, value, maximum)
                        })
                        .labelled_by(label.id);
                    changed |= *value != previous;
                    editing |= response.has_focus();
                });
            }
        });
        (changed, editing)
    }
}

pub struct UsefulTimerApp {
    shared: Arc<Mutex<SharedState>>,
    audio: AudioEngine,
    next_id: Option<TimerId>,
    windows: Vec<WidgetWindow>,
    monitors: Vec<Monitor>,
    selected: Option<TimerId>,
    draft: TimerSettings,
    default_duration: Duration,
    duration_input: TimeInput,
    remaining_input: TimeInput,
    remaining_dirty: bool,
    editor_error: Option<String>,
    persistence_warning: Option<String>,
    operation_warning: Option<String>,
    completions: Vec<CompletionEvent>,
    repaint_ids: Vec<(TimerId, bool)>,
    theme: Theme,
    logo: egui::TextureHandle,
    graphics_context: egui::Context,
    updater: Updater,
    restart_requested: Arc<AtomicBool>,
    update_toast: Option<UpdateToast>,
    update_close_at: Option<Instant>,
}

impl UsefulTimerApp {
    pub fn new(context: &eframe::CreationContext<'_>, restart_requested: Arc<AtomicBool>) -> Self {
        let theme = context
            .storage
            .and_then(|storage| eframe::get_value::<Theme>(storage, theme::STORAGE_KEY))
            .unwrap_or_default();
        theme.apply(&context.egui_ctx);
        context.egui_ctx.global_style_mut(|style| {
            style.spacing.item_spacing = egui::vec2(10.0, 10.0);
            style.spacing.button_padding = egui::vec2(14.0, 9.0);
            style.spacing.interact_size.y = 36.0;
            style.spacing.icon_width = 18.0;
            style.spacing.icon_spacing = 10.0;
            style.spacing.slider_width = 140.0;
            for (kind, size) in [
                (egui::TextStyle::Body, 15.0),
                (egui::TextStyle::Button, 15.0),
                (egui::TextStyle::Small, 12.0),
                (egui::TextStyle::Heading, 24.0),
            ] {
                style
                    .text_styles
                    .insert(kind, egui::FontId::proportional(size));
            }
        });

        let (saved, mut persistence_warning) = SavedState::load(context.storage);
        let timers = match saved.into_timers() {
            Ok(timers) => timers,
            Err(error) => {
                persistence_warning = Some(format!("Saved timers could not be restored: {error}"));
                Vec::new()
            }
        };
        let next_id = next_timer_id(&timers);
        let monitors = context
            .winit_window()
            .map(|window| {
                window
                    .available_monitors()
                    .map(|monitor| {
                        let position = monitor.position();
                        let size = monitor.size();
                        Monitor {
                            bounds: egui::Rect::from_min_size(
                                egui::pos2(position.x as f32, position.y as f32),
                                egui::vec2(size.width as f32, size.height as f32),
                            ),
                            scale_factor: monitor.scale_factor() as f32,
                        }
                    })
                    .collect()
            })
            .unwrap_or_default();
        let default_duration = context
            .storage
            .and_then(|storage| eframe::get_value::<u64>(storage, DEFAULT_DURATION_KEY))
            .filter(|seconds| (1..=86_400).contains(seconds))
            .map(Duration::from_secs)
            .unwrap_or_else(|| TimerSettings::default().duration);
        let selected = timers.first().map(|timer| timer.id);
        let draft = timers
            .first()
            .map(|timer| timer.settings().clone())
            .unwrap_or_else(|| TimerSettings {
                duration: default_duration,
                ..TimerSettings::default()
            });
        let app = Self {
            shared: Arc::new(Mutex::new(SharedState {
                timers,
                commands: VecDeque::new(),
            })),
            audio: AudioEngine::new(),
            next_id,
            windows: Vec::new(),
            monitors,
            selected,
            default_duration,
            duration_input: TimeInput::from_duration(draft.duration),
            remaining_input: TimeInput::from_duration(draft.duration),
            remaining_dirty: false,
            draft,
            editor_error: None,
            persistence_warning,
            operation_warning: None,
            completions: Vec::with_capacity(MAX_TIMERS),
            repaint_ids: Vec::with_capacity(MAX_TIMERS),
            theme,
            logo: branding::texture(&context.egui_ctx),
            graphics_context: context.egui_ctx.clone(),
            updater: Updater::new(&context.egui_ctx),
            restart_requested,
            update_toast: None,
            update_close_at: None,
        };
        eprintln!("Useful Timer ready");
        app
    }

    fn update_notification(&mut self, ui: &mut egui::Ui) {
        if matches!(
            self.updater.state,
            UpdateState::Current | UpdateState::Dismissed
        ) {
            return;
        }
        let colors = self.theme.palette();
        egui::Panel::top("update_notification")
            .frame(
                egui::Frame::new()
                    .fill(colors.sidebar)
                    .stroke(egui::Stroke::new(1.0, colors.border))
                    .inner_margin(egui::Margin::symmetric(24, 12)),
            )
            .show(ui, |ui| {
                let mut install = false;
                let mut retry = false;
                let mut dismiss = false;
                match &self.updater.state {
                    UpdateState::Checking => {
                        ui.horizontal(|ui| {
                            ui.spinner();
                            ui.label("Checking for updates…");
                        });
                    }
                    UpdateState::Available(release) => {
                        ui.horizontal_wrapped(|ui| {
                            ui.label(RichText::new(format!(
                                "Useful Timer {} is ready to install.",
                                release.version
                            )).strong());
                            install = ui.button(if cfg!(target_os = "windows") {
                                "Install and restart"
                            } else {
                                "Install update"
                            }).clicked();
                            dismiss = ui.button("Later").clicked();
                            ui.hyperlink_to("Release notes", RELEASES_URL);
                        });
                        ui.small("Restarting restores timers at their full duration; running countdowns do not survive a restart.");
                    }
                    UpdateState::Installing => {
                        ui.horizontal(|ui| {
                            ui.spinner();
                            ui.label("Installing update and verifying its checksum…");
                        });
                        ui.small("Keep the app open until installation finishes.");
                    }
                    UpdateState::Installed => {
                        ui.horizontal(|ui| {
                            ui.label("Update installed. Restart to use the new version.");
                            if ui.button("Restart now").clicked() {
                                self.restart_requested.store(true, Ordering::Relaxed);
                                ui.ctx().send_viewport_cmd_to(ViewportId::ROOT, ViewportCommand::Close);
                            }
                        });
                        ui.small("Restarting resets running and paused timers to their full duration.");
                    }
                    UpdateState::Failed(error) => {
                        ui.horizontal_wrapped(|ui| {
                            ui.label(RichText::new(error).color(colors.warning));
                            retry = ui.button("Check again").clicked();
                            dismiss = ui.button("Dismiss").clicked();
                            ui.hyperlink_to("Releases / manual install", RELEASES_URL);
                        });
                    }
                    UpdateState::Closing => {
                        ui.label("Closing to install the update. Useful Timer will reopen automatically.");
                    }
                    UpdateState::Current | UpdateState::Dismissed => {}
                }
                if install {
                    self.updater.install(ui.ctx());
                } else if retry {
                    self.updater.check(ui.ctx());
                } else if dismiss {
                    self.updater.state = UpdateState::Dismissed;
                }
            });
    }

    fn show_update_toast(&mut self, ctx: &egui::Context) {
        let Some(toast) = &self.update_toast else {
            return;
        };
        let now = Instant::now();
        if now >= toast.expires {
            self.update_toast = None;
            return;
        }
        ctx.request_repaint_after_for(toast.expires - now, ViewportId::ROOT);
        let colors = self.theme.palette();
        let accent = if toast.success {
            Color32::from_rgb(118, 197, 144)
        } else {
            colors.warning
        };
        let mut dismiss = false;
        egui::Area::new(egui::Id::new("update_toast"))
            .order(egui::Order::Foreground)
            .anchor(egui::Align2::RIGHT_BOTTOM, egui::vec2(-20.0, -20.0))
            .show(ctx, |ui| {
                egui::Frame::new()
                    .fill(colors.raised)
                    .stroke(egui::Stroke::new(1.0, accent))
                    .corner_radius(8)
                    .inner_margin(16.0)
                    .show(ui, |ui| {
                        ui.set_width(380.0_f32.min(ctx.content_rect().width() - 72.0).max(160.0));
                        ui.horizontal(|ui| {
                            ui.label(
                                RichText::new(if toast.success {
                                    "Checksum verified"
                                } else {
                                    "Update failed"
                                })
                                .strong()
                                .color(accent),
                            );
                            ui.with_layout(
                                egui::Layout::right_to_left(egui::Align::Center),
                                |ui| {
                                    dismiss = ui.small_button("Dismiss").clicked();
                                },
                            );
                        });
                        ui.label(&toast.message);
                    });
            });
        if dismiss {
            self.update_toast = None;
        }
    }

    fn select(&mut self, view: &TimerView) {
        self.selected = Some(view.id);
        self.draft = view.settings.clone();
        self.duration_input = TimeInput::from_duration(self.draft.duration);
        self.remaining_input = TimeInput::from_duration(view.snapshot.remaining);
        self.remaining_dirty = false;
        self.editor_error = None;
    }

    fn new_draft(&mut self) {
        self.selected = None;
        self.draft = TimerSettings {
            duration: self.default_duration,
            ..TimerSettings::default()
        };
        self.duration_input = TimeInput::from_duration(self.draft.duration);
        self.remaining_dirty = false;
        self.editor_error = None;
    }

    fn submit_settings(&mut self, ctx: &egui::Context) {
        self.draft.duration = self.duration_input.duration();
        if let Err(error) = self.draft.validate() {
            self.editor_error = Some(error);
            return;
        }
        if let Some(id) = self.selected {
            queue(&self.shared, ctx, Command::Settings(id, self.draft.clone()));
        } else {
            let mut state = self.shared.lock().expect("timer state lock poisoned");
            if state.timers.len() >= MAX_TIMERS {
                self.editor_error = Some(format!("At most {MAX_TIMERS} timers can be open."));
                return;
            }
            let Some(id) = self.next_id else {
                self.editor_error = Some("No further timer identities are available.".to_owned());
                return;
            };
            match Timer::new(id, self.draft.clone()) {
                Ok(timer) => {
                    state.timers.push(timer);
                    self.next_id = id.checked_add(1);
                    self.selected = Some(id);
                }
                Err(error) => {
                    self.editor_error = Some(error);
                    return;
                }
            }
            drop(state);
            ctx.request_repaint_of(ViewportId::ROOT);
        }
        self.editor_error = None;
    }

    fn sidebar(&mut self, ui: &mut egui::Ui, views: &[TimerView]) {
        let colors = self.theme.palette();
        ui.horizontal(|ui| {
            ui.label(RichText::new("Your timers").strong());
            ui.label(RichText::new(format!("{:02}", views.len())).color(colors.muted));
        });
        ui.add_space(6.0);
        if ui
            .add_enabled(
                views.len() < MAX_TIMERS,
                egui::Button::new("+  New timer").min_size(egui::vec2(ui.available_width(), 40.0)),
            )
            .on_hover_text("Create an independent desktop countdown")
            .clicked()
        {
            self.new_draft();
        }
        ui.add_space(14.0);
        egui::ScrollArea::vertical()
            .id_salt("timer-list")
            .auto_shrink([false, false])
            .show(ui, |ui| {
                if views.is_empty() {
                    ui.add_space(12.0);
                    ui.label(RichText::new("A little room to focus.").strong());
                    ui.label(
                        RichText::new(
                            "Create your first timer. It will stay above your other windows.",
                        )
                        .color(colors.muted),
                    );
                }
                for view in views {
                    ui.push_id(view.id, |ui| {
                        let selected = self.selected == Some(view.id);
                        let response = ui.add_sized(
                            [ui.available_width(), 112.0],
                            egui::Button::new("")
                                .fill(if selected {
                                    colors.selected
                                } else {
                                    colors.sidebar
                                })
                                .stroke(egui::Stroke::new(
                                    1.0,
                                    if selected {
                                        colors.accent
                                    } else {
                                        colors.border
                                    },
                                ))
                                .corner_radius(12),
                        );
                        let rect = response.rect.shrink(14.0);
                        let label = display_label(&view.settings.label);
                        response.widget_info(|| {
                            egui::WidgetInfo::selected(
                                egui::WidgetType::SelectableLabel,
                                true,
                                selected,
                                label,
                            )
                        });
                        let mut title = egui::text::LayoutJob::simple_singleline(
                            label.to_owned(),
                            egui::FontId::proportional(16.0),
                            colors.text,
                        );
                        title.wrap.max_width = rect.width();
                        title.wrap.max_rows = 1;
                        let title = ui.painter().layout_job(title);
                        ui.painter().galley(rect.min, title, colors.text);
                        ui.painter().text(
                            rect.min + egui::vec2(0.0, 28.0),
                            egui::Align2::LEFT_TOP,
                            visuals::format_remaining(view.snapshot.remaining),
                            egui::FontId::monospace(26.0),
                            if selected { colors.accent } else { colors.text },
                        );
                        ui.painter().text(
                            rect.left_bottom(),
                            egui::Align2::LEFT_BOTTOM,
                            view.settings.style.label(),
                            egui::FontId::proportional(12.0),
                            colors.muted,
                        );
                        ui.painter().text(
                            rect.right_bottom(),
                            egui::Align2::RIGHT_BOTTOM,
                            phase_label(view.snapshot.phase),
                            egui::FontId::proportional(12.0),
                            if view.snapshot.phase == TimerPhase::Running {
                                colors.accent
                            } else {
                                colors.muted
                            },
                        );
                        if response.on_hover_text(label).clicked() {
                            self.select(view);
                        }
                    });
                }
            });
    }

    fn live_controls(&mut self, ui: &mut egui::Ui, view: &TimerView) {
        let colors = self.theme.palette();
        egui::Frame::new()
            .fill(colors.surface)
            .corner_radius(16)
            .inner_margin(22.0)
            .show(ui, |ui| {
                ui.set_width(ui.available_width());
                ui.horizontal(|ui| {
                    ui.label(RichText::new(phase_label(view.snapshot.phase)).color(colors.accent));
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        ui.label(
                            RichText::new("CURRENT COUNTDOWN")
                                .small()
                                .color(colors.muted),
                        );
                    });
                });
                ui.label(
                    RichText::new(visuals::format_remaining(view.snapshot.remaining))
                        .monospace()
                        .size(if ui.available_width() < 480.0 {
                            48.0
                        } else {
                            64.0
                        }),
                );
                ui.add(
                    egui::ProgressBar::new(view.snapshot.remaining_fraction)
                        .fill(colors.accent)
                        .desired_height(4.0),
                );
                ui.add_space(4.0);
                ui.horizontal(|ui| {
                    if let Some(action) = timer_controls(ui, view.snapshot.phase, colors) {
                        queue(&self.shared, ui.ctx(), Command::Action(view.id, action));
                        self.remaining_dirty = false;
                    }
                });
            });
        if matches!(
            view.snapshot.phase,
            TimerPhase::Running | TimerPhase::Paused
        ) {
            egui::CollapsingHeader::new("Adjust remaining time")
                .id_salt(("remaining", view.id))
                .show(ui, |ui| {
                    if !self.remaining_dirty {
                        self.remaining_input = TimeInput::from_duration(view.snapshot.remaining);
                    }
                    ui.push_id("remaining-time", |ui| {
                        let (changed, editing) = self.remaining_input.ui(ui);
                        self.remaining_dirty |= changed || editing;
                    });
                    let remaining = self.remaining_input.duration();
                    ui.horizontal_wrapped(|ui| {
                        if ui
                            .add_enabled(
                                remaining <= Duration::from_secs(86_400),
                                egui::Button::new("Set remaining time"),
                            )
                            .clicked()
                        {
                            queue(
                                &self.shared,
                                ui.ctx(),
                                Command::Action(view.id, TimerAction::SetRemaining(remaining)),
                            );
                            self.remaining_dirty = false;
                        }
                        for (label, seconds) in [
                            ("−5 min", -300),
                            ("−1 min", -60),
                            ("+1 min", 60),
                            ("+5 min", 300),
                        ] {
                            if ui
                                .add_sized([80.0, 36.0], egui::Button::new(label))
                                .clicked()
                            {
                                queue(
                                    &self.shared,
                                    ui.ctx(),
                                    Command::Action(view.id, TimerAction::AdjustRemaining(seconds)),
                                );
                                self.remaining_dirty = false;
                            }
                        }
                    });
                    ui.label(
                        RichText::new(
                            "Keeps the current running or paused state. Zero finishes the timer.",
                        )
                        .small()
                        .color(colors.muted),
                    );
                });
        }
    }

    fn artwork_picker(&mut self, ui: &mut egui::Ui) -> bool {
        let colors = self.theme.palette();
        ui.label(RichText::new("Desktop artwork").size(17.0).strong());
        let columns = ((ui.available_width() + 10.0) / 170.0)
            .floor()
            .clamp(1.0, 4.0) as usize;
        let width = (ui.available_width() - 10.0 * (columns - 1) as f32) / columns as f32;
        let mut changed = false;
        let mut settings = self.draft.clone();
        let preview = TimerSnapshot {
            phase: TimerPhase::Idle,
            remaining: self.duration_input.duration(),
            remaining_fraction: 1.0,
            effect_elapsed: None,
            animation_elapsed: Duration::ZERO,
        };
        egui::Grid::new("artwork-styles")
            .num_columns(columns)
            .spacing(egui::vec2(10.0, 10.0))
            .show(ui, |ui| {
                for (index, style) in TimerStyle::ALL.into_iter().enumerate() {
                    let selected = self.draft.style == style;
                    let response = ui.add_sized(
                        [width, 148.0],
                        egui::Button::new("")
                            .fill(if selected {
                                colors.selected
                            } else {
                                colors.surface
                            })
                            .corner_radius(12)
                            .stroke(egui::Stroke::new(
                                1.0,
                                if selected {
                                    colors.accent
                                } else {
                                    colors.border
                                },
                            )),
                    );
                    response.widget_info(|| {
                        egui::WidgetInfo::selected(
                            egui::WidgetType::SelectableLabel,
                            true,
                            selected,
                            style.label(),
                        )
                    });
                    let rect = response.rect;
                    let artwork = egui::Rect::from_center_size(
                        egui::pos2(rect.center().x, rect.top() + 57.0),
                        Vec2::splat(96.0_f32.min(width - 8.0)),
                    );
                    settings.style = style;
                    visuals::draw_timer(ui.painter(), artwork, &settings, &preview);
                    ui.painter().text(
                        egui::pos2(rect.center().x, rect.bottom() - 20.0),
                        egui::Align2::CENTER_CENTER,
                        style.label(),
                        egui::FontId::proportional(14.0),
                        if selected { colors.accent } else { colors.text },
                    );
                    if response.clicked() && !selected {
                        self.draft.style = style;
                        changed = true;
                    }
                    if (index + 1) % columns == 0 {
                        ui.end_row();
                    }
                }
            });
        changed
    }

    fn duration_controls(&mut self, ui: &mut egui::Ui) -> bool {
        let colors = self.theme.palette();
        ui.label(RichText::new("Duration").size(17.0).strong());
        let mut changed = ui
            .push_id("configured-duration", |ui| self.duration_input.ui(ui).0)
            .inner;
        ui.horizontal_wrapped(|ui| {
            for (label, seconds) in [
                ("+1 min", 60),
                ("−1 min", -60),
                ("+5 min", 300),
                ("−5 min", -300),
            ] {
                if ui
                    .add_sized([80.0, 36.0], egui::Button::new(label))
                    .clicked()
                {
                    let seconds = self
                        .duration_input
                        .duration()
                        .as_secs()
                        .saturating_add_signed(seconds)
                        .clamp(1, 86_400);
                    self.duration_input = TimeInput::from_duration(Duration::from_secs(seconds));
                    changed = true;
                }
            }
        });
        ui.add_space(12.0);
        ui.horizontal_wrapped(|ui| {
            let duration = self.duration_input.duration();
            if ui
                .add_enabled(
                    (Duration::from_secs(1)..=Duration::from_secs(86_400)).contains(&duration)
                        && duration != self.default_duration,
                    egui::Button::new("Save duration as default")
                        .fill(Color32::TRANSPARENT)
                        .stroke(egui::Stroke::new(1.0, colors.border)),
                )
                .on_hover_text("Use this duration for new timers, including after restarting the app. Existing timers are unchanged.")
                .clicked()
            {
                self.default_duration = duration;
            }
            ui.label(
                RichText::new(format!(
                    "Default: {}",
                    visuals::format_remaining(self.default_duration)
                ))
                .small()
                .color(colors.muted),
            );
        });
        ui.add_space(16.0);
        ui.separator();
        ui.add_space(8.0);
        ui.label(RichText::new("Duration presets").size(17.0).strong());
        let columns = ((ui.available_width() + 10.0) / 90.0)
            .floor()
            .clamp(1.0, 3.0) as usize;
        egui::Grid::new("duration-presets")
            .num_columns(columns)
            .spacing(egui::vec2(10.0, 10.0))
            .show(ui, |ui| {
                for (index, (label, seconds)) in [
                    ("30 sec", 30),
                    ("1 min", 60),
                    ("3 min", 180),
                    ("5 min", 300),
                    ("10 min", 600),
                    ("15 min", 900),
                    ("30 min", 1800),
                    ("1 h", 3600),
                    ("2 h", 7200),
                ]
                .into_iter()
                .enumerate()
                {
                    let duration = Duration::from_secs(seconds);
                    if ui
                        .add_sized(
                            [80.0, 36.0],
                            egui::Button::new(label)
                                .selected(self.duration_input.duration() == duration),
                        )
                        .clicked()
                    {
                        self.duration_input = TimeInput::from_duration(duration);
                        changed = true;
                    }
                    if (index + 1) % columns == 0 {
                        ui.end_row();
                    }
                }
            });
        ui.label(
            RichText::new("Changing duration resets the countdown.")
                .small()
                .color(colors.muted),
        );
        changed
    }

    fn widget_settings(&mut self, ui: &mut egui::Ui) -> bool {
        let colors = self.theme.palette();
        ui.label(RichText::new("Widget").size(17.0).strong());
        let mut changed = ui
            .horizontal(|ui| {
                let label = ui.label(RichText::new("Size").color(colors.muted));
                ui.add(
                    egui::Slider::new(&mut self.draft.size, 220.0..=460.0)
                        .suffix(" px")
                        .integer(),
                )
                .labelled_by(label.id)
                .changed()
            })
            .inner;
        changed |= ui
            .checkbox(&mut self.draft.reduced_motion, "Reduced motion")
            .on_hover_text(
                "Keep progress and time visible without ambient motion or completion bursts",
            )
            .changed();
        changed
    }

    fn sound_settings(&mut self, ui: &mut egui::Ui) -> bool {
        let colors = self.theme.palette();
        let mut volume_percent = self.draft.volume * 100.0;
        ui.label(RichText::new("Sound").size(17.0).strong());
        let mut changed = ui
            .horizontal(|ui| {
                let label = ui.label(RichText::new("Volume").color(colors.muted));
                ui.add_enabled(
                    !self.draft.muted,
                    egui::Slider::new(&mut volume_percent, 0.0..=100.0)
                        .suffix("%")
                        .fixed_decimals(0),
                )
                .labelled_by(label.id)
                .changed()
            })
            .inner;
        if changed {
            self.draft.volume = volume_percent / 100.0;
        }
        changed |= ui
            .checkbox(&mut self.draft.muted, "Mute completion sound")
            .changed();
        changed
    }

    fn editor(&mut self, ui: &mut egui::Ui, views: &[TimerView]) {
        let colors = self.theme.palette();
        egui::ScrollArea::vertical()
            .id_salt(("workspace", self.selected))
            .auto_shrink([false, false])
            .show(ui, |ui| {
                ui.set_max_width(ui.available_width().min(860.0));
                let mut changed = false;
                ui.horizontal(|ui| {
                    ui.label(RichText::new(if self.selected.is_some() { "TIMER WORKSPACE" } else { "NEW TIMER" }).small().color(colors.muted));
                    if let Some(id) = self.selected {
                        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                            ui.menu_button("•••", |ui| {
                                if ui.button("Restore defaults").on_hover_text("Reset to Focus, 25 minutes, Bomb, 300 px, volume 60%, and idle.").clicked() {
                                    queue(&self.shared, ui.ctx(), Command::Action(id, TimerAction::RestoreDefaults));
                                    self.draft = TimerSettings::default();
                                    self.duration_input = TimeInput::from_duration(self.draft.duration);
                                    self.remaining_dirty = false;
                                    self.editor_error = None;
                                    ui.close();
                                }
                                ui.separator();
                                if ui.button(RichText::new("Remove timer").color(colors.danger)).clicked() {
                                    queue(&self.shared, ui.ctx(), Command::Remove(id));
                                    ui.close();
                                }
                            }).response.on_hover_text("Timer actions");
                        });
                    }
                });
                changed |= ui.add(
                    egui::TextEdit::singleline(&mut self.draft.label)
                        .font(egui::FontId::proportional(28.0))
                        .char_limit(80)
                        .hint_text("Name your timer")
                        .desired_width(ui.available_width())
                        .frame(egui::Frame::NONE),
                ).on_hover_text("Timer name · click to rename").changed();
                ui.label(RichText::new(if self.selected.is_some() {
                    "Make it yours. Changes apply instantly."
                } else {
                    "A countdown with a little character. Always on your desktop."
                }).color(colors.muted));
                ui.add_space(8.0);
                if let Some(view) = views.iter().find(|view| Some(view.id) == self.selected) {
                    self.live_controls(ui, view);
                    ui.add_space(10.0);
                    ui.separator();
                    ui.add_space(8.0);
                }
                changed |= self.artwork_picker(ui);
                ui.add_space(12.0);
                ui.separator();
                ui.add_space(8.0);
                changed |= self.duration_controls(ui);
                ui.add_space(16.0);
                ui.separator();
                ui.add_space(8.0);
                changed |= self.widget_settings(ui);
                ui.add_space(16.0);
                ui.separator();
                ui.add_space(8.0);
                changed |= self.sound_settings(ui);
                if changed && self.selected.is_some() {
                    self.submit_settings(ui.ctx());
                }
                if let Some(error) = &self.editor_error {
                    ui.colored_label(colors.danger, error);
                }
                if self.selected.is_none() {
                    ui.add_space(12.0);
                    ui.horizontal(|ui| {
                        if ui.add(primary_button("Create timer", colors).min_size(egui::vec2(160.0, 42.0))).clicked() {
                            self.submit_settings(ui.ctx());
                        }
                        if ui.add(egui::Button::new("Reset form").fill(Color32::TRANSPARENT).stroke(egui::Stroke::new(1.0, colors.border)).min_size(egui::vec2(160.0, 42.0))).clicked() {
                            self.new_draft();
                        }
                    });
                }
                ui.add_space(16.0);
                ui.separator();
                ui.label(RichText::new("Drag a desktop widget to move it. Right-click it for controls.").small().color(colors.muted));
                ui.label(RichText::new("Saved on this device · Reopens at full duration · Closing this app quits all timers").small().color(colors.muted));
            });
    }

    fn register_widgets(&mut self, ctx: &egui::Context, views: &[TimerView]) {
        self.windows
            .retain(|window| views.iter().any(|view| view.id == window.timer_id));
        let root_pixels_per_point = ctx.pixels_per_point();
        let root_position = ctx.input(|input| {
            input.viewport().outer_rect.map(|rect| {
                let position = capture_position(rect.center(), root_pixels_per_point);
                egui::pos2(position.x, position.y)
            })
        });
        for view in views {
            let index = match self
                .windows
                .iter()
                .position(|window| window.timer_id == view.id)
            {
                Some(index) => index,
                None => {
                    self.windows.push(WidgetWindow::new(
                        view,
                        &self.monitors,
                        root_position,
                        ctx.zoom_factor(),
                    ));
                    self.windows.len() - 1
                }
            };
            let window = &mut self.windows[index];
            window.update(&view.settings);
            // The child's native scale is unknown until its first deferred callback.
            // Keep its builder unpositioned and share the one-shot placement across callbacks.
            let builder = window.builder.clone();
            let initial_position = Arc::clone(&window.initial_position);
            let shared = Arc::clone(&self.shared);
            let id = view.id;
            ctx.show_viewport_deferred(window.viewport_id, builder, move |ui, _class| {
                widget_ui(ui, id, &shared, &initial_position);
            });
        }
    }
}

impl eframe::App for UsefulTimerApp {
    fn logic(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        self.updater.poll();
        let now = Instant::now();
        if let Some(result) = self.updater.take_installation_result() {
            let success = result.is_ok();
            let closing = matches!(self.updater.state, UpdateState::Closing);
            let message = match result {
                Ok(()) if closing => "Download verified. Restarting to finish installation…".into(),
                Ok(()) => "Update installed successfully. Restart when you are ready.".into(),
                Err(error) => error,
            };
            let duration = if closing {
                Duration::from_secs(3)
            } else {
                Duration::from_secs(10)
            };
            self.update_toast = Some(UpdateToast {
                message,
                success,
                expires: now + duration,
            });
            if closing {
                self.update_close_at = Some(now + duration);
            }
        }
        if let Some(deadline) = self.update_close_at {
            if now >= deadline {
                ctx.send_viewport_cmd_to(ViewportId::ROOT, ViewportCommand::Close);
            } else {
                ctx.request_repaint_after_for(deadline - now, ViewportId::ROOT);
            }
        }
        let mut next_tick = None;
        let selection_removed;
        {
            let mut state = self.shared.lock().expect("timer state lock poisoned");
            while let Some(command) = state.commands.pop_front() {
                let id = match &command {
                    Command::Action(id, _)
                    | Command::Settings(id, _)
                    | Command::Locked(id, _)
                    | Command::Remove(id) => *id,
                };
                let Some(index) = state.timers.iter().position(|timer| timer.id == id) else {
                    continue;
                };
                let removed = matches!(&command, Command::Remove(_));
                let completion = match command {
                    Command::Action(_, action) => state.timers[index].apply(action, now),
                    Command::Settings(_, settings) => {
                        match state.timers[index].update_settings(settings, now) {
                            Ok(event) => event,
                            Err(error) => {
                                self.operation_warning = Some(error);
                                None
                            }
                        }
                    }
                    Command::Locked(_, locked) => {
                        state.timers[index].locked = locked;
                        None
                    }
                    Command::Remove(_) => {
                        state.timers.remove(index);
                        None
                    }
                };
                if let Some(event) = completion {
                    self.completions.push(event);
                }
                self.repaint_ids.push((id, removed));
            }
            for timer in &mut state.timers {
                if let Some(event) = timer.advance(now) {
                    self.completions.push(event);
                    self.repaint_ids.push((timer.id, false));
                }
                let snapshot = timer.snapshot(now);
                if snapshot.phase == TimerPhase::Running {
                    let delay = snapshot.remaining.min(LOGIC_INTERVAL);
                    next_tick =
                        Some(next_tick.map_or(delay, |current: Duration| current.min(delay)));
                }
            }
            selection_removed = self
                .selected
                .is_some_and(|id| !state.timers.iter().any(|timer| timer.id == id));
        }
        if selection_removed {
            self.new_draft();
        }
        // Audio and egui callbacks can never run while the timer mutex is held.
        for event in self.completions.drain(..) {
            self.audio.play_completion(event);
        }
        for (id, removed) in self.repaint_ids.drain(..) {
            if let Some(window) = self.windows.iter().find(|window| window.timer_id == id) {
                if removed {
                    ctx.send_viewport_cmd_to(window.viewport_id, ViewportCommand::Close);
                } else {
                    ctx.request_repaint_of(window.viewport_id);
                }
            }
        }
        // ROOT scheduling calls App::logic even when every native window is hidden.
        // Visible deferred children schedule their own artwork independently.
        if let Some(delay) = next_tick {
            ctx.request_repaint_after_for(delay.max(Duration::from_millis(20)), ViewportId::ROOT);
        }
    }

    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        let colors = self.theme.palette();
        let previous_theme = self.theme;
        let now = Instant::now();
        let mut views: Vec<_> = {
            let state = self.shared.lock().expect("timer state lock poisoned");
            state
                .timers
                .iter()
                .map(|timer| TimerView::from_timer(timer, now))
                .collect()
        };
        egui::Panel::top("title")
            .frame(
                egui::Frame::new()
                    .fill(colors.sidebar)
                    .inner_margin(egui::Margin::symmetric(24, 16)),
            )
            .show(ui, |ui| {
                ui.horizontal(|ui| {
                    ui.add(egui::Image::new(&self.logo).fit_to_exact_size(Vec2::splat(38.0)));
                    ui.add_space(2.0);
                    ui.label(RichText::new("Useful Timer").size(22.0).strong());
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        egui::ComboBox::from_label("Theme")
                            .selected_text(self.theme.label())
                            .width(110.0)
                            .show_ui(ui, |ui| {
                                for theme in [Theme::Charcoal, Theme::GitHub] {
                                    ui.selectable_value(&mut self.theme, theme, theme.label());
                                }
                            });
                    });
                });
            });
        if self.theme != previous_theme {
            self.theme.apply(ui.ctx());
            ui.ctx().request_discard("theme changed");
            ui.ctx().request_repaint();
            for window in &self.windows {
                ui.ctx().request_repaint_of(window.viewport_id);
            }
        }
        self.update_notification(ui);
        if self.persistence_warning.is_some()
            || self.operation_warning.is_some()
            || self.audio.warning().is_some()
        {
            egui::Panel::bottom("warnings")
                .frame(
                    egui::Frame::new()
                        .fill(colors.warning_background)
                        .stroke(egui::Stroke::new(1.0, colors.warning))
                        .inner_margin(12.0),
                )
                .show(ui, |ui| {
                    for warning in [
                        self.persistence_warning.as_deref(),
                        self.operation_warning.as_deref(),
                        self.audio.warning(),
                    ]
                    .into_iter()
                    .flatten()
                    {
                        ui.label(RichText::new(warning).color(colors.text));
                    }
                });
        }
        egui::Panel::left("timers")
            .resizable(false)
            .exact_size(240.0)
            .frame(egui::Frame::new().fill(colors.sidebar).inner_margin(20.0))
            .show(ui, |ui| self.sidebar(ui, &views));
        egui::CentralPanel::default()
            .frame(
                egui::Frame::new()
                    .fill(colors.background)
                    .inner_margin(28.0),
            )
            .show(ui, |ui| self.editor(ui, &views));

        // A timer added during this pass must be registered immediately, too.
        {
            let state = self.shared.lock().expect("timer state lock poisoned");
            let already_shown = views.len();
            views.extend(
                state
                    .timers
                    .iter()
                    .skip(already_shown)
                    .map(|timer| TimerView::from_timer(timer, now)),
            );
        }
        self.register_widgets(ui.ctx(), &views);
        self.show_update_toast(ui.ctx());
    }

    fn save(&mut self, storage: &mut dyn eframe::Storage) {
        eframe::set_value(storage, theme::STORAGE_KEY, &self.theme);
        eframe::set_value(storage, DEFAULT_DURATION_KEY, &self.default_duration.as_secs());
        let saved = {
            let state = self.shared.lock().expect("timer state lock poisoned");
            SavedState::from_timers(&state.timers)
        };
        if let Err(error) = saved.save(storage) {
            self.persistence_warning = Some(format!("Timer settings were not saved: {error}"));
        }
    }

    fn on_exit(&mut self, gl: Option<&eframe::glow::Context>) {
        if let Some(gl) = gl {
            visuals::destroy_renderer(&self.graphics_context, gl);
        }
    }

    fn auto_save_interval(&self) -> Duration {
        Duration::from_secs(5)
    }

    fn persist_egui_memory(&self) -> bool {
        false
    }

    fn clear_color(&self, _visuals: &egui::Visuals) -> [f32; 4] {
        [0.0; 4]
    }
}

fn queue(shared: &Arc<Mutex<SharedState>>, ctx: &egui::Context, command: Command) {
    {
        let mut state = shared.lock().expect("timer state lock poisoned");
        state.commands.push_back(command);
    }
    ctx.request_repaint_of(ViewportId::ROOT);
}

fn widget_ui(
    ui: &mut egui::Ui,
    id: TimerId,
    shared: &Arc<Mutex<SharedState>>,
    initial_position: &Mutex<InitialPosition>,
) {
    let ctx = ui.ctx().clone();
    if ctx.input(|input| input.viewport().close_requested()) {
        queue(shared, &ctx, Command::Remove(id));
        ctx.send_viewport_cmd(ViewportCommand::Visible(false));
        return;
    }
    let pixels_per_point = ctx.pixels_per_point();
    let (initial_position, positioning) = take_initial_position(
        initial_position,
        pixels_per_point,
        ctx.cumulative_frame_nr(),
    );
    if let Some(position) = initial_position {
        ctx.send_viewport_cmd(ViewportCommand::OuterPosition(position));
        ctx.request_repaint();
    }
    let now = Instant::now();
    let outer = ctx.input(|input| input.viewport().outer_rect);
    let view = {
        let mut state = shared.lock().expect("timer state lock poisoned");
        state
            .timers
            .iter_mut()
            .find(|timer| timer.id == id)
            .map(|timer| {
                // This pass still reports the pre-move rectangle; capture on a later pass.
                if !positioning
                    && let Some(rect) = outer
                    && rect.min.is_finite()
                {
                    timer.position = Some(capture_position(rect.min, pixels_per_point));
                }
                TimerView::from_timer(timer, now)
            })
    };
    let Some(view) = view else {
        ctx.send_viewport_cmd(ViewportCommand::Close);
        return;
    };
    let size = widget_size(view.settings.size);
    if ctx.input(|input| {
        input
            .viewport()
            .inner_rect
            .is_some_and(|rect| rect.size() != size)
    }) {
        ctx.send_viewport_cmd(ViewportCommand::InnerSize(size));
    }
    ui.spacing_mut().item_spacing = egui::vec2(6.0, 6.0);
    ui.spacing_mut().button_padding = egui::vec2(9.0, 5.0);
    ui.spacing_mut().interact_size.y = 28.0;
    egui::CentralPanel::default()
        .frame(egui::Frame::NONE.inner_margin(8.0))
        .show(ui, |ui| {
            let width = ui.available_width();
            let font = egui::FontId::proportional(19.0);
            let label = display_label(&view.settings.label);
            let color = Color32::from_rgb(240, 244, 250);
            let mut title = egui::text::LayoutJob::simple_singleline(label.to_owned(), font, color);
            title.wrap.max_width = (width - 38.0).max(1.0);
            title.wrap.max_rows = 1;
            let title = ui.painter().layout_job(title);
            let badge_width = title.size().x + 38.0;
            ui.add_space(WIDGET_TITLE_HEIGHT);
            let (rect, response) =
                ui.allocate_exact_size(Vec2::splat(width), egui::Sense::click_and_drag());
            visuals::draw_timer(ui.painter(), rect, &view.settings, &view.snapshot);
            // Keep a 12-point gap to each style's upper silhouette at every widget size.
            let artwork_top = visuals::artwork_top(view.settings.style);
            let badge = egui::Rect::from_center_size(
                egui::pos2(
                    rect.center().x,
                    rect.top() + artwork_top * width / 320.0 - 30.0,
                ),
                egui::vec2(badge_width, 36.0),
            );
            let title_response = ui.interact(
                badge,
                ui.id().with("timer-caption"),
                egui::Sense::click_and_drag(),
            );
            ui.painter().rect_filled(
                badge.translate(egui::vec2(0.0, 2.0)),
                12.0,
                Color32::from_black_alpha(45),
            );
            ui.painter().rect_filled(
                badge,
                10.0,
                Color32::from_rgba_unmultiplied(22, 27, 35, 235),
            );
            ui.painter().rect_stroke(
                badge,
                10.0,
                egui::Stroke::new(1.0, Color32::from_rgba_unmultiplied(190, 207, 229, 75)),
                egui::StrokeKind::Inside,
            );
            let accent = match view.settings.style {
                TimerStyle::Bomb => Color32::from_rgb(255, 174, 91),
                TimerStyle::Hourglass => Color32::from_rgb(241, 207, 134),
                TimerStyle::Rocket => Color32::from_rgb(126, 217, 230),
                TimerStyle::CodeRain => Color32::from_rgb(103, 255, 174),
                TimerStyle::MachineCore => Color32::from_rgb(255, 94, 83),
                TimerStyle::DragonOrb => Color32::from_rgb(255, 190, 76),
                TimerStyle::CrescentWand => Color32::from_rgb(174, 224, 255),
                TimerStyle::ClockworkBloom => Color32::from_rgb(226, 195, 135),
            };
            ui.painter().circle_filled(
                egui::pos2(badge.left() + 13.0, badge.center().y),
                3.0,
                accent,
            );
            ui.painter().galley(
                egui::pos2(badge.left() + 24.0, badge.center().y - title.size().y * 0.5),
                title,
                color,
            );
            let interaction = response.union(title_response);
            interaction.context_menu(|ui| {
                let (label, action) = phase_action(view.snapshot.phase);
                for (label, action, enabled) in [
                    (label, action, view.snapshot.phase != TimerPhase::Finished),
                    ("Reset", TimerAction::Reset, true),
                ] {
                    if ui.add_enabled(enabled, egui::Button::new(label)).clicked() {
                        queue(shared, &ctx, Command::Action(id, action));
                        ui.close();
                    }
                }
                if ui.button("Remove").clicked() {
                    queue(shared, &ctx, Command::Remove(id));
                    ui.close();
                }
                if ui
                    .button(if view.locked { "Unlock" } else { "Lock" })
                    .clicked()
                {
                    queue(shared, &ctx, Command::Locked(id, !view.locked));
                    ui.close();
                }
            });
            // Start native dragging on the press, not after egui's drag threshold.
            // Title and artwork both drag; controls live in the context menu and panel.
            if !view.locked
                && !interaction.context_menu_opened()
                && interaction.hovered()
                && ui.input(|input| input.pointer.primary_pressed())
            {
                ctx.send_viewport_cmd(ViewportCommand::StartDrag);
            }
        });
    if visuals::needs_animation(&view.settings, &view.snapshot) {
        // Delay plus egui's frame prediction targets smooth native animation near 60fps.
        ctx.request_repaint_after(FRAME_INTERVAL);
    }
}

fn primary_button<'a>(label: &'a str, colors: &Palette) -> egui::Button<'a> {
    egui::Button::new(RichText::new(label).color(colors.primary_text).strong()).fill(colors.primary)
}

fn phase_action(phase: TimerPhase) -> (&'static str, TimerAction) {
    match phase {
        TimerPhase::Idle | TimerPhase::Finished => ("Start", TimerAction::Start),
        TimerPhase::Running => ("Pause", TimerAction::Pause),
        TimerPhase::Paused => ("Resume", TimerAction::Resume),
    }
}

fn timer_controls(ui: &mut egui::Ui, phase: TimerPhase, colors: &Palette) -> Option<TimerAction> {
    let (label, action) = phase_action(phase);
    let mut selected = None;
    let button = primary_button(label, colors).min_size(egui::vec2(100.0, 36.0));
    if ui
        .add_enabled(phase != TimerPhase::Finished, button)
        .clicked()
    {
        selected = Some(action);
    }
    let reset = ui.add_sized([100.0, 36.0], egui::Button::new("Reset"));
    if reset.clicked() {
        selected = Some(TimerAction::Reset);
    }
    selected
}

fn display_label(label: &str) -> &str {
    if label.is_empty() {
        "Untitled timer"
    } else {
        label
    }
}

fn phase_label(phase: TimerPhase) -> &'static str {
    match phase {
        TimerPhase::Idle => "Ready",
        TimerPhase::Running => "Running",
        TimerPhase::Paused => "Paused",
        TimerPhase::Finished => "Done",
    }
}

fn widget_size(width: f32) -> Vec2 {
    egui::vec2(width, width + WIDGET_TITLE_HEIGHT)
}

fn widget_title(id: TimerId, settings: &TimerSettings) -> String {
    format!(
        "Useful Timer - {} - {id} - {}",
        settings.style.label(),
        display_label(&settings.label)
    )
}

fn capture_position(position: egui::Pos2, pixels_per_point: f32) -> WidgetPosition {
    // egui-winit divides the physical outer position by this viewport's pixels per point.
    let physical = position * pixels_per_point;
    WidgetPosition {
        x: physical.x,
        y: physical.y,
    }
}

fn take_initial_position(
    initial_position: &Mutex<InitialPosition>,
    pixels_per_point: f32,
    frame: u64,
) -> (Option<egui::Pos2>, bool) {
    // OuterPosition multiplies by the current native window's pixels per point.
    // Consume once so later callbacks never undo a native drag.
    let mut initial = initial_position
        .lock()
        .expect("widget position lock poisoned");
    let position = initial.physical.take().map(|position| {
        initial.applied_frame = Some(frame);
        position / pixels_per_point
    });
    // egui may run multiple passes before native commands are applied at frame end.
    (position, initial.applied_frame == Some(frame))
}

fn bottom_left_position(
    size: Vec2,
    monitors: &[Monitor],
    root_position: Option<egui::Pos2>,
    zoom_factor: f32,
) -> Option<egui::Pos2> {
    let monitor = match root_position {
        Some(position) => monitors.iter().min_by(|left, right| {
            left.bounds
                .distance_sq_to_pos(position)
                .total_cmp(&right.bounds.distance_sq_to_pos(position))
        }),
        None => monitors.first(),
    }?;
    let pixels_per_point = monitor.scale_factor * zoom_factor;
    let physical_size = size * pixels_per_point;
    let margin = 16.0 * pixels_per_point;
    let maximum = (monitor.bounds.max - physical_size).max(monitor.bounds.min);
    Some(egui::pos2(
        (monitor.bounds.min.x + margin).min(maximum.x),
        (maximum.y - margin).max(monitor.bounds.min.y),
    ))
}

fn clamp_position(
    position: egui::Pos2,
    size: Vec2,
    monitors: &[Monitor],
    zoom_factor: f32,
) -> egui::Pos2 {
    // No monitor coordinates means no defensible clamp; do not invent origin (0,0).
    let Some(monitor) = monitors.iter().min_by(|left, right| {
        left.bounds
            .distance_sq_to_pos(position)
            .total_cmp(&right.bounds.distance_sq_to_pos(position))
    }) else {
        return position;
    };
    let physical_size = size * monitor.scale_factor * zoom_factor;
    let maximum = (monitor.bounds.max - physical_size).max(monitor.bounds.min);
    egui::pos2(
        position.x.clamp(monitor.bounds.min.x, maximum.x),
        position.y.clamp(monitor.bounds.min.y, maximum.y),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    fn mixed_dpi_monitors() -> [Monitor; 2] {
        [
            Monitor {
                bounds: egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(1920.0, 1080.0)),
                scale_factor: 1.0,
            },
            Monitor {
                bounds: egui::Rect::from_min_size(
                    egui::pos2(1920.0, 0.0),
                    egui::vec2(2560.0, 1440.0),
                ),
                scale_factor: 2.0,
            },
        ]
    }

    #[test]
    fn new_widget_opens_at_bottom_left_of_control_panel_monitor() {
        let monitors = mixed_dpi_monitors();
        let timer = Timer::new(1, TimerSettings::default()).unwrap();
        let view = TimerView::from_timer(&timer, Instant::now());
        let window = WidgetWindow::new(&view, &monitors, Some(egui::pos2(2600.0, 500.0)), 1.25);
        let position = take_initial_position(&window.initial_position, 2.5, 0)
            .0
            .unwrap()
            * 2.5;
        let size = widget_size(view.settings.size) * 2.5;
        assert_eq!(position.x, monitors[1].bounds.min.x + 40.0);
        assert_eq!(position.y + size.y, monitors[1].bounds.max.y - 40.0);
        assert!(
            monitors[1]
                .bounds
                .contains_rect(egui::Rect::from_min_size(position, size,))
        );
    }

    #[test]
    fn new_widget_uses_negative_monitor_origin_without_control_panel_position() {
        let monitors = [Monitor {
            bounds: egui::Rect::from_min_size(
                egui::pos2(-1920.0, -1080.0),
                egui::vec2(1920.0, 1080.0),
            ),
            scale_factor: 1.0,
        }];
        let timer = Timer::new(1, TimerSettings::default()).unwrap();
        let view = TimerView::from_timer(&timer, Instant::now());
        let window = WidgetWindow::new(&view, &monitors, None, 1.0);
        let position = take_initial_position(&window.initial_position, 1.0, 0)
            .0
            .unwrap();
        assert_eq!(position.x, -1904.0);
        assert_eq!(position.y + widget_size(view.settings.size).y, -16.0);
    }

    fn captured_view(position_in_points: egui::Pos2, pixels_per_point: f32) -> TimerView {
        let mut timer = Timer::new(1, TimerSettings::default()).unwrap();
        timer.position = Some(capture_position(position_in_points, pixels_per_point));
        TimerView::from_timer(&timer, Instant::now())
    }

    #[test]
    fn mixed_dpi_capture_restores_on_original_monitor_once() {
        let monitors = mixed_dpi_monitors();
        // A physical x=2200 on the 2x monitor is reported by egui as x=1100.
        let view = captured_view(egui::pos2(1100.0, 150.0), 2.0);
        let window = WidgetWindow::new(&view, &monitors, Some(egui::pos2(100.0, 100.0)), 1.0);
        // The new window can initially be created on the 1x monitor.
        let native_pixels_per_point = 1.0;
        let (command_position, positioning) =
            take_initial_position(&window.initial_position, native_pixels_per_point, 0);
        let command_position = command_position.unwrap();
        assert!(positioning);
        let physical_position = command_position * native_pixels_per_point;
        assert_eq!(physical_position, egui::pos2(2200.0, 300.0));
        assert!(monitors[1].bounds.contains(physical_position));
        assert!(!monitors[0].bounds.contains(physical_position));
        // A repeated pass must neither resend the move nor capture pre-move coordinates.
        assert_eq!(
            take_initial_position(&window.initial_position, 1.0, 0),
            (None, true)
        );
        // Later frames can capture native dragging without snapping back.
        assert_eq!(
            take_initial_position(&window.initial_position, 2.0, 1),
            (None, false)
        );
    }

    #[test]
    fn high_dpi_edge_clamp_uses_target_scale_and_egui_zoom() {
        let monitors = mixed_dpi_monitors();
        let zoom_factor = 1.25;
        let view = captured_view(egui::pos2(1760.0, 520.0), 2.5);
        let window = WidgetWindow::new(&view, &monitors, None, zoom_factor);
        // Restore from physical (4400,1300), allowing for a 750x875 physical widget.
        let command_position = take_initial_position(&window.initial_position, 1.25, 0)
            .0
            .unwrap();
        let physical_position = command_position * 1.25;
        assert_eq!(physical_position, egui::pos2(3730.0, 565.0));
        let physical_size =
            widget_size(view.settings.size) * monitors[1].scale_factor * zoom_factor;
        let widget_bounds = egui::Rect::from_min_size(physical_position, physical_size);
        assert_eq!(widget_bounds.max, monitors[1].bounds.max);
        assert!(monitors[1].bounds.contains_rect(widget_bounds));
    }

    #[test]
    fn negative_monitor_origin_survives_capture_and_restoration() {
        let monitors = [Monitor {
            bounds: egui::Rect::from_min_size(
                egui::pos2(-2560.0, -1440.0),
                egui::vec2(2560.0, 1440.0),
            ),
            scale_factor: 2.0,
        }];
        let view = captured_view(egui::pos2(-1200.0, -650.0), 2.0);
        let window = WidgetWindow::new(&view, &monitors, None, 1.0);
        let command_position = take_initial_position(&window.initial_position, 1.0, 0)
            .0
            .unwrap();
        assert_eq!(command_position, egui::pos2(-2400.0, -1300.0));
        assert!(monitors[0].bounds.contains_rect(egui::Rect::from_min_size(
            command_position,
            widget_size(view.settings.size) * monitors[0].scale_factor,
        )));

        let edge_view = captured_view(egui::pos2(-5.0, -5.0), 2.0);
        let edge_window = WidgetWindow::new(&edge_view, &monitors, None, 1.0);
        let edge_position = take_initial_position(&edge_window.initial_position, 2.0, 0)
            .0
            .unwrap()
            * 2.0;
        assert_eq!(edge_position, egui::pos2(-600.0, -700.0));
    }
}
