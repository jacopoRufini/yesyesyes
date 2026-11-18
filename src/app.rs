//! The rules and log window (egui). Closing it only hides it; the app lives in the tray.

use crate::backend::{Access, Backend, Platform};
use crate::core::gate::{self, Decision, Mode};
use crate::core::{AddResult, Channel, ClickEvent, ClickResult, Engine, LogSettings, Rule, clicklog};
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
    gate_regex: String,
    /// A request typed to try the regex, never saved.
    gate_try: String,
}

impl App {
    pub fn new(cc: &eframe::CreationContext<'_>, state: State) -> Self {
        apply_text_sizes(&cc.egui_ctx);
        start_engine(state.clone(), cc.egui_ctx.clone());
        crate::gate_server::start(state.clone(), cc.egui_ctx.clone());
        let tray = match Tray::new(state.clone(), cc.egui_ctx.clone()) {
            Ok(tray) => Some(tray),
            Err(e) => {
                // Without a tray (e.g. GNOME without the AppIndicator extension) the window stays open.
                eprintln!("yesyesyes: no tray icon: {e}");
                state.lock().error = Some(format!("No tray icon ({e}): closing this window quits yesyesyes."));
                state.lock().show_window = true;
                None
            }
        };
        let (log_path, gate_regex) = {
            let config = &state.lock().config;
            (config.log.path.clone().unwrap_or_default(), config.gate.regex.clone())
        };
        App { state, tray, new_app: String::new(), new_action: String::new(), form_error: None, expanded: None, log_path, gate_regex, gate_try: String::new() }
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
            let field = egui::TextEdit::singleline(&mut self.log_path).hint_text(default).desired_width(ui.available_width() - 170.0);
            let response = ui.add(field);
            if response.lost_focus() {
                let typed = self.log_path.trim().to_string();
                self.state.update(|c| c.log.path = (!typed.is_empty()).then_some(typed));
            }
            if ui.button("Choose…").clicked()
                && let Some(path) = choose_log_file(&self.log_path)
            {
                self.log_path = path.display().to_string();
                let chosen = self.log_path.clone();
                self.state.update(|c| c.log.path = Some(chosen));
            }
            if ui.add_enabled(!self.log_path.is_empty(), egui::Button::new("Default")).clicked() {
                self.log_path.clear();
                self.state.update(|c| c.log.path = None);
            }
        });
        ui.label(
            RichText::new("One line per click, with what Claude asked: it may hold commands, paths or secrets. Readable by you only; check it before sharing.")
                .weak()
                .small(),
        );
    }

    /// What yesyesyes answers to Claude Code's permission requests, through the hook.
    fn gate_options(&mut self, ui: &mut egui::Ui, mode: Mode) {
        let mut chosen = mode;
        egui::ComboBox::from_id_salt("gate-mode").width(320.0).selected_text(mode.label()).show_ui(ui, |ui| {
            for option in Mode::ALL {
                ui.selectable_value(&mut chosen, option, option.label());
            }
            for later in ["Allow what matches a wildcard (coming later)", "Ask an LLM (coming later)"] {
                ui.add_enabled(false, egui::Button::selectable(false, later));
            }
        });
        if chosen != mode {
            self.state.update(|c| c.gate.mode = chosen);
        }
        match chosen {
            Mode::Ignore => {}
            Mode::Allow => {
                ui.colored_label(ERROR, "Every request runs without asking you: any command, file edit or tool Claude wants.");
            }
            Mode::Deny => {
                ui.label(RichText::new("Claude is told each request was denied by yesyesyes.").weak());
            }
            Mode::Regex => {
                let mut settings = self.state.lock().config.gate.clone();
                settings.regex = self.gate_regex.clone();
                let verdict = (!self.gate_try.is_empty()).then(|| gate::try_regex(&settings, &self.gate_try));
                egui::Grid::new("gate-regex").num_columns(2).spacing([12.0, 6.0]).show(ui, |ui| {
                    ui.label("Regex");
                    let field = egui::TextEdit::singleline(&mut self.gate_regex).hint_text("Bash: git (status|diff)( .*)?").code_editor().desired_width(f32::INFINITY);
                    if ui.add(field).changed() {
                        let regex = self.gate_regex.clone();
                        self.state.update(|c| c.gate.regex = regex);
                    }
                    ui.end_row();

                    ui.label("");
                    let mut ignore_case = settings.ignore_case;
                    if ui.checkbox(&mut ignore_case, "Ignore upper and lower case").changed() {
                        self.state.update(|c| c.gate.ignore_case = ignore_case);
                    }
                    ui.end_row();

                    ui.label("");
                    let help = match gate::compile(&self.gate_regex, settings.ignore_case) {
                        Err(_) => RichText::new("Not a valid regex: every request is asked.").color(ERROR).small(),
                        Ok(_) => RichText::new("Matched against the whole Tool: input, e.g. Bash: git status. Bash commands with ; & | ` $( > < or a new line are always asked.").weak().small(),
                    };
                    ui.add(egui::Label::new(help).wrap());
                    ui.end_row();

                    ui.label("Test with");
                    ui.horizontal(|ui| {
                        let width = ui.available_width() - if verdict.is_some() { 100.0 } else { 0.0 };
                        ui.add(egui::TextEdit::singleline(&mut self.gate_try).hint_text("Bash: git status").code_editor().desired_width(width));
                        if let Some((decision, _)) = &verdict {
                            let (text, color) = if *decision == Decision::Allow { ("Match", MATCH) } else { ("No match", ERROR) };
                            pill(ui, text, color);
                        }
                    });
                    ui.end_row();

                    if let Some((decision, reason)) = &verdict {
                        ui.label("");
                        let text = if *decision == Decision::Allow { "Allowed without asking you.".to_string() } else { format!("Asked: {reason}.") };
                        ui.label(RichText::new(text).small().weak());
                        ui.end_row();
                    }
                });
            }
        }
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
            if access == Access::Ready && config.channel == Channel::Notifications {
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
            egui::ScrollArea::vertical().id_salt("window").auto_shrink([false, false]).show(ui, |ui| {
                ui.spacing_mut().item_spacing = egui::vec2(8.0, 8.0);
                for message in [access_hint(&access), error].into_iter().flatten() {
                    ui.colored_label(ERROR, message);
                }

                ui.horizontal(|ui| {
                    ui.label("Answer Claude through");
                    channel_picker(ui, &self.state, &config);
                    ui.add_space(16.0);
                    let mut at_login = login::is_enabled();
                    if ui.checkbox(&mut at_login, "Launch at login").changed() {
                        self.state.lock().error = login::set(at_login).err().map(|e| format!("Could not change Launch at Login: {e}"));
                    }
                });

                let channel = config.enabled.then_some(config.channel);
                if channel.is_none() {
                    ui.label(RichText::new("Off: nothing is pressed or answered; matches are logged as ignored. Pick a channel to set it up.").weak());
                }
                if channel == Some(Channel::Notifications) {
                    ui.add_space(4.0);
                    ui.heading("Notifications");
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
                }

                if channel == Some(Channel::ClaudeCode) {
                    ui.add_space(4.0);
                    ui.heading("Claude Code requests");
                    ui.label(RichText::new("Answered before any prompt or notification, through the hook in the README.").weak());
                    section(ui, |ui| self.gate_options(ui, config.gate.mode));
                }

                ui.add_space(4.0);
                ui.heading("Recent clicks");
                let kept = if config.log.save { "also saved to the log file below" } else { "it stays in memory only, until you quit" };
                ui.label(RichText::new(format!("The last 100, newest first (ignored: seen while off, or left to you). Click a line to see what Claude asked, whole; {kept}.")).weak());
                self.log_file_options(ui, &config.log);
                section(ui, |ui| {
                    if events.is_empty() {
                        ui.label(RichText::new("Nothing pressed yet.").weak());
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

/// Off, or one of the two channels. Off keeps the channel, for the tray's Start.
fn channel_picker(ui: &mut egui::Ui, state: &State, config: &crate::core::Config) {
    const OFF: Option<Channel> = None;
    let current = config.enabled.then_some(config.channel);
    let label = |choice: Option<Channel>| match choice {
        None => "Off",
        Some(Channel::Notifications) => "Notifications",
        Some(Channel::ClaudeCode) => "Claude Code requests",
    };
    let mut chosen = current;
    egui::ComboBox::from_id_salt("channel").selected_text(label(current)).show_ui(ui, |ui| {
        for choice in [OFF, Some(Channel::Notifications), Some(Channel::ClaudeCode)] {
            ui.selectable_value(&mut chosen, choice, label(choice));
        }
    });
    if chosen != current {
        state.update(|c| match chosen {
            None => c.enabled = false,
            Some(channel) => {
                c.enabled = true;
                c.channel = channel;
            }
        });
    }
}

/// A save dialog: the file may exist, lines are added at its end.
fn choose_log_file(current: &str) -> Option<std::path::PathBuf> {
    let current = clicklog::resolve(Some(current));
    let mut dialog = rfd::FileDialog::new().set_title("Log file").add_filter("JSON lines", &["jsonl"]);
    if let Some(dir) = current.parent().filter(|d| d.is_dir()) {
        dialog = dialog.set_directory(dir);
    }
    if let Some(name) = current.file_name() {
        dialog = dialog.set_file_name(name.to_string_lossy());
    }
    dialog.save_file()
}

/// Time, app, button and result; under them what Claude asked (else the notification text),
/// cut to one line. A click anywhere on the row shows all of it, or hides it again.
fn log_row(ui: &mut egui::Ui, event: &ClickEvent, expanded: &mut Option<u64>) {
    let open = *expanded == Some(event.id);
    let summary = ui.vertical(|ui| {
        ui.horizontal(|ui| {
            ui.label(RichText::new(event.at.format("%H:%M:%S").to_string()).monospace());
            ui.label(RichText::new(&event.app).strong());
            ui.label(&event.action);
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                match event.result {
                    ClickResult::Failed => ui.colored_label(ERROR, "failed"),
                    ClickResult::Ignored => ui.label(RichText::new("ignored").italics().weak()),
                    ClickResult::Pressed => ui.label(RichText::new("pressed").weak()),
                    ClickResult::Allowed => ui.label(RichText::new("allowed").weak()),
                    ClickResult::Denied => ui.colored_label(ERROR, "denied"),
                };
            });
        });
        let snippet = event.snippet();
        if !snippet.is_empty() && !open {
            ui.add(egui::Label::new(RichText::new(snippet).monospace().weak()).truncate().selectable(false));
        }
    });
    let rect = summary.response.rect.expand(2.0);
    let row = ui.interact(rect, ui.make_persistent_id(("log-row", event.id)), egui::Sense::click()).on_hover_cursor(egui::CursorIcon::PointingHand);
    if row.hovered() {
        ui.painter().rect_filled(rect, 4.0, ui.visuals().widgets.hovered.weak_bg_fill.gamma_multiply(0.4));
    }
    if row.clicked() {
        *expanded = if open { None } else { Some(event.id) };
    }
    if open {
        egui::Frame::new().fill(ui.visuals().extreme_bg_color).inner_margin(8.0).corner_radius(4.0).show(ui, |ui| {
            ui.set_width(ui.available_width());
            text_block(ui, &event.shown_text());
        });
    }
}

/// Selectable text and a button that copies it.
fn text_block(ui: &mut egui::Ui, text: &str) {
    let mut shown = text.to_string();
    ui.add(egui::TextEdit::multiline(&mut shown).code_editor().desired_rows(1).desired_width(f32::INFINITY).interactive(true));
    if ui.button("Copy").clicked() {
        ui.ctx().copy_text(text.to_string());
    }
}

const ERROR: Color32 = Color32::from_rgb(210, 70, 50);
const MATCH: Color32 = Color32::from_rgb(46, 160, 67);

fn access_hint(access: &Access) -> Option<String> {
    match access {
        Access::Missing(hint) => Some(hint.clone()),
        _ => None,
    }
}

/// A short colored label on a tinted, rounded background.
fn pill(ui: &mut egui::Ui, text: &str, color: Color32) {
    egui::Frame::new().fill(color.gamma_multiply(0.2)).corner_radius(10.0).inner_margin(egui::Margin::symmetric(10, 3)).show(ui, |ui| {
        ui.label(RichText::new(text).color(color).strong());
    });
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
