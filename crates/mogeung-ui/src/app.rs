use crate::net::Net;
use crate::ui::{self, *};
use chrono::Utc;
use egui::{Color32, RichText};
use mogeung_core::attention::{fmt_dur, AttentionItem, AttentionReason};
use mogeung_core::change::RiskLevel;
use mogeung_core::session::LiveStatus;
use mogeung_core::transcript::{EventKind, NoticeLevel};
use mogeung_core::{Change, ClientMsg, ServerMsg, Session, SessionId, TranscriptEvent};
use std::collections::{HashMap, HashSet};

#[derive(PartialEq, Clone, Copy)]
enum Tab {
    Changes,
    Transcript,
    Info,
}

pub struct App {
    net: Net,

    sessions: HashMap<SessionId, Session>,
    queue: Vec<AttentionItem>,
    changes: HashMap<SessionId, Change>,
    events: HashMap<SessionId, Vec<TranscriptEvent>>,
    hydrated: HashSet<SessionId>,

    selected: Option<SessionId>,
    tab: Tab,
    selected_file: Option<String>,

    hide_reviewed: bool,
    hide_noise: bool,
    show_quiet: bool,

    launch_dir: String,
    launch_worktree: bool,
    show_launch: bool,

    errors: Vec<String>,
}

impl App {
    pub fn new(cc: &eframe::CreationContext<'_>, url: String) -> Self {
        cc.egui_ctx.set_visuals(egui::Visuals::dark());
        let net = Net::connect(url, cc.egui_ctx.clone());
        App {
            net,
            sessions: HashMap::new(),
            queue: Vec::new(),
            changes: HashMap::new(),
            events: HashMap::new(),
            hydrated: HashSet::new(),
            selected: None,
            tab: Tab::Changes,
            selected_file: None,
            hide_reviewed: false,
            hide_noise: true,
            show_quiet: false,
            launch_dir: String::new(),
            launch_worktree: true,
            show_launch: false,
            errors: Vec::new(),
        }
    }

    fn ingest(&mut self) {
        for msg in self.net.drain() {
            match msg {
                ServerMsg::Snapshot { sessions, queue } => {
                    self.sessions = sessions.into_iter().map(|s| (s.id.clone(), s)).collect();
                    self.queue = queue;
                    // A reconnect invalidates our transcript cache.
                    self.hydrated.clear();
                }
                ServerMsg::SessionUpdated { session } => {
                    self.sessions.insert(session.id.clone(), *session);
                }
                ServerMsg::SessionRemoved { session_id } => {
                    self.sessions.remove(&session_id);
                    self.changes.remove(&session_id);
                    self.events.remove(&session_id);
                    self.hydrated.remove(&session_id);
                    if self.selected.as_ref() == Some(&session_id) {
                        self.selected = None;
                    }
                }
                ServerMsg::Events { events } => {
                    for ev in events {
                        let list = self.events.entry(ev.session_id.clone()).or_default();
                        if list.last().map(|l| l.seq).unwrap_or(0) < ev.seq {
                            list.push(ev);
                        } else if !list.iter().any(|e| e.seq == ev.seq) {
                            list.push(ev);
                            list.sort_by_key(|e| e.seq);
                        }
                    }
                }
                ServerMsg::Queue { queue } => self.queue = queue,
                ServerMsg::ChangeUpdated { session_id, change } => {
                    if self.selected.as_ref() == Some(&session_id) {
                        let still_there = self
                            .selected_file
                            .as_ref()
                            .map(|p| change.files.iter().any(|f| &f.path == p))
                            .unwrap_or(false);
                        if !still_there {
                            self.selected_file = change.files.first().map(|f| f.path.clone());
                        }
                    }
                    self.changes.insert(session_id, change);
                }
                ServerMsg::Error { message } => {
                    self.errors.push(message);
                    if self.errors.len() > 6 {
                        self.errors.remove(0);
                    }
                }
            }
        }
    }

    fn select(&mut self, id: SessionId) {
        self.selected = Some(id.clone());
        self.selected_file = None;
        if self.hydrated.insert(id.clone()) {
            self.net.send(ClientMsg::FetchEvents {
                session_id: id.clone(),
                since: 0,
            });
        }
        self.net.send(ClientMsg::RefreshChange { session_id: id });
    }

    fn selected_session(&self) -> Option<&Session> {
        self.selected.as_ref().and_then(|id| self.sessions.get(id))
    }
}

impl eframe::App for App {
    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        self.ingest();
        // "Waiting for you — 4m12s" has to keep ticking with no new events.
        ui.ctx()
            .request_repaint_after(std::time::Duration::from_millis(1000));

        self.top_bar(ui);
        self.queue_panel(ui);
        self.detail_panel(ui);
        self.launch_window(ui);
    }
}

// ---------------------------------------------------------------------------
// Top bar
// ---------------------------------------------------------------------------

impl App {
    fn top_bar(&mut self, root: &mut egui::Ui) {
        egui::Panel::top("top").show(root, |ui| {
            ui.horizontal(|ui| {
                ui.label(RichText::new("mogeung").strong().size(17.0));

                let (dot, tip) = if self.net.connected {
                    (RichText::new("●").color(GREEN), "connected".to_string())
                } else {
                    (
                        RichText::new("●").color(RED),
                        self.net
                            .last_error
                            .clone()
                            .unwrap_or_else(|| "disconnected".into()),
                    )
                };
                ui.label(dot)
                    .on_hover_text(format!("{} — {}", self.net.url, tip));

                ui.separator();

                let waiting = self
                    .queue
                    .iter()
                    .filter(|i| i.reason == AttentionReason::AwaitingInput)
                    .count();
                let needing = self.queue.iter().filter(|i| i.reason.needs_human()).count();
                let live = self.sessions.values().filter(|s| s.alive).count();

                if waiting > 0 {
                    ui.label(badge(&format!("{waiting} waiting for you"), RED));
                } else if needing > 0 {
                    ui.label(badge(&format!("{needing} need you"), AMBER));
                } else {
                    ui.label(dim("queue clear"));
                }
                ui.label(dim(format!("· {live} live session(s)")));

                let out: u64 = self.sessions.values().map(|s| s.tokens_out).sum();
                ui.label(dim(format!("· {} tokens out", tokens(out))));

                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    if ui
                        .button("+ New session")
                        .on_hover_text("opens a real interactive claude in your terminal")
                        .clicked()
                    {
                        if self.launch_dir.is_empty() {
                            // Default to the repo of whatever you are looking at.
                            self.launch_dir = self
                                .selected_session()
                                .map(|s| s.repo_root.clone().unwrap_or_else(|| s.cwd.clone()))
                                .unwrap_or_default();
                        }
                        self.show_launch = true;
                    }
                    if ui.button("Rescan").clicked() {
                        self.net.send(ClientMsg::Rescan);
                    }
                });
            });

            if !self.errors.is_empty() {
                ui.horizontal_wrapped(|ui| {
                    ui.label(RichText::new("!").color(RED).strong());
                    ui.label(
                        RichText::new(self.errors.last().unwrap())
                            .color(RED)
                            .size(12.0),
                    );
                    if ui.small_button("dismiss").clicked() {
                        self.errors.clear();
                    }
                });
            }
        });
    }
}

// ---------------------------------------------------------------------------
// Attention queue
// ---------------------------------------------------------------------------

impl App {
    fn queue_panel(&mut self, root: &mut egui::Ui) {
        egui::Panel::left("queue")
            .default_size(380.0)
            .size_range(300.0..=560.0)
            .show(root, |ui| {
                ui.add_space(4.0);
                ui.horizontal(|ui| {
                    ui.label(RichText::new("ATTENTION").size(11.0).color(DIM).strong());
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        ui.checkbox(&mut self.show_quiet, "show quiet");
                    });
                });
                ui.label(dim("waiting → failed → review → stalled → running"));
                ui.separator();

                let now = Utc::now();
                let queue = self.queue.clone();
                let mut to_select = None;
                let show_quiet = self.show_quiet;

                egui::ScrollArea::vertical()
                    .auto_shrink([false, false])
                    .show(ui, |ui| {
                        let mut shown = 0;
                        let mut hidden = 0;
                        for item in &queue {
                            let Some(session) = self.sessions.get(&item.session_id) else {
                                continue;
                            };
                            let quiet = item.reason == AttentionReason::Idle;
                            if quiet && !show_quiet {
                                hidden += 1;
                                continue;
                            }
                            shown += 1;

                            let selected = self.selected.as_ref() == Some(&item.session_id);
                            let resp = ui.push_id(&item.session_id, |ui| {
                                egui::Frame::group(ui.style())
                                    .fill(if selected {
                                        ui.visuals().selection.bg_fill.linear_multiply(0.35)
                                    } else {
                                        Color32::TRANSPARENT
                                    })
                                    .show(ui, |ui| {
                                        ui.set_width(ui.available_width());
                                        queue_card(ui, session, item, now);
                                    })
                            });
                            if resp.response.interact(egui::Sense::click()).clicked() {
                                to_select = Some(item.session_id.clone());
                            }
                            ui.add_space(2.0);
                        }

                        if shown == 0 {
                            ui.add_space(20.0);
                            ui.vertical_centered(|ui| {
                                ui.label(dim("nothing needs you"));
                                ui.label(dim("run claude in a terminal and it shows up here"));
                            });
                        }
                        if hidden > 0 {
                            ui.add_space(6.0);
                            ui.label(dim(format!("{hidden} quiet session(s) hidden")));
                        }
                    });

                if let Some(id) = to_select {
                    self.select(id);
                }
            });
    }
}

fn queue_card(
    ui: &mut egui::Ui,
    s: &Session,
    item: &AttentionItem,
    now: chrono::DateTime<Utc>,
) {
    ui.horizontal(|ui| {
        ui.label(badge(item.reason.label(), reason_color(item.reason)));
        if s.alive {
            let (txt, col) = match s.live_status {
                Some(LiveStatus::Busy) => ("live·busy", BLUE),
                Some(LiveStatus::Idle) => ("live·idle", AMBER),
                _ => ("live", DIM),
            };
            ui.label(RichText::new(txt).size(10.5).color(col));
        }
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            ui.label(dim(fmt_dur(s.duration_secs(now))));
        });
    });

    ui.label(RichText::new(truncate(&s.label(), 100)).size(13.0));

    ui.horizontal_wrapped(|ui| {
        ui.label(dim(s.repo_name()));
        if let Some(b) = &s.git_branch {
            ui.label(dim(format!("⑂ {b}")));
        }
        if s.files_changed > 0 {
            ui.label(dim(format!(
                "{} files +{} -{}",
                s.files_changed, s.insertions, s.deletions
            )));
        }
        if s.turns > 0 {
            ui.label(dim(format!("{} turns", s.turns)));
        }
    });
    ui.label(dim(truncate(&item.detail, 90)));
}

// ---------------------------------------------------------------------------
// Detail
// ---------------------------------------------------------------------------

impl App {
    fn detail_panel(&mut self, root: &mut egui::Ui) {
        egui::CentralPanel::default().show(root, |ui| {
            let Some(s) = self.selected_session().cloned() else {
                ui.centered_and_justified(|ui| {
                    ui.label(dim("select a session"));
                });
                return;
            };
            let now = Utc::now();

            ui.horizontal_wrapped(|ui| {
                if s.alive {
                    let (txt, col) = match s.live_status {
                        Some(LiveStatus::Idle) => ("WAITING FOR YOU", RED),
                        Some(LiveStatus::Busy) => ("BUSY", BLUE),
                        _ => ("LIVE", DIM),
                    };
                    ui.label(badge(txt, col));
                } else {
                    ui.label(badge("ended", DIM));
                }
                ui.label(RichText::new(truncate(&s.label(), 120)).size(14.0).strong());
            });

            ui.horizontal_wrapped(|ui| {
                ui.label(dim(s.repo_name()));
                if let Some(b) = &s.git_branch {
                    ui.label(dim(format!("· ⑂ {b}")));
                }
                ui.label(dim(format!("· {}", fmt_dur(s.duration_secs(now)))));
                ui.label(dim(format!("· {} turns", s.turns)));
                ui.label(dim(format!("· {} tool calls", s.tool_calls)));
                ui.label(dim(format!("· {} tokens out", tokens(s.tokens_out))));
                if let Some(pid) = s.pid {
                    ui.label(dim(format!("· pid {pid}")));
                }
            });

            if let Some(err) = &s.error {
                ui.label(RichText::new(err).color(RED).size(12.0));
            }
            if let Some(w) = s.waiting_secs(now) {
                ui.label(
                    RichText::new(format!(
                        "This session has been waiting for your input for {}.",
                        fmt_dur(w)
                    ))
                    .color(AMBER)
                    .size(12.5),
                );
            }

            ui.horizontal_wrapped(|ui| {
                if ui.button("Refresh diff").clicked() {
                    self.net.send(ClientMsg::RefreshChange {
                        session_id: s.id.clone(),
                    });
                }
                ui.separator();
                ui.label(dim("open in"));
                for t in [
                    OpenTarget::Terminal,
                    OpenTarget::Intellij,
                    OpenTarget::VsCode,
                    OpenTarget::Finder,
                ] {
                    if ui.button(t.label()).clicked() {
                        if let Err(e) = ui::open_in(t, &s.cwd) {
                            self.errors.push(e);
                        }
                    }
                }
                ui.separator();
                if ui
                    .button("Forget")
                    .on_hover_text("stop tracking this session and drop its review marks")
                    .clicked()
                {
                    self.net.send(ClientMsg::ForgetSession {
                        session_id: s.id.clone(),
                    });
                }
            });

            ui.label(mono(&s.cwd).color(DIM));
            ui.separator();

            let unread = self
                .changes
                .get(&s.id)
                .map(|c| c.unreviewed_hunks())
                .unwrap_or(0);
            ui.horizontal(|ui| {
                let changes_label = if unread > 0 {
                    format!("Changes ({unread} unread)")
                } else {
                    "Changes".to_string()
                };
                ui.selectable_value(&mut self.tab, Tab::Changes, changes_label);
                ui.selectable_value(&mut self.tab, Tab::Transcript, "Transcript");
                ui.selectable_value(&mut self.tab, Tab::Info, "Info");
            });
            ui.separator();

            match self.tab {
                Tab::Changes => self.changes_tab(ui, &s),
                Tab::Transcript => self.transcript_tab(ui, &s),
                Tab::Info => self.info_tab(ui, &s),
            }
        });
    }

    fn transcript_tab(&mut self, ui: &mut egui::Ui, s: &Session) {
        let events = self.events.get(&s.id).cloned().unwrap_or_default();
        egui::ScrollArea::vertical()
            .auto_shrink([false, false])
            .stick_to_bottom(true)
            .show(ui, |ui| {
                if events.is_empty() {
                    ui.label(dim("no events yet"));
                }
                for ev in &events {
                    event_row(ui, ev);
                }
            });
    }

    fn changes_tab(&mut self, ui: &mut egui::Ui, s: &Session) {
        let Some(change) = self.changes.get(&s.id).cloned() else {
            ui.label(dim("computing diff…"));
            return;
        };
        if let Some(err) = &change.error {
            ui.label(RichText::new(err).color(AMBER).size(12.0));
        }
        if change.files.is_empty() {
            ui.label(dim("no file changes attributed to this session"));
            if !s.touched_files.is_empty() {
                ui.label(dim(format!(
                    "{} file(s) were edited but the diff is empty — they may have been committed or reverted",
                    s.touched_files.len()
                )));
            }
            return;
        }

        ui.horizontal(|ui| {
            let total = change.total_hunks();
            let read = change.reviewed_hunks();
            ui.label(dim(format!("{read}/{total} hunks read")));
            ui.add(
                egui::ProgressBar::new(change.review_progress())
                    .desired_width(140.0)
                    .show_percentage(),
            );
            ui.checkbox(&mut self.hide_reviewed, "hide read");
            ui.checkbox(&mut self.hide_noise, "hide noise");
            if ui.button("Mark all read").clicked() {
                self.net.send(ClientMsg::ReviewAll {
                    session_id: s.id.clone(),
                });
            }
        });
        ui.separator();

        if self.selected_file.is_none() {
            self.selected_file = change.files.first().map(|f| f.path.clone());
        }

        egui::Panel::left("files").default_size(300.0).show(ui, |ui| {
            egui::ScrollArea::vertical()
                .auto_shrink([false, false])
                .show(ui, |ui| {
                    for f in &change.files {
                        if self.hide_noise && f.risk() == RiskLevel::Noise {
                            continue;
                        }
                        if self.hide_reviewed && f.fully_reviewed() {
                            continue;
                        }
                        let selected = self.selected_file.as_deref() == Some(f.path.as_str());
                        let unread = f.hunks.len() - f.reviewed_hunks();
                        let resp = ui.selectable_label(
                            selected,
                            RichText::new(format!(
                                "{}{}",
                                if unread == 0 { "✓ " } else { "" },
                                f.path
                            ))
                            .size(12.5),
                        );
                        ui.horizontal_wrapped(|ui| {
                            ui.label(badge(f.risk().label(), risk_color(f.risk())));
                            ui.label(dim(format!("+{} -{}", f.insertions, f.deletions)));
                            for fl in f.flags.iter().take(3) {
                                ui.label(dim(fl.label()));
                            }
                        });
                        if resp.clicked() {
                            self.selected_file = Some(f.path.clone());
                        }
                        ui.add_space(2.0);
                    }
                });
        });

        let file = change
            .files
            .iter()
            .find(|f| Some(f.path.as_str()) == self.selected_file.as_deref());
        egui::ScrollArea::vertical()
            .auto_shrink([false, false])
            .show(ui, |ui| {
                let Some(file) = file else {
                    ui.label(dim("select a file"));
                    return;
                };
                if file.truncated {
                    ui.label(dim("diff not shown (binary or too large)"));
                    return;
                }
                for hunk in &file.hunks {
                    if self.hide_reviewed && hunk.reviewed {
                        continue;
                    }
                    egui::Frame::group(ui.style()).show(ui, |ui| {
                        ui.set_width(ui.available_width());
                        ui.horizontal_wrapped(|ui| {
                            let mut reviewed = hunk.reviewed;
                            if ui.checkbox(&mut reviewed, "read").changed() {
                                self.net.send(ClientMsg::SetHunkReviewed {
                                    session_id: s.id.clone(),
                                    anchor: hunk.anchor.clone(),
                                    reviewed,
                                });
                            }
                            ui.label(badge(hunk.risk().label(), risk_color(hunk.risk())));
                            ui.label(mono(&hunk.header).color(DIM));
                            for fl in hunk.flags.iter().take(4) {
                                ui.label(dim(fl.label()));
                            }
                        });
                        for line in hunk.lines.iter().take(500) {
                            diff_line(ui, line);
                        }
                        if hunk.lines.len() > 500 {
                            ui.label(dim(format!(
                                "… {} more lines — open in an editor",
                                hunk.lines.len() - 500
                            )));
                        }
                    });
                    ui.add_space(4.0);
                }
            });
    }

    fn info_tab(&mut self, ui: &mut egui::Ui, s: &Session) {
        egui::ScrollArea::vertical()
            .auto_shrink([false, false])
            .show(ui, |ui| {
                let mut row = |k: &str, v: String| {
                    ui.horizontal(|ui| {
                        ui.label(dim(format!("{k:>16}")));
                        ui.label(mono(v));
                    });
                };
                row("session id", s.id.clone());
                row("name", s.name.clone().unwrap_or_else(|| "—".into()));
                row("cwd", s.cwd.clone());
                row("repo", s.repo_root.clone().unwrap_or_else(|| "—".into()));
                row("branch", s.git_branch.clone().unwrap_or_else(|| "—".into()));
                row("base commit", s.base_sha.clone().unwrap_or_else(|| "—".into()));
                row("cli version", s.version.clone().unwrap_or_else(|| "—".into()));
                row("started", s.started_at.to_rfc3339());
                row("last event", s.last_event_at.to_rfc3339());
                row("transcript", s.transcript_path.clone());

                ui.add_space(8.0);
                ui.label(dim(format!("files touched ({})", s.touched_files.len())));
                for f in s.touched_files.iter().take(60) {
                    ui.label(mono(f));
                }
                if s.touched_files.len() > 60 {
                    ui.label(dim(format!("… {} more", s.touched_files.len() - 60)));
                }

                if let Some(p) = &s.last_prompt {
                    ui.add_space(8.0);
                    ui.label(dim("last prompt"));
                    ui.label(RichText::new(p).size(12.5));
                }
            });
    }
}

fn diff_line(ui: &mut egui::Ui, line: &str) {
    let text = RichText::new(line).monospace().size(11.5);
    let styled = match line.chars().next() {
        Some('+') => text
            .color(Color32::from_rgb(0x8F, 0xE0, 0xA6))
            .background_color(ADD_BG),
        Some('-') => text
            .color(Color32::from_rgb(0xF0, 0x9C, 0xA0))
            .background_color(DEL_BG),
        _ => text.color(Color32::from_rgb(0xA8, 0xA8, 0xB0)),
    };
    ui.label(styled);
}

fn event_row(ui: &mut egui::Ui, ev: &TranscriptEvent) {
    let time = ev.ts.format("%H:%M:%S").to_string();
    match &ev.kind {
        EventKind::Init {
            model,
            cwd,
            tool_count,
        } => {
            ui.horizontal_wrapped(|ui| {
                ui.label(dim(time));
                ui.label(badge("init", DIM));
                ui.label(dim(format!("{model} · {tool_count} tools · {cwd}")));
            });
        }
        EventKind::UserPrompt { text } => {
            ui.horizontal_wrapped(|ui| {
                ui.label(dim(time));
                ui.label(badge("you", BLUE));
            });
            ui.indent("u", |ui| {
                ui.label(RichText::new(text).size(12.5));
            });
        }
        EventKind::AssistantText { text } => {
            ui.horizontal_wrapped(|ui| {
                ui.label(dim(time));
                ui.label(badge("agent", GREEN));
            });
            ui.indent("a", |ui| {
                ui.label(RichText::new(text).size(12.5));
            });
        }
        EventKind::Thinking { text } => {
            egui::CollapsingHeader::new(dim(format!("{time}  thinking")))
                .id_salt(ev.seq)
                .show(ui, |ui| {
                    ui.label(dim(text.clone()));
                });
        }
        EventKind::ToolUse { name, summary, .. } => {
            ui.horizontal_wrapped(|ui| {
                ui.label(dim(time));
                ui.label(badge(name, PURPLE));
                ui.label(mono(truncate(summary, 150)));
            });
        }
        EventKind::ToolResult {
            is_error, preview, ..
        } => {
            let head = if *is_error { "error" } else { "result" };
            let color = if *is_error { RED } else { DIM };
            egui::CollapsingHeader::new(
                RichText::new(format!("{time}  {head}: {}", truncate(preview, 90)))
                    .size(11.5)
                    .color(color),
            )
            .id_salt(ev.seq)
            .show(ui, |ui| {
                ui.label(mono(preview.clone()));
            });
        }
        EventKind::Result {
            cost_usd,
            num_turns,
            terminal_reason,
            is_error,
            text,
        } => {
            ui.separator();
            ui.horizontal_wrapped(|ui| {
                ui.label(dim(time));
                ui.label(badge(
                    if *is_error { "failed" } else { "done" },
                    if *is_error { RED } else { GREEN },
                ));
                ui.label(dim(format!(
                    "{terminal_reason} · {num_turns} turns · {}",
                    money(*cost_usd)
                )));
            });
            if !text.trim().is_empty() {
                ui.indent("r", |ui| {
                    ui.label(RichText::new(text).size(12.5));
                });
            }
        }
        EventKind::Notice { level, message } => {
            let color = match level {
                NoticeLevel::Info => DIM,
                NoticeLevel::Warn => AMBER,
                NoticeLevel::Error => RED,
            };
            ui.horizontal_wrapped(|ui| {
                ui.label(dim(time));
                ui.label(badge("notice", color));
                ui.label(RichText::new(message).color(color).size(12.0));
            });
        }
    }
}

// ---------------------------------------------------------------------------
// Launch a real session
// ---------------------------------------------------------------------------

impl App {
    fn launch_window(&mut self, root: &mut egui::Ui) {
        if !self.show_launch {
            return;
        }
        let ctx = root.ctx().clone();
        let mut open = true;
        let mut go = false;

        egui::Window::new("New session")
            .open(&mut open)
            .default_width(560.0)
            .collapsible(false)
            .show(&ctx, |ui| {
                ui.label(
                    RichText::new("Opens a real interactive claude in Terminal.")
                        .size(12.5),
                );
                ui.label(dim(
                    "mogeung does not wrap the conversation — you drive it exactly as usual, and it shows up in the queue.",
                ));
                ui.add_space(8.0);

                ui.label(dim("directory"));
                ui.add(
                    egui::TextEdit::singleline(&mut self.launch_dir)
                        .hint_text("~/projects/foo")
                        .desired_width(f32::INFINITY),
                );

                // Offer the repos we already know about, so this is one click.
                let mut repos: Vec<String> = self
                    .sessions
                    .values()
                    .filter_map(|s| s.repo_root.clone())
                    .collect();
                repos.sort();
                repos.dedup();
                if !repos.is_empty() {
                    ui.add_space(4.0);
                    ui.label(dim("recent repos"));
                    egui::ScrollArea::vertical()
                        .max_height(120.0)
                        .show(ui, |ui| {
                            for r in repos {
                                if ui.selectable_label(self.launch_dir == r, &r).clicked() {
                                    self.launch_dir = r.clone();
                                }
                            }
                        });
                }

                ui.add_space(8.0);
                ui.checkbox(
                    &mut self.launch_worktree,
                    "create a fresh git worktree first",
                );
                ui.label(dim(
                    "a worktree isolates the new session so parallel agents cannot collide",
                ));

                ui.add_space(10.0);
                ui.horizontal(|ui| {
                    if ui
                        .add_enabled(
                            !self.launch_dir.trim().is_empty(),
                            egui::Button::new("Launch"),
                        )
                        .clicked()
                    {
                        go = true;
                    }
                    if ui.button("Cancel").clicked() {
                        self.show_launch = false;
                    }
                });
            });

        if go {
            self.net.send(ClientMsg::LaunchTerminal {
                dir: self.launch_dir.trim().to_string(),
                worktree: self.launch_worktree,
            });
            self.show_launch = false;
        }
        if !open {
            self.show_launch = false;
        }
    }
}
