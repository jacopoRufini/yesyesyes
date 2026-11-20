//! The settings of the mode "Ask a local LLM": server, model, prompt and timings, the status of
//! the model, and a field to try a request. Calls to the server run on their own thread.

use super::{ASK, ERROR, MATCH, pill};
use crate::core::gate::{self, Decision};
use crate::core::llm::{self, Backend, LlmSettings, LlmState};
use crate::llm_bridge::{self, Answer};
use crate::llm_server;
use crate::memory;
use crate::state::State;
use eframe::egui::{self, RichText};
use std::sync::{Arc, Mutex};

enum Fetch<T> {
    Idle,
    Running,
    Done(Result<T, String>),
}

type Shared<T> = Arc<Mutex<Fetch<T>>>;

/// Half seconds to wait for a server just started.
const STARTING_TRIES: u32 = 60;

pub struct LlmPanel {
    url: String,
    models_dir: String,
    prompt: String,
    model: String,
    try_text: String,
    models: Shared<Vec<String>>,
    /// The server the model list was read from.
    listed: Option<String>,
    answer: Shared<Answer>,
}

impl LlmPanel {
    pub fn new(settings: &LlmSettings) -> Self {
        LlmPanel {
            url: settings.url.clone(),
            models_dir: settings.models_dir.clone(),
            prompt: settings.prompt().to_string(),
            model: settings.model.clone(),
            try_text: String::new(),
            models: Arc::new(Mutex::new(Fetch::Idle)),
            listed: None,
            answer: Arc::new(Mutex::new(Fetch::Idle)),
        }
    }

    pub fn show(&mut self, ui: &mut egui::Ui, state: &State) {
        let (settings, status, memory) = {
            let shared = state.lock();
            (shared.config.gate.llm.clone(), shared.llm.clone(), shared.llm_memory)
        };
        ui.label(
            RichText::new("For now every request is still asked as usual: the answers are only tried here. The model and the requests stay on this computer.")
                .weak()
                .small(),
        );
        let server = settings.url().ok();
        if server.is_some() && self.listed != server {
            self.listed = server.clone();
            refresh_models(&self.models, &settings, ui.ctx());
        }
        egui::Grid::new("llm").num_columns(2).spacing([12.0, 8.0]).show(ui, |ui| {
            self.server_row(ui, state, &settings);
            self.model_row(ui, state, &settings);
            self.prompt_row(ui, state);
            status_row(ui, state, &settings, &status, memory);
            self.try_rows(ui, &settings, status.is_ready());
        });
        ui.add_space(8.0);
        egui::CollapsingHeader::new("Advanced config").id_salt("llm-advanced").show(ui, |ui| {
            egui::Grid::new("llm-advanced-grid").num_columns(2).spacing([12.0, 8.0]).show(ui, |ui| {
                self.address_rows(ui, state, &settings);
                timing_rows(ui, state, &settings);
            });
        });
    }

    fn server_row(&mut self, ui: &mut egui::Ui, state: &State, settings: &LlmSettings) {
        ui.label("Server");
        let mut backend = settings.backend;
        egui::ComboBox::from_id_salt("llm-backend").width(220.0).selected_text(backend.label()).show_ui(ui, |ui| {
            for option in Backend::ALL {
                ui.selectable_value(&mut backend, option, option.label());
            }
        });
        if backend != settings.backend {
            self.url.clear();
            state.update(|c| {
                c.gate.llm.backend = backend;
                c.gate.llm.url.clear();
            });
        }
        ui.end_row();
    }

    fn address_rows(&mut self, ui: &mut egui::Ui, state: &State, settings: &LlmSettings) {
        ui.label("Address");
        ui.vertical(|ui| {
            let field = egui::TextEdit::singleline(&mut self.url).hint_text(settings.backend.default_url()).desired_width(320.0);
            if ui.add(field).lost_focus() {
                let url = self.url.trim().to_string();
                state.update(|c| c.gate.llm.url = url);
            }
            if let Err(e) = settings.url() {
                ui.label(RichText::new(e).color(ERROR).small());
            }
        });
        ui.end_row();

        if settings.backend == Backend::LlamaCpp {
            ui.label("Models folder");
            ui.horizontal(|ui| {
                let field = egui::TextEdit::singleline(&mut self.models_dir).hint_text("llama.cpp cache").desired_width(320.0);
                if ui.add(field).lost_focus() {
                    let dir = self.models_dir.trim().to_string();
                    state.update(|c| c.gate.llm.models_dir = dir);
                }
                ui.label(RichText::new("the GGUF files a llama-server started by Start can load").weak().small());
            });
            ui.end_row();
        }
    }

    fn model_row(&mut self, ui: &mut egui::Ui, state: &State, settings: &LlmSettings) {
        ui.label("Model");
        ui.horizontal(|ui| {
            let listed = match &*self.models.lock().unwrap_or_else(|e| e.into_inner()) {
                Fetch::Done(Ok(names)) if !names.is_empty() => Some(names.clone()),
                _ => None,
            };
            match listed {
                Some(names) => {
                    let mut chosen = settings.model.clone();
                    let shown = if chosen.is_empty() { "Pick a model" } else { chosen.as_str() }.to_string();
                    egui::ComboBox::from_id_salt("llm-model").width(320.0).selected_text(shown).show_ui(ui, |ui| {
                        for name in &names {
                            ui.selectable_value(&mut chosen, name.clone(), name);
                        }
                    });
                    if chosen != settings.model {
                        self.model = chosen.clone();
                        state.update(|c| c.gate.llm.model = chosen);
                    }
                }
                None => {
                    let field = egui::TextEdit::singleline(&mut self.model).hint_text("qwen3.5:4b").desired_width(320.0);
                    if ui.add(field).lost_focus() {
                        let model = self.model.trim().to_string();
                        state.update(|c| c.gate.llm.model = model);
                    }
                }
            }
            let running = matches!(&*self.models.lock().unwrap_or_else(|e| e.into_inner()), Fetch::Running);
            if ui.add_enabled(!running, egui::Button::new("Refresh")).on_hover_text("Read the models of the server again").clicked() {
                refresh_models(&self.models, settings, ui.ctx());
            }
            let error = match &*self.models.lock().unwrap_or_else(|e| e.into_inner()) {
                Fetch::Done(Err(e)) => Some(e.clone()),
                _ => None,
            };
            if let Some(e) = error {
                if e.starts_with("cannot reach the server")
                    && settings.url().is_ok()
                    && ui.button(format!("Start {}", llm_server::name(settings.backend))).on_hover_text("Start it on this computer; yesyesyes stops it when it quits").clicked()
                {
                    start_server(&self.models, settings, ui.ctx());
                }
                ui.label(RichText::new(format!("No list: {e}")).color(ERROR).small());
            } else if running {
                ui.spinner();
            }
        });
        ui.end_row();
    }

    fn prompt_row(&mut self, ui: &mut egui::Ui, state: &State) {
        ui.label("Prompt");
        ui.vertical(|ui| {
            let field = egui::TextEdit::multiline(&mut self.prompt).desired_rows(7).desired_width(f32::INFINITY);
            if ui.add(field).changed() {
                let prompt = if self.prompt.trim() == llm::DEFAULT_PROMPT { String::new() } else { self.prompt.clone() };
                state.update(|c| c.gate.llm.prompt = prompt);
            }
            if ui.add_enabled(self.prompt != llm::DEFAULT_PROMPT, egui::Button::new("Default")).clicked() {
                self.prompt = llm::DEFAULT_PROMPT.to_string();
                state.update(|c| c.gate.llm.prompt.clear());
            }
        });
        ui.end_row();
    }

    fn try_rows(&mut self, ui: &mut egui::Ui, settings: &LlmSettings, ready: bool) {
        let running = matches!(&*self.answer.lock().unwrap_or_else(|e| e.into_inner()), Fetch::Running);
        ui.label("Test with");
        ui.horizontal(|ui| {
            let field = egui::TextEdit::singleline(&mut self.try_text).hint_text("Bash: git status").code_editor().desired_width(ui.available_width() - 170.0);
            let entered = ui.add(field).lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter));
            let can_ask = ready && !running && !self.try_text.trim().is_empty();
            let clicked = ui.add_enabled(can_ask, egui::Button::new(if running { "Asking…" } else { "Ask" })).clicked();
            if can_ask && (clicked || entered) {
                ask(&self.answer, settings, &self.try_text, ui.ctx());
            }
            if let Fetch::Done(Ok(answer)) = &*self.answer.lock().unwrap_or_else(|e| e.into_inner()) {
                let (text, color) = if answer.decision == Decision::Allow { ("Allow", MATCH) } else { ("Ask", ASK) };
                pill(ui, text, color);
            }
        });
        ui.end_row();

        let line = match &*self.answer.lock().unwrap_or_else(|e| e.into_inner()) {
            Fetch::Done(Ok(answer)) => {
                let (tool, text) = split(&self.try_text);
                let mut line = format!("{} ({:.1} s)", answer.reason, answer.took.as_secs_f32());
                if tool == "Bash" && gate::chains(text) {
                    line.push_str(". yesyesyes asks this anyway: the command chains or redirects other commands.");
                }
                Some(RichText::new(line))
            }
            Fetch::Done(Err(e)) => Some(RichText::new(format!("No answer: {e}")).color(ERROR).small()),
            _ => None,
        };
        if let Some(line) = line {
            ui.label("");
            ui.add(egui::Label::new(line).wrap());
            ui.end_row();
        }
    }
}

fn timing_rows(ui: &mut egui::Ui, state: &State, settings: &LlmSettings) {
    ui.label("Keep loaded");
    ui.horizontal(|ui| {
        let mut minutes = settings.keep_minutes;
        if ui.add(egui::DragValue::new(&mut minutes).range(llm::KEEP_MINUTES).suffix(" min")).changed() {
            state.update(|c| c.gate.llm.keep_minutes = minutes);
        }
        let hint = match settings.backend {
            Backend::Ollama => format!(
                "refreshed every {} min while yesyesyes is on; unloaded when you pick another mode or quit",
                settings.refresh_every().as_secs() / 60
            ),
            Backend::LlamaCpp => "llama-server has no timer: yesyesyes unloads it when you pick another mode or quit".to_string(),
        };
        ui.label(RichText::new(hint).weak().small());
    });
    ui.end_row();

    ui.label("Answer within");
    ui.horizontal(|ui| {
        let mut secs = settings.answer_secs;
        if ui.add(egui::DragValue::new(&mut secs).range(llm::ANSWER_SECS).speed(0.1).fixed_decimals(1).suffix(" s")).changed() {
            state.update(|c| c.gate.llm.answer_secs = secs);
        }
        ui.label(RichText::new("slower answers are asked as usual (Claude Code waits 5 s)").weak().small());
    });
    ui.end_row();

    ui.label("Temperature");
    ui.horizontal(|ui| {
        let mut temperature = settings.temperature;
        if ui.add(egui::Slider::new(&mut temperature, 0.0..=1.0).step_by(0.1)).changed() {
            state.update(|c| c.gate.llm.temperature = temperature);
        }
        ui.label(RichText::new("0 gives the same answer to the same request").weak().small());
    });
    ui.end_row();
}

fn status_row(ui: &mut egui::Ui, state: &State, settings: &LlmSettings, status: &LlmState, memory: Option<u64>) {
    ui.label("Status");
    ui.horizontal_wrapped(|ui| {
        let (word, model, details) = status.summary(settings);
        let color = match status {
            LlmState::Ready { .. } => MATCH,
            LlmState::Loading(_) => ASK,
            LlmState::Blocked(_) | LlmState::Failed(_) => ERROR,
            LlmState::Off => ui.visuals().text_color(),
        };
        pill(ui, word, color);
        if matches!(status, LlmState::Loading(_)) {
            ui.spinner();
        }
        if let Some(model) = model {
            ui.label(RichText::new(model).monospace().strong());
        }
        if let (true, Some(bytes)) = (status.is_ready(), memory) {
            memory_pill(ui, bytes);
        }
        if !details.is_empty() {
            ui.label(details);
        }
        if let LlmState::Blocked(others) = status
            && ui.button(format!("Unload {}", others.join(", "))).clicked()
        {
            state.lock().llm_unload_others = true;
        }
    });
    ui.end_row();
}

/// The memory the model takes, green, yellow or red by its share of the computer's memory.
fn memory_pill(ui: &mut egui::Ui, bytes: u64) {
    let Some(total) = memory::total() else {
        pill(ui, &format!("{} memory used", memory::text(bytes)), ui.visuals().text_color());
        return;
    };
    let color = match memory::level(bytes, total) {
        memory::Level::Green => MATCH,
        memory::Level::Yellow => ASK,
        memory::Level::Red => ERROR,
    };
    let hint = format!(
        "{:.0}% of the {} of this computer. Yellow from {:.0}%, red from {:.0}%.",
        bytes as f64 / total as f64 * 100.0,
        memory::text(total),
        memory::YELLOW * 100.0,
        memory::RED * 100.0
    );
    ui.scope(|ui| pill(ui, &format!("{} memory used", memory::text(bytes)), color)).response.on_hover_text(hint);
}

/// `Tool: input` typed in the test field; without a tool, a Bash command.
fn split(typed: &str) -> (&str, &str) {
    match typed.split_once(": ") {
        Some((tool, text)) if !tool.is_empty() && !tool.contains(' ') => (tool, text),
        _ => ("Bash", typed),
    }
}

fn refresh_models(models: &Shared<Vec<String>>, settings: &LlmSettings, ctx: &egui::Context) {
    *models.lock().unwrap_or_else(|e| e.into_inner()) = Fetch::Running;
    let (models, settings, ctx) = (models.clone(), settings.clone(), ctx.clone());
    std::thread::spawn(move || {
        let result = llm_bridge::models(&settings);
        *models.lock().unwrap_or_else(|e| e.into_inner()) = Fetch::Done(result);
        ctx.request_repaint();
    });
}

/// Starts the server, then reads its models once it answers.
fn start_server(models: &Shared<Vec<String>>, settings: &LlmSettings, ctx: &egui::Context) {
    *models.lock().unwrap_or_else(|e| e.into_inner()) = Fetch::Running;
    let (models, settings, ctx) = (models.clone(), settings.clone(), ctx.clone());
    std::thread::spawn(move || {
        let result = llm_server::start(&settings).and_then(|()| {
            let mut last = Err(String::new());
            for _ in 0..STARTING_TRIES {
                std::thread::sleep(std::time::Duration::from_millis(500));
                last = llm_bridge::models(&settings);
                if last.is_ok() {
                    break;
                }
            }
            last
        });
        *models.lock().unwrap_or_else(|e| e.into_inner()) = Fetch::Done(result);
        ctx.request_repaint();
    });
}

fn ask(answer: &Shared<Answer>, settings: &LlmSettings, typed: &str, ctx: &egui::Context) {
    *answer.lock().unwrap_or_else(|e| e.into_inner()) = Fetch::Running;
    let (tool, text) = split(typed.trim());
    let (answer, settings, tool, text, ctx) = (answer.clone(), settings.clone(), tool.to_string(), text.to_string(), ctx.clone());
    std::thread::spawn(move || {
        let result = llm_bridge::ask(&settings, &tool, &text);
        *answer.lock().unwrap_or_else(|e| e.into_inner()) = Fetch::Done(result);
        ctx.request_repaint();
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn splits_the_typed_request() {
        assert_eq!(split("Bash: git status"), ("Bash", "git status"));
        assert_eq!(split(r#"Read: {"file_path": "a"}"#), ("Read", r#"{"file_path": "a"}"#));
        assert_eq!(split("git status"), ("Bash", "git status"));
        assert_eq!(split("echo a: b"), ("Bash", "echo a: b"), "a command with a colon");
    }
}
