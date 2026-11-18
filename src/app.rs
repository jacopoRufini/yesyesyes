//! The rules and log window (egui). Closing it only hides it; the app lives in the tray.

use crate::backend::{Access, Backend, Platform};
use crate::core::{AddResult, ClickEvent, ClickResult, Engine, LogSettings, Rule, clicklog};
use crate::login;
use crate::state::State;
use crate::tray::Tray;
use eframe::egui::{self, Color32, RichText, ViewportCommand};
use std::time::{Duration, Instant};

const TICK: Duration = Duration::from_secs(1);
const COOLDOWN: Duration = Duration::from_secs(3);

pub struct App {
    state: State,
    tray: Option<Tray>,
    new_app: String,
    new_action: String,
    form_error: Option<String>,
    /// The one recent click whose whole text is shown.
    expanded: Option<u64>,
    log_path: String,
}

impl App {
    pub fn new(cc: &eframe::CreationContext<'_>, state: State) -> Self {
        apply_text_sizes(&cc.egui_ctx);
        start_engine(state.clone(), cc.egui_ctx.clone());
        let tray = match Tray::new(state.clone(), cc.egui_ctx.clone()) {
            Ok(tray) => Some(tray),
            Err(e) => {
                // Without a tray (e.g. GNOME without the AppIndicator extension) the window stays open.
                state.lock().error = Some(format!("No tray icon ({e}): closing this window quits yesyesyes."));
                state.lock().show_window = true;
                None
            }
        };
        let log_path = state.lock().config.log.path.clone().unwrap_or_default();
        App { state, tray, new_app: String::new(), new_action: String::new(), form_error: None, expanded: None, log_path }
    }

    /// Off by default. The path field shows the default of this OS until the user types another.
    fn log_file_options(&mut self, ui: &mut egui::Ui, log: &LogSettings) {
        let mut save = log.save;
        if ui.checkbox(&mut save, "Save the log to a file").changed() {
            self.state.update(|c| c.log.save = save);
        }
        if !save {
            return;
        }
        ui.horizontal(|ui| {
            let default = clicklog::default_path().display().to_string();
            let field = egui::TextEdit::singleline(&mut self.log_path).hint_text(default).desired_width(ui.available_width() - 90.0);
            let response = ui.add(field);
            if response.lost_focus() {
                let typed = self.log_path.trim().to_string();
                self.state.update(|c| c.log.path = (!typed.is_empty()).then_some(typed));
            }
            if ui.add_enabled(!self.log_path.is_empty(), egui::Button::new("Default")).clicked() {
                self.log_path.clear();
                self.state.update(|c| c.log.path = None);
            }
        });
        ui.label(
            RichText::new("One line per click, notification text included: it may hold commands, paths or secrets. Readable by you only; check it before sharing.")
                .weak()
                .small(),
        );
    }

    fn add_rule(&mut self) {
        let rule = Rule::new(&self.new_app, &self.new_action);
        let mut result = AddResult::Invalid;
        self.state.update(|c| result = c.add(rule));
        self.form_error = match result {
            AddResult::Added => {
                self.new_app.clear();
                self.new_action.clear();
                None
            }
            AddResult::Invalid => Some("App name and button label are both required.".into()),
            AddResult::Duplicate => Some("That rule already exists.".into()),
        };
    }
}

/// The engine runs on its own thread: the backend is created there (Windows needs COM on that thread).
fn start_engine(state: State, ctx: egui::Context) {
    std::thread::spawn(move || {
        let platform = Platform::new();
        let mut engine = Engine::new(COOLDOWN);
        let mut prompt = true;
        loop {
            let access = platform.access(prompt);
            prompt = false;
            let config = {
                let mut shared = state.lock();
                if shared.access != access {
                    shared.access = access.clone();
                    ctx.request_repaint();
                }
                shared.config.clone()
            };
            if access == Access::Ready {
                let events = engine.tick(&config, &platform.roots(), Instant::now());
                if !events.is_empty() {
                    state.record(events);
                    ctx.request_repaint();
                }
            }
            std::thread::sleep(TICK);
        }
    });
}

impl eframe::App for App {
    fn logic(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        if ctx.input(|i| i.viewport().close_requested()) && self.tray.is_some() {
            ctx.send_viewport_cmd(ViewportCommand::CancelClose);
            ctx.send_viewport_cmd(ViewportCommand::Visible(false));
        }
        let (show, enabled) = {
            let mut shared = self.state.lock();
            (std::mem::take(&mut shared.show_window), shared.config.enabled)
        };
        if show {
            ctx.send_viewport_cmd(ViewportCommand::Visible(true));
            ctx.send_viewport_cmd(ViewportCommand::Focus);
            #[cfg(target_os = "macos")]
            crate::macos_app::activate();
        }
        if let Some(tray) = &self.tray {
            tray.sync(enabled);
        }
    }

    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        let (access, config, events, error) = {
            let shared = self.state.lock();
            (shared.access.clone(), shared.config.clone(), shared.events.clone(), shared.error.clone())
        };
        egui::CentralPanel::default().show(ui, |ui| {
            ui.spacing_mut().item_spacing = egui::vec2(8.0, 8.0);
            for message in [access_hint(&access), error].into_iter().flatten() {
                ui.colored_label(ERROR, message);
            }

            ui.horizontal(|ui| {
                let mut active = config.enabled;
                if ui.checkbox(&mut active, "Active").changed() {
                    self.state.update(|c| c.enabled = active);
                }
                ui.add_space(16.0);
                let mut at_login = login::is_enabled();
                if ui.checkbox(&mut at_login, "Launch at login").changed() {
                    self.state.lock().error = login::set(at_login).err().map(|e| format!("Could not change Launch at Login: {e}"));
                }
            });

            ui.add_space(4.0);
            ui.heading("Rules");
            ui.label(RichText::new("When a notification mentions the app, press the button with that exact label.").weak());
            section(ui, |ui| {
                egui::ScrollArea::vertical().id_salt("rules").max_height(170.0).show(ui, |ui| {
                    egui::Grid::new("rules-grid").num_columns(3).striped(true).min_col_width(150.0).spacing([16.0, 8.0]).show(ui, |ui| {
                        ui.label(RichText::new("App").strong());
                        ui.label(RichText::new("Button").strong());
                        ui.label("");
                        ui.end_row();
                        for rule in &config.rules {
                            ui.label(&rule.app);
                            ui.label(&rule.action);
                            if ui.button("Remove").clicked() {
                                let id = rule.id();
                                self.state.update(|c| c.rules.retain(|r| r.id() != id));
                            }
                            ui.end_row();
                        }
                    });
                    if config.rules.is_empty() {
                        ui.label(RichText::new("No rules yet: add one below.").weak());
                    }
                });
                ui.separator();
                ui.horizontal(|ui| {
                    ui.add(egui::TextEdit::singleline(&mut self.new_app).hint_text("App, e.g. Claude").desired_width(150.0));
                    ui.add(egui::TextEdit::singleline(&mut self.new_action).hint_text("Button, e.g. Allow once").desired_width(180.0));
                    if ui.button("Add rule").clicked() {
                        self.add_rule();
                    }
                });
                if let Some(error) = &self.form_error {
                    ui.colored_label(ERROR, error);
                }
            });

            ui.add_space(4.0);
            ui.heading("Recent clicks");
            let kept = if config.log.save { "also saved to the log file below" } else { "it stays in memory only, until you quit" };
            ui.label(RichText::new(format!("The last 50 matches, newest first (ignored: seen while paused). Click a line to see the whole text; {kept}.")).weak());
            self.log_file_options(ui, &config.log);
            section(ui, |ui| {
                egui::ScrollArea::vertical().id_salt("log").show(ui, |ui| {
                    if events.is_empty() {
                        ui.label(RichText::new("Nothing pressed yet.").weak());
                        return;
                    }
                    for (index, event) in events.iter().enumerate() {
                        if index > 0 {
                            ui.separator();
                        }
                        log_row(ui, event, &mut self.expanded);
                    }
                });
            });
        });
    }
}

/// Time, app, button and result; under them the notification text, cut to one line until clicked.
fn log_row(ui: &mut egui::Ui, event: &ClickEvent, expanded: &mut Option<u64>) {
    ui.horizontal(|ui| {
        ui.label(RichText::new(event.at.format("%H:%M:%S").to_string()).monospace());
        ui.label(RichText::new(&event.app).strong());
        ui.label(&event.action);
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            match event.result {
                ClickResult::Failed => ui.colored_label(ERROR, "failed"),
                ClickResult::Ignored => ui.label(RichText::new("ignored").italics().weak()),
                ClickResult::Pressed => ui.label(RichText::new("pressed").weak()),
            };
        });
    });
    let snippet = event.snippet();
    if snippet.is_empty() {
        return;
    }
    let open = *expanded == Some(event.id);
    let marker = if open { "Hide" } else { "Show" };
    let line = RichText::new(format!("{snippet}   {marker}")).monospace().weak();
    let clicked = ui.add(egui::Label::new(line).truncate().sense(egui::Sense::click())).on_hover_cursor(egui::CursorIcon::PointingHand).clicked();
    if clicked {
        *expanded = if open { None } else { Some(event.id) };
    }
    if open {
        let mut text = event.full_text();
        egui::Frame::new().fill(ui.visuals().extreme_bg_color).inner_margin(8.0).corner_radius(4.0).show(ui, |ui| {
            ui.set_width(ui.available_width());
            ui.add(egui::TextEdit::multiline(&mut text).code_editor().desired_rows(1).desired_width(f32::INFINITY).interactive(true));
            if ui.button("Copy").clicked() {
                ui.ctx().copy_text(event.full_text());
            }
        });
    }
}

const ERROR: Color32 = Color32::from_rgb(210, 70, 50);

fn access_hint(access: &Access) -> Option<String> {
    match access {
        Access::Missing(hint) => Some(hint.clone()),
        _ => None,
    }
}

fn section(ui: &mut egui::Ui, contents: impl FnOnce(&mut egui::Ui)) {
    egui::Frame::group(ui.style()).inner_margin(10.0).show(ui, |ui| {
        ui.set_width(ui.available_width());
        contents(ui);
    });
}

/// One scale for the whole window: body and buttons alike, headings a step up, small a step down.
pub fn apply_text_sizes(ctx: &egui::Context) {
    use egui::{FontFamily::Monospace, FontFamily::Proportional, FontId, TextStyle};
    ctx.all_styles_mut(|style| {
        style.text_styles = [
            (TextStyle::Heading, FontId::new(19.0, Proportional)),
            (TextStyle::Body, FontId::new(14.0, Proportional)),
            (TextStyle::Button, FontId::new(14.0, Proportional)),
            (TextStyle::Small, FontId::new(12.0, Proportional)),
            (TextStyle::Monospace, FontId::new(13.0, Monospace)),
        ]
        .into();
    });
}
