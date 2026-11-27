#[path = "discord-client/appearance.rs"]
mod appearance;
use appearance::*;
#[path = "discord-client/example_view.rs"]
mod example_view;
use discord_text::{Channel, GUILD, Message, Session};
use eframe::egui;
use reqwest::blocking::{Client, RequestBuilder};
use serde::de::DeserializeOwned;
use serde_json::json;
use std::{
    collections::HashMap,
    sync::mpsc,
    thread,
    time::{Duration, Instant},
};

enum Request {
    Login(String),
    History(String, Option<String>),
    CreateChannel(String),
    Send(String, String),
    TestCommand(String),
    Edit(String, String, String),
    Delete(String, String),
}
enum Reply {
    Login(Session, Vec<Channel>),
    History(String, Vec<Message>, Vec<Channel>),
    ChannelCreated(Channel),
    Saved(String),
    CommandPreview(String, String),
}
type Result<T> = std::result::Result<T, String>;
fn response(req: RequestBuilder) -> Result<reqwest::blocking::Response> {
    let r = req
        .send()
        .map_err(|e| format!("No se pudo contactar con el servidor local. {e}"))?;
    if r.status().is_success() {
        Ok(r)
    } else {
        let status = r.status();
        Err(format!("{status} · {}", r.text().unwrap_or_default()))
    }
}
fn read<T: DeserializeOwned>(req: RequestBuilder) -> Result<T> {
    response(req)?.json().map_err(|e| e.to_string())
}
#[derive(serde::Deserialize)]
struct CommandCallback {
    data: CommandCallbackData,
}
#[derive(serde::Deserialize)]
struct CommandCallbackData {
    components: [CommandText; 1],
}
#[derive(serde::Deserialize)]
struct CommandText {
    content: String,
}
struct Api {
    client: Client,
    root: String,
    token: String,
}
impl Api {
    fn run(&mut self, r: Request) -> Result<Reply> {
        let api = format!("{}/api/v10", self.root);
        let messages = |c: &str| format!("{api}/channels/{c}/messages");
        let channels_path = format!("{api}/guilds/{GUILD}/channels");
        match r {
            Request::Login(name) => {
                let session: Session = read(
                    self.client
                        .post(format!("{}/local/sessions", self.root))
                        .json(&json!({"username":name})),
                )?;
                self.token = session.token.clone();
                let channels = read(self.client.get(&channels_path).bearer_auth(&self.token))?;
                Ok(Reply::Login(session, channels))
            }
            Request::History(c, before) => {
                let mut request = self.client.get(messages(&c)).bearer_auth(&self.token);
                if let Some(id) = before {
                    request = request.query(&[("before", id)]);
                }
                Ok(Reply::History(
                    c,
                    read(request)?,
                    read(self.client.get(&channels_path).bearer_auth(&self.token))?,
                ))
            }
            Request::CreateChannel(name) => {
                let channel = read(
                    self.client
                        .post(&channels_path)
                        .bearer_auth(&self.token)
                        .json(&json!({"name":name,"type":0})),
                )?;
                Ok(Reply::ChannelCreated(channel))
            }
            Request::TestCommand(c) => {
                let callback: CommandCallback = read(
                    self.client
                        .post(format!("{}/local/channels/{c}/interactions", self.root))
                        .bearer_auth(&self.token)
                        .json(&json!({"type":2,"data":{"name":"test"},"locale":"es-ES"})),
                )?;
                let [component] = callback.data.components;
                Ok(Reply::CommandPreview(c, component.content))
            }
            Request::Send(c, text) => {
                let _: Message = read(
                    self.client
                        .post(messages(&c))
                        .bearer_auth(&self.token)
                        .json(&json!({"content":text})),
                )?;
                Ok(Reply::Saved(c))
            }
            Request::Edit(c, id, text) => {
                let _: Message = read(
                    self.client
                        .patch(format!("{}/{id}", messages(&c)))
                        .bearer_auth(&self.token)
                        .json(&json!({"content":text})),
                )?;
                Ok(Reply::Saved(c))
            }
            Request::Delete(c, id) => {
                response(
                    self.client
                        .delete(format!("{}/{id}", messages(&c)))
                        .bearer_auth(&self.token),
                )?;
                Ok(Reply::Saved(c))
            }
        }
    }
}
struct App {
    tx: mpsc::Sender<Request>,
    rx: mpsc::Receiver<Result<Reply>>,
    busy: bool,
    next: Instant,
    retry: discord_text::backoff::Backoff,
    background: egui::Color32,
    new_channel: Option<String>,
    name: String,
    user: String,
    channels: Vec<Channel>,
    channel: String,
    messages: Vec<Message>,
    pages: Vec<String>,
    drafts: HashMap<String, String>,
    editing: Option<(String, String)>,
    deleting: Option<String>,
    error: String,
    clear_sent: Option<(String, String)>,
    command_preview: Option<String>,
    timeline_height: f32,
}
impl App {
    fn new(ctx: egui::Context, root: String, background: egui::Color32) -> Self {
        let (tx, requests) = mpsc::channel();
        let (results, rx) = mpsc::channel();
        thread::spawn(move || {
            let mut api = Api {
                root,
                token: String::new(),
                client: Client::builder()
                    .no_proxy()
                    .timeout(Duration::from_secs(4))
                    .build()
                    .unwrap(),
            };
            for req in requests {
                if results.send(api.run(req)).is_err() {
                    break;
                }
                ctx.request_repaint();
            }
        });
        Self {
            tx,
            rx,
            busy: false,
            next: Instant::now(),
            retry: Default::default(),
            background,
            new_channel: None,
            name: String::new(),
            user: String::new(),
            channels: vec![],
            channel: String::new(),
            messages: vec![],
            pages: Vec::new(),
            drafts: HashMap::new(),
            editing: None,
            deleting: None,
            error: String::new(),
            clear_sent: None,
            command_preview: None,
            timeline_height: 0.,
        }
    }
    fn send(&mut self, r: Request) {
        self.busy = self.tx.send(r).is_ok()
    }
    fn select_channel(&mut self, channel: String) {
        self.channel = channel;
        self.messages.clear();
        self.pages.clear();
        self.editing = None;
        self.deleting = None;
        self.command_preview = None;
        self.timeline_height = 0.;
        self.next = Instant::now();
    }
    fn clear_draft(&mut self, sent: Option<(String, String)>) {
        if let Some((key, text)) = sent
            && self.drafts.get(&key) == Some(&text)
        {
            self.drafts.remove(&key);
        }
    }
    fn receive(&mut self) {
        let Ok(result) = self.rx.try_recv() else {
            return;
        };
        self.busy = false;
        if result.is_ok() {
            self.retry.success();
        }
        let sent = self.clear_sent.take();
        self.next = Instant::now() + Duration::from_secs(2);
        match result {
            Ok(Reply::Login(s, channels)) => {
                self.user = s.user.id;
                self.channel = channels[0].id.clone();
                self.channels = channels;
                self.error.clear();
                self.next = Instant::now();
            }
            Ok(Reply::ChannelCreated(channel)) => {
                self.select_channel(channel.id.clone());
                self.channels.push(channel);
                self.new_channel = None;
                self.error.clear();
            }
            Ok(Reply::History(c, mut messages, channels)) => {
                self.channels = channels;
                if c == self.channel {
                    messages.reverse();
                    self.messages = messages;
                }
            }
            Ok(Reply::CommandPreview(c, text)) => {
                self.clear_draft(sent);
                if c == self.channel {
                    self.command_preview = Some(text);
                    if !self.pages.is_empty() {
                        self.pages.clear();
                        self.messages.clear();
                        self.timeline_height = 0.;
                        self.next = Instant::now();
                    }
                }
                self.error.clear();
            }
            Ok(Reply::Saved(c)) => {
                self.clear_draft(sent);
                self.editing = None;
                self.deleting = None;
                self.error.clear();
                if self.channel == c {
                    self.next = Instant::now();
                }
            }
            Err(e) => {
                self.next = Instant::now() + self.retry.failure();
                self.error = e;
            }
        }
    }
}
impl eframe::App for App {
    fn update(&mut self, ctx: &egui::Context, _: &mut eframe::Frame) {
        self.receive();
        ctx.request_repaint_after(Duration::from_millis(200));
        if !self.error.is_empty() {
            egui::TopBottomPanel::top("status").show(ctx, |ui| {
                ui.horizontal_wrapped(|ui| {
                    ui.colored_label(egui::Color32::LIGHT_RED, &self.error);
                    if ui.small_button("Cerrar").clicked() {
                        self.error.clear();
                    }
                });
            });
        }
        if self.user.is_empty() {
            egui::CentralPanel::default()
                .frame(surface(self.background, 16., 16.))
                .show(ctx, |_| {});
            egui::Area::new(egui::Id::new("sign_in"))
                .anchor(egui::Align2::CENTER_CENTER, egui::Vec2::ZERO)
                .movable(false)
                .show(ctx, |ui| {
                    surface(SIDEBAR, 28., 24.)
                        .rounding(12.)
                        .stroke(egui::Stroke::new(1_f32, BORDER))
                        .show(ui, |ui| {
                            ui.set_width(360.);
                            ui.vertical_centered(|ui| {
                                avatar(ui, "D", GUILD, 44.);
                                ui.heading("Bienvenido");
                                ui.label(muted("Entra en tu servidor local"));
                            });
                            ui.add_space(18.);
                            ui.label(muted("Nombre para esta sesión"));
                            let field = ui.add(
                                egui::TextEdit::singleline(&mut self.name)
                                    .desired_width(f32::INFINITY)
                                    .margin(egui::vec2(12., 9.)),
                            );
                            let ready = !self.busy && !self.name.trim().is_empty();
                            if ui
                                .add_enabled(
                                    ready,
                                    egui::Button::new("Entrar")
                                        .fill(ACCENT)
                                        .min_size(egui::vec2(ui.available_width(), 36.)),
                                )
                                .clicked()
                                || ready
                                    && field.lost_focus()
                                    && ui.input(|i| i.key_pressed(egui::Key::Enter))
                            {
                                self.send(Request::Login(self.name.clone()));
                            }
                        });
                });
            return;
        }

        let mut action = None;
        egui::SidePanel::left("servers")
            .exact_width(66.)
            .resizable(false)
            .show_separator_line(false)
            .frame(surface(self.background, 8., 12.))
            .show(ctx, |ui| {
                ui.vertical_centered(|ui| {
                    ui.add(
                        egui::Button::new(egui::RichText::new("D").size(22.).strong())
                            .fill(ACCENT)
                            .rounding(15.)
                            .min_size(egui::vec2(46., 46.))
                            .sense(egui::Sense::hover()),
                    )
                    .on_hover_text("Servidor local");
                });
            });
        egui::SidePanel::left("channels")
            .exact_width(220.)
            .resizable(false)
            .frame(surface(SIDEBAR, 0., 0.))
            .show(ctx, |ui| {
                surface(SIDEBAR, 16., 11.).show(ui, |ui| {
                    ui.label(egui::RichText::new("Servidor local").size(18.).strong());
                });
                ui.separator();
                egui::TopBottomPanel::bottom("account")
                    .frame(surface(INPUT, 12., 9.))
                    .show_inside(ui, |ui| {
                        ui.horizontal(|ui| {
                            avatar(ui, &self.name, &self.user, 32.);
                            ui.add(
                                egui::Label::new(egui::RichText::new(&self.name).strong())
                                    .truncate(),
                            );
                        });
                    });
                surface(SIDEBAR, 9., 10.).show(ui, |ui| {
                    ui.horizontal(|ui| {
                        ui.label(muted("Canales de texto").size(13.).strong());
                        ui.allocate_ui_with_layout(
                            egui::vec2(ui.available_width(), 22.),
                            egui::Layout::right_to_left(egui::Align::Center),
                            |ui| {
                                if ui.small_button("+").on_hover_text("Crear canal").clicked() {
                                    self.new_channel = Some(String::new());
                                }
                            },
                        );
                    });
                    let mut selected = None;
                    egui::ScrollArea::vertical()
                        .auto_shrink([false, false])
                        .show(ui, |ui| {
                            for c in &self.channels {
                                let active = self.channel == c.id;
                                let label = format!("#  {}", c.name);
                                let button = egui::Button::new(if active {
                                    egui::RichText::new(label).strong()
                                } else {
                                    muted(label)
                                })
                                .fill(if active {
                                    BORDER
                                } else {
                                    egui::Color32::TRANSPARENT
                                })
                                .min_size(egui::vec2(ui.available_width(), 32.))
                                .wrap_mode(egui::TextWrapMode::Truncate);
                                if ui.add_enabled(!self.busy, button).clicked() && !active {
                                    selected = Some(c.id.clone());
                                }
                            }
                        });
                    if let Some(channel) = selected {
                        self.select_channel(channel);
                    }
                });
            });

        let channel_name = self
            .channels
            .iter()
            .find(|c| c.id == self.channel)
            .map(|c| c.name.as_str())
            .unwrap_or("")
            .to_owned();
        egui::TopBottomPanel::top("channel_header")
            .frame(surface(CHAT, 18., 10.))
            .show(ctx, |ui| {
                ui.horizontal(|ui| {
                    ui.label(muted("#").size(25.));
                    ui.label(egui::RichText::new(&channel_name).size(18.).strong());
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        if !self.pages.is_empty()
                            && ui
                                .add_enabled(!self.busy, egui::Button::new("Más recientes"))
                                .clicked()
                        {
                            self.timeline_height = 0.;
                            self.pages.pop();
                            action = Some(Request::History(
                                self.channel.clone(),
                                self.pages.last().cloned(),
                            ));
                        }
                        if self.messages.len() == 50
                            && ui
                                .add_enabled(!self.busy, egui::Button::new("Anteriores"))
                                .clicked()
                        {
                            if let Some(first) = self.messages.first() {
                                self.timeline_height = 0.;
                                self.pages.push(first.id.clone());
                            }
                            action = Some(Request::History(
                                self.channel.clone(),
                                self.pages.last().cloned(),
                            ));
                        }
                    });
                });
            });
        egui::TopBottomPanel::bottom("composer")
            .show_separator_line(false)
            .frame(surface(CHAT, 16., 16.))
            .show(ctx, |ui| {
                let draft = self.drafts.entry(self.channel.clone()).or_default();
                let suggestion = matches!(draft.trim(), "/" | "/t" | "/te" | "/tes");
                let selected = suggestion && example_view::show_suggestion(ui);
                if selected {
                    example_view::COMMAND.clone_into(draft);
                    ui.add_space(4.);
                }
                surface(INPUT, 10., 7.).rounding(9.).show(ui, |ui| {
                    ui.horizontal(|ui| {
                        let field = ui.add(
                            egui::TextEdit::singleline(draft)
                                .frame(false)
                                .desired_width((ui.available_width() - 78.).max(60.))
                                .hint_text(format!("Enviar mensaje a #{channel_name}")),
                        );
                        if selected {
                            field.request_focus();
                        }
                        let ready = !self.busy && !draft.trim().is_empty();
                        if ui
                            .add_enabled(ready, egui::Button::new("Enviar").fill(ACCENT))
                            .clicked()
                            || ready
                                && field.lost_focus()
                                && ui.input(|i| i.key_pressed(egui::Key::Enter))
                        {
                            if suggestion
                                && !selected
                                && field.lost_focus()
                                && ui.input(|i| i.key_pressed(egui::Key::Enter))
                            {
                                example_view::COMMAND.clone_into(draft);
                                field.request_focus();
                                return;
                            }
                            self.clear_sent = Some((self.channel.clone(), draft.clone()));
                            action = Some(if draft.trim() == example_view::COMMAND {
                                Request::TestCommand(self.channel.clone())
                            } else {
                                Request::Send(self.channel.clone(), draft.clone())
                            });
                            field.request_focus();
                        }
                    });
                });
            });
        egui::CentralPanel::default()
            .frame(surface(CHAT, 18., 12.))
            .show(ctx, |ui| {
                let show_preview = self.pages.is_empty();
                egui::ScrollArea::vertical()
                    .id_salt((&self.channel, self.pages.last()))
                    .stick_to_bottom(true)
                    .auto_shrink([false, false])
                    .show_viewport(ui, |ui, viewport| {
                        ui.add_space((viewport.height() - self.timeline_height).max(0.));
                        let start = ui.cursor().top();
                        if show_preview && self.messages.len() < 50 {
                            example_view::welcome(ui, &channel_name);
                        }
                        if !show_preview && self.messages.is_empty() {
                            ui.label(muted("No hay mensajes anteriores."));
                        }
                        for m in &self.messages {
                            ui.push_id(&m.id, |ui| {
                                ui.horizontal_top(|ui| {
                                    avatar(ui, &m.author.username, &m.author.id, 38.);
                                    ui.vertical(|ui| {
                                        ui.horizontal_wrapped(|ui| {
                                            ui.label(
                                                egui::RichText::new(&m.author.username).strong(),
                                            );
                                            let time =
                                                chrono::DateTime::parse_from_rfc3339(&m.timestamp)
                                                    .ok()
                                                    .map_or_else(String::new, |t| {
                                                        t.with_timezone(&chrono::Local)
                                                            .format("%H:%M")
                                                            .to_string()
                                                    });
                                            ui.label(muted(time).size(12.));
                                            if m.edited_timestamp.is_some() {
                                                ui.label(muted("(editado)").size(12.));
                                            }
                                            if self.user == m.author.id {
                                                ui.menu_button("...", |ui| {
                                                    if ui.button("Editar mensaje").clicked() {
                                                        self.editing =
                                                            Some((m.id.clone(), m.content.clone()));
                                                        ui.close_menu();
                                                    }
                                                    if ui.button("Eliminar mensaje").clicked() {
                                                        self.deleting = Some(m.id.clone());
                                                        ui.close_menu();
                                                    }
                                                })
                                                .response
                                                .on_hover_text("Acciones del mensaje");
                                            }
                                        });
                                        ui.add(egui::Label::new(&m.content).wrap());
                                    });
                                });
                            });
                            ui.add_space(6.);
                        }
                        if show_preview
                            && let Some(text) = &self.command_preview
                            && example_view::show_reply(ui, &self.name, text)
                        {
                            self.command_preview = None;
                        }
                        let height = ui.cursor().top() - start;
                        if (height - self.timeline_height).abs() > 0.5 {
                            self.timeline_height = height;
                            ctx.request_repaint();
                        }
                    });
            });
        let mut cancel_channel = false;
        if let Some(name) = &mut self.new_channel {
            egui::Window::new("Crear canal de texto")
                .collapsible(false)
                .resizable(false)
                .show(ctx, |ui| {
                    ui.label("Nombre del canal");
                    ui.text_edit_singleline(name);
                    ui.horizontal(|ui| {
                        if ui.button("Cancelar").clicked() {
                            cancel_channel = true;
                        }
                        if ui
                            .add_enabled(
                                !self.busy && !name.trim().is_empty(),
                                egui::Button::new("Crear canal").fill(ACCENT),
                            )
                            .clicked()
                        {
                            action = Some(Request::CreateChannel(name.clone()));
                        }
                    });
                });
        }
        if cancel_channel {
            self.new_channel = None;
        }
        let mut close = false;
        if let Some((id, text)) = &mut self.editing {
            egui::Window::new("Editar mensaje").show(ctx, |ui| {
                ui.text_edit_multiline(text);
                if ui
                    .add_enabled(
                        !self.busy && !text.trim().is_empty(),
                        egui::Button::new("Guardar"),
                    )
                    .clicked()
                {
                    action = Some(Request::Edit(
                        self.channel.clone(),
                        id.clone(),
                        text.clone(),
                    ));
                }
                close = ui.button("Cancelar").clicked();
            });
            if close {
                self.editing = None;
            }
        }
        if let Some(id) = self.deleting.clone() {
            egui::Window::new("¿Eliminar el mensaje?").show(ctx, |ui| {
                if ui
                    .add_enabled(!self.busy, egui::Button::new("Eliminar"))
                    .clicked()
                {
                    action = Some(Request::Delete(self.channel.clone(), id));
                }
                if ui.button("Cancelar").clicked() {
                    self.deleting = None;
                }
            });
        }
        if let Some(r) = action {
            self.send(r)
        } else if !self.busy && Instant::now() >= self.next {
            self.send(Request::History(
                self.channel.clone(),
                self.pages.last().cloned(),
            ));
        }
    }
}
fn main() -> eframe::Result {
    let port = std::env::var("DISCORD_LOCAL_PORT")
        .unwrap_or_else(|_| "8790".into())
        .parse::<u16>()
        .expect("Puerto local inválido");
    let profile = Profile::load();
    let background = profile.background();
    eframe::run_native(
        "Discord · Rust local",
        eframe::NativeOptions {
            viewport: profile.viewport(),
            ..Default::default()
        },
        Box::new(move |cc| {
            apply(&cc.egui_ctx);
            apply_local_fonts(&cc.egui_ctx);

            Ok(Box::new(App::new(
                cc.egui_ctx.clone(),
                format!("http://127.0.0.1:{port}"),
                background,
            )))
        }),
    )
}
