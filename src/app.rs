use std::{
    collections::VecDeque,
    sync::{Arc, Mutex},
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
};

use crate::visuals;

const BONE: Color32 = Color32::from_rgb(247, 246, 243);
const INK: Color32 = Color32::from_rgb(42, 43, 40);
const MUTED: Color32 = Color32::from_rgb(100, 101, 95);
const FRAME_INTERVAL: Duration = Duration::from_millis(33);
const LOGIC_INTERVAL: Duration = Duration::from_millis(100);
const WIDGET_TITLE_HEIGHT: f32 = 50.0;

enum Command {
    Action(TimerId, TimerAction),
    Settings(TimerId, TimerSettings),
    Style(TimerId, TimerStyle),
    Muted(TimerId, bool),
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
}

impl TimerView {
    fn from_timer(timer: &Timer, now: Instant) -> Self {
        Self {
            id: timer.id,
            settings: timer.settings().clone(),
            snapshot: timer.snapshot(now),
            position: timer.position,
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
        let suggested = view
            .position
            .map(|position| egui::pos2(position.x, position.y))
            .or(root_position);
        let initial_position =
            suggested.map(|position| clamp_position(position, size, monitors, zoom_factor));
        let builder = egui::ViewportBuilder::default()
            .with_title(widget_title(view.id, &view.settings))
            .with_app_id("useful-timer")
            .with_icon(egui::IconData::default())
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

    fn ui(&mut self, ui: &mut egui::Ui) -> bool {
        let mut editing = false;
        ui.horizontal(|ui| {
            for (label, value, maximum) in [
                ("Hours", &mut self.hours, 24),
                ("Minutes", &mut self.minutes, 59),
                ("Seconds", &mut self.seconds, 59),
            ] {
                ui.vertical(|ui| {
                    let label = ui.label(label);
                    let response = ui
                        .add(
                            egui::DragValue::new(value)
                                .range(0..=maximum)
                                .speed(1.0)
                                .update_while_editing(false),
                        )
                        .labelled_by(label.id);
                    editing |= response.changed() || response.has_focus();
                });
            }
        });
        editing
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
    duration_input: TimeInput,
    remaining_input: TimeInput,
    remaining_dirty: bool,
    editor_error: Option<String>,
    persistence_warning: Option<String>,
    operation_warning: Option<String>,
    completions: Vec<CompletionEvent>,
    repaint_ids: Vec<(TimerId, bool)>,
    started: Instant,
}

impl UsefulTimerApp {
    pub fn new(context: &eframe::CreationContext<'_>) -> Self {
        let mut visuals = egui::Visuals::light();
        visuals.panel_fill = BONE;
        visuals.override_text_color = Some(INK);
        visuals.selection.bg_fill = Color32::from_rgb(220, 228, 212);
        visuals.selection.stroke.color = INK;
        context.egui_ctx.set_visuals(visuals);
        context.egui_ctx.global_style_mut(|style| {
            style.spacing.item_spacing = egui::vec2(10.0, 10.0);
            style.spacing.button_padding = egui::vec2(12.0, 7.0);
            style.spacing.interact_size.y = 32.0;
            style
                .text_styles
                .insert(egui::TextStyle::Body, egui::FontId::proportional(16.0));
            style
                .text_styles
                .insert(egui::TextStyle::Button, egui::FontId::proportional(16.0));
            style
                .text_styles
                .insert(egui::TextStyle::Small, egui::FontId::proportional(13.0));
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
        let draft = TimerSettings::default();
        let app = Self {
            shared: Arc::new(Mutex::new(SharedState {
                timers,
                commands: VecDeque::new(),
            })),
            audio: AudioEngine::new(),
            next_id,
            windows: Vec::new(),
            monitors,
            selected: None,
            duration_input: TimeInput::from_duration(draft.duration),
            remaining_input: TimeInput::from_duration(draft.duration),
            remaining_dirty: false,
            draft,
            editor_error: None,
            persistence_warning,
            operation_warning: None,
            completions: Vec::with_capacity(MAX_TIMERS),
            repaint_ids: Vec::with_capacity(MAX_TIMERS),
            started: Instant::now(),
        };
        eprintln!("Useful Timer ready");
        app
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
        self.draft = TimerSettings::default();
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
        ui.add_space(12.0);
        ui.heading("Your timers");
        ui.label(RichText::new(format!("{} / {MAX_TIMERS} open", views.len())).color(MUTED));
        ui.add_space(6.0);
        if ui
            .add_sized(
                [ui.available_width(), 36.0],
                egui::Button::new("+ New timer"),
            )
            .clicked()
        {
            self.new_draft();
        }
        ui.separator();
        egui::ScrollArea::vertical()
            .auto_shrink([false, false])
            .show(ui, |ui| {
                if views.is_empty() {
                    ui.add_space(16.0);
                    ui.label("No timers yet.");
                    ui.label(
                        RichText::new(
                            "Choose a style and duration, then add your first desktop widget.",
                        )
                        .color(MUTED),
                    );
                }
                for view in views {
                    ui.push_id(view.id, |ui| {
                        egui::Frame::new()
                            .fill(if self.selected == Some(view.id) {
                                Color32::from_rgb(229, 232, 223)
                            } else {
                                Color32::from_rgb(255, 254, 252)
                            })
                            .inner_margin(12.0)
                            .show(ui, |ui| {
                                ui.set_min_width((ui.available_width() - 2.0).max(0.0));
                                let label = display_label(&view.settings.label);
                                if ui
                                    .selectable_label(
                                        self.selected == Some(view.id),
                                        RichText::new(label).strong(),
                                    )
                                    .clicked()
                                {
                                    self.select(view);
                                }
                                ui.horizontal_wrapped(|ui| {
                                    ui.label(
                                        RichText::new(view.settings.style.label()).color(MUTED),
                                    );
                                    ui.label(
                                        RichText::new(phase_label(view.snapshot.phase))
                                            .color(MUTED),
                                    );
                                });
                                ui.label(
                                    RichText::new(visuals::format_remaining(
                                        view.snapshot.remaining,
                                    ))
                                    .monospace()
                                    .size(23.0),
                                );
                                ui.horizontal_wrapped(|ui| {
                                    if let Some(action) = timer_controls(ui, view.snapshot.phase) {
                                        queue(
                                            &self.shared,
                                            ui.ctx(),
                                            Command::Action(view.id, action),
                                        );
                                    }
                                });
                                ui.horizontal_wrapped(|ui| {
                                    if ui.small_button("Edit").clicked() {
                                        self.select(view);
                                    }
                                    if ui.small_button("Remove").clicked() {
                                        queue(&self.shared, ui.ctx(), Command::Remove(view.id));
                                    }
                                });
                            });
                        ui.add_space(6.0);
                    });
                }
            });
    }

    fn live_controls(&mut self, ui: &mut egui::Ui, view: &TimerView) {
        ui.label(
            RichText::new("CURRENT COUNTDOWN")
                .small()
                .strong()
                .color(MUTED),
        );
        ui.horizontal_wrapped(|ui| {
            ui.label(RichText::new(phase_label(view.snapshot.phase)).strong());
            ui.label(
                RichText::new(visuals::format_remaining(view.snapshot.remaining))
                    .monospace()
                    .size(24.0),
            );
            if let Some(action) = timer_controls(ui, view.snapshot.phase) {
                queue(&self.shared, ui.ctx(), Command::Action(view.id, action));
                self.remaining_dirty = false;
            }
        });
        if matches!(
            view.snapshot.phase,
            TimerPhase::Running | TimerPhase::Paused
        ) {
            if !self.remaining_dirty {
                self.remaining_input = TimeInput::from_duration(view.snapshot.remaining);
            }
            ui.label("Remaining time");
            ui.push_id("remaining-time", |ui| {
                self.remaining_dirty |= self.remaining_input.ui(ui);
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
                    ("-5 min", -300),
                    ("-1 min", -60),
                    ("+1 min", 60),
                    ("+5 min", 300),
                ] {
                    if ui.button(label).clicked() {
                        queue(
                            &self.shared,
                            ui.ctx(),
                            Command::Action(view.id, TimerAction::AdjustRemaining(seconds)),
                        );
                        self.remaining_dirty = false;
                    }
                }
            });
            ui.label(RichText::new("Live edits keep the timer running or paused. Zero finishes it. Maximum: 24 hours.").small().color(MUTED));
        }
        ui.horizontal_wrapped(|ui| {
            ui.label("Animation");
            for style in TimerStyle::ALL {
                if ui
                    .selectable_label(view.settings.style == style, style.label())
                    .clicked()
                {
                    self.draft.style = style;
                    queue(&self.shared, ui.ctx(), Command::Style(view.id, style));
                }
            }
        });
        let mut muted = view.settings.muted;
        if ui.checkbox(&mut muted, "Mute completion sound").changed() {
            self.draft.muted = muted;
            queue(&self.shared, ui.ctx(), Command::Muted(view.id, muted));
        }
        ui.label(
            RichText::new("Animation and mute changes apply immediately without resetting time.")
                .small()
                .color(MUTED),
        );
        ui.add_space(12.0);
        ui.separator();
    }

    fn editor(&mut self, ui: &mut egui::Ui, views: &[TimerView]) {
        ui.add_space(12.0);
        let heading = match self.selected {
            Some(id) => format!("Edit timer #{id}"),
            None => "New desktop timer".to_owned(),
        };
        ui.heading(heading);
        ui.label(
            RichText::new("Independent. Always on top. Drag the artwork to move.").color(MUTED),
        );
        ui.add_space(12.0);
        egui::ScrollArea::vertical().id_salt(self.selected).auto_shrink([false, false]).show(ui, |ui| {
            if let Some(view) = views.iter().find(|view| Some(view.id) == self.selected) {
                self.live_controls(ui, view);
            }
            ui.label(RichText::new("STYLE").small().strong().color(MUTED));
            let preview_width = ((ui.available_width() - 28.0) / 3.0).clamp(70.0, 150.0);
            let previous_style = self.draft.style;
            ui.horizontal(|ui| {
                for style in TimerStyle::ALL {
                    ui.vertical(|ui| {
                        ui.set_width(preview_width);
                        let (rect, response) = ui.allocate_exact_size(Vec2::splat(preview_width), egui::Sense::click());
                        let mut settings = self.draft.clone();
                        settings.style = style;
                        let preview = TimerSnapshot {
                            phase: TimerPhase::Idle,
                            remaining: settings.duration,
                            remaining_fraction: 1.0,
                            effect_elapsed: None,
                        };
                        visuals::draw_timer(ui.painter(), rect, &settings, &preview, 0.0);
                        if response.clicked() {
                            self.draft.style = style;
                        }
                        if ui.selectable_label(self.draft.style == style, style.label()).clicked() {
                            self.draft.style = style;
                        }
                    });
                }
            });
            if self.draft.style != previous_style && let Some(id) = self.selected {
                queue(&self.shared, ui.ctx(), Command::Style(id, self.draft.style));
            }
            ui.add_space(14.0);
            let value_width = (ui.available_width() - 150.0).clamp(110.0, 240.0);
            egui::Grid::new("timer-settings")
                .num_columns(2)
                .spacing([16.0, 16.0])
                .show(ui, |ui| {
                    let label = ui.label("Label");
                    ui.add(egui::TextEdit::singleline(&mut self.draft.label)
                        .char_limit(80)
                        .desired_width(value_width))
                        .labelled_by(label.id);
                    ui.end_row();

                    ui.label("Duration");
                    ui.push_id("configured-duration", |ui| {
                        self.duration_input.ui(ui);
                    });
                    ui.end_row();

                    let label = ui.label("Widget width");
                    ui.add(egui::Slider::new(&mut self.draft.size, 220.0..=460.0)
                        .suffix(" px")
                        .integer())
                        .labelled_by(label.id);
                    ui.end_row();

                    let label = ui.label("Sound volume");
                    ui.add(egui::Slider::new(&mut self.draft.volume, 0.0..=1.0)
                        .fixed_decimals(2))
                        .labelled_by(label.id)
                        .on_hover_text("0 is silent. Each timer has its own volume.");
                    ui.end_row();
                });
            if self.selected.is_none() {
                ui.checkbox(&mut self.draft.muted, "Mute completion sound");
            }
            ui.add_space(10.0);
            ui.label(RichText::new(format!("Duration: {}", visuals::format_remaining(self.duration_input.duration()))).monospace().color(MUTED));
            ui.label(RichText::new("Changing duration resets this timer to idle. Other edits preserve its countdown.").small().color(MUTED));
            ui.add_space(14.0);
            ui.horizontal_wrapped(|ui| {
                let text = if self.selected.is_some() { "Apply settings" } else { "Add timer" };
                if ui.add(egui::Button::new(RichText::new(text).color(Color32::WHITE)).fill(INK)).clicked() {
                    self.submit_settings(ui.ctx());
                }
                if let Some(id) = self.selected {
                    if ui.button("Restore defaults").on_hover_text("Restore Focus, 25 minutes, Bomb, 300 px, volume 0.60, and idle. Keep this timer's identity and position.").clicked() {
                        queue(&self.shared, ui.ctx(), Command::Action(id, TimerAction::RestoreDefaults));
                        self.draft = TimerSettings::default();
                        self.duration_input = TimeInput::from_duration(self.draft.duration);
                        self.remaining_dirty = false;
                        self.editor_error = None;
                    }
                } else if ui.button("Defaults").clicked() {
                    self.new_draft();
                }
            });
            if let Some(error) = &self.editor_error {
                ui.colored_label(Color32::from_rgb(151, 46, 40), error);
            }
            ui.add_space(20.0);
            ui.separator();
            ui.label(RichText::new("Settings and widget positions are saved locally. Reopening restores every timer idle at its full duration.").small().color(MUTED));
            ui.label(RichText::new("Closing a widget removes that timer. Closing this panel quits all timers.").small().color(MUTED));
        });
    }

    fn register_widgets(&mut self, ctx: &egui::Context, views: &[TimerView]) {
        self.windows
            .retain(|window| views.iter().any(|view| view.id == window.timer_id));
        let root_pixels_per_point = ctx.pixels_per_point();
        let root_position = ctx.input(|input| {
            input.viewport().outer_rect.map(|rect| {
                let position =
                    capture_position(rect.min + egui::vec2(36.0, 60.0), root_pixels_per_point);
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
            let started = self.started;
            ctx.show_viewport_deferred(window.viewport_id, builder, move |ui, _class| {
                widget_ui(ui, id, &shared, &initial_position, started);
            });
        }
    }
}

impl eframe::App for UsefulTimerApp {
    fn logic(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        let now = Instant::now();
        let mut next_tick = None;
        let selection_removed;
        {
            let mut state = self.shared.lock().expect("timer state lock poisoned");
            while let Some(command) = state.commands.pop_front() {
                let id = match &command {
                    Command::Action(id, _)
                    | Command::Settings(id, _)
                    | Command::Style(id, _)
                    | Command::Muted(id, _)
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
                    Command::Style(_, _) | Command::Muted(_, _) => {
                        let mut settings = state.timers[index].settings().clone();
                        match command {
                            Command::Style(_, style) => settings.style = style,
                            Command::Muted(_, muted) => settings.muted = muted,
                            _ => unreachable!(),
                        }
                        state.timers[index]
                            .update_settings(settings, now)
                            .expect("existing settings are valid")
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
            .frame(egui::Frame::new().fill(BONE).inner_margin(20.0))
            .show(ui, |ui| {
                ui.horizontal(|ui| {
                    ui.label(RichText::new("Useful Timer").size(28.0).strong());
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        ui.label(RichText::new("DESKTOP COUNTDOWNS").small().color(MUTED));
                    });
                });
            });
        if self.persistence_warning.is_some()
            || self.operation_warning.is_some()
            || self.audio.warning().is_some()
        {
            egui::Panel::bottom("warnings")
                .frame(
                    egui::Frame::new()
                        .fill(Color32::from_rgb(251, 240, 211))
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
                        ui.label(RichText::new(warning).color(INK));
                    }
                });
        }
        egui::Panel::left("timers")
            .resizable(false)
            .exact_size(260.0)
            .frame(egui::Frame::new().fill(BONE).inner_margin(16.0))
            .show(ui, |ui| self.sidebar(ui, &views));
        egui::CentralPanel::default()
            .frame(egui::Frame::new().fill(BONE).inner_margin(24.0))
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
    }

    fn save(&mut self, storage: &mut dyn eframe::Storage) {
        let saved = {
            let state = self.shared.lock().expect("timer state lock poisoned");
            SavedState::from_timers(&state.timers)
        };
        if let Err(error) = saved.save(storage) {
            self.persistence_warning = Some(format!("Timer settings were not saved: {error}"));
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
    started: Instant,
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
    ui.style_mut().visuals = egui::Visuals::dark();
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
            visuals::draw_timer(
                ui.painter(),
                rect,
                &view.settings,
                &view.snapshot,
                started.elapsed().as_secs_f64(),
            );
            // Keep a 12-point gap to each style's upper silhouette at every widget size.
            let artwork_top = match view.settings.style {
                TimerStyle::Bomb => 34.0, // Full fuse crest, including its thick stroke.
                TimerStyle::Hourglass => 45.0,
                TimerStyle::Rocket => 36.0,
            };
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
                ui.label(RichText::new(label).strong());
                ui.label(format!(
                    "{} · {}",
                    phase_label(view.snapshot.phase),
                    visuals::format_remaining(view.snapshot.remaining)
                ));
                ui.separator();
                if let Some(action) = timer_controls(ui, view.snapshot.phase) {
                    queue(shared, &ctx, Command::Action(id, action));
                    ui.close();
                }
                if ui.button("Remove timer").clicked() {
                    queue(shared, &ctx, Command::Remove(id));
                    ui.close();
                }
            });
            // Start native dragging on the press, not after egui's drag threshold.
            // Title and artwork both drag; controls live in the context menu and panel.
            if interaction.hovered() && ui.input(|input| input.pointer.primary_pressed()) {
                ctx.send_viewport_cmd(ViewportCommand::StartDrag);
            }
        });
    if visuals::needs_animation(view.settings.style, &view.snapshot) {
        // Delay plus egui's frame prediction targets smooth native animation near 60fps.
        ctx.request_repaint_after(FRAME_INTERVAL);
    }
}

fn timer_controls(ui: &mut egui::Ui, phase: TimerPhase) -> Option<TimerAction> {
    let (label, action) = match phase {
        TimerPhase::Idle => ("Start", TimerAction::Start),
        TimerPhase::Running => ("Pause", TimerAction::Pause),
        TimerPhase::Paused => ("Resume", TimerAction::Resume),
        TimerPhase::Finished => ("Done", TimerAction::Start),
    };
    let mut selected = None;
    let button = egui::Button::new(label);
    if ui
        .add_enabled(phase != TimerPhase::Finished, button)
        .clicked()
    {
        selected = Some(action);
    }
    let reset = ui.button("Reset");
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
