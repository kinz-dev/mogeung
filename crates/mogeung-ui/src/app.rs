use crate::net::Net;
use crate::ui::{self, *};
use chrono::Utc;
use egui::{Color32, RichText};
use mogeung_core::attention::{fmt_dur, AttentionItem, AttentionReason};
use mogeung_core::change::RiskLevel;
use mogeung_core::transcript::{EventKind, NoticeLevel};
use mogeung_core::{
    Change, ClientMsg, NewRunSpec, PermissionMode, Run, RunId, RunStatus, ServerMsg,
    TranscriptEvent,
};
use std::collections::{HashMap, HashSet};

#[derive(PartialEq, Clone, Copy)]
enum Tab {
    Transcript,
    Changes,
    Info,
}

pub struct App {
    net: Net,

    runs: HashMap<RunId, Run>,
    queue: Vec<AttentionItem>,
    repos: Vec<String>,
    changes: HashMap<RunId, Change>,
    events: HashMap<RunId, Vec<TranscriptEvent>>,
    hydrated: HashSet<RunId>,

    selected: Option<RunId>,
    tab: Tab,
    selected_file: Option<String>,

    // Review view filters
    hide_reviewed: bool,
    hide_noise: bool,

    follow_up: String,

    // New-run dialog
    show_new: bool,
    new_repo: String,
    new_intent: String,
    new_model: String,
    new_perm: PermissionMode,
    new_worktree: bool,
    add_repo_path: String,

    errors: Vec<String>,
}

impl App {
    pub fn new(cc: &eframe::CreationContext<'_>, url: String) -> Self {
        cc.egui_ctx.set_visuals(egui::Visuals::dark());
        let net = Net::connect(url, cc.egui_ctx.clone());
        App {
            net,
            runs: HashMap::new(),
            queue: Vec::new(),
            repos: Vec::new(),
            changes: HashMap::new(),
            events: HashMap::new(),
            hydrated: HashSet::new(),
            selected: None,
            tab: Tab::Transcript,
            selected_file: None,
            hide_reviewed: false,
            hide_noise: true,
            follow_up: String::new(),
            show_new: false,
            new_repo: String::new(),
            new_intent: String::new(),
            new_model: "sonnet".into(),
            new_perm: PermissionMode::AcceptEdits,
            new_worktree: true,
            add_repo_path: String::new(),
            errors: Vec::new(),
        }
    }

    fn ingest(&mut self) {
        for msg in self.net.drain() {
            match msg {
                ServerMsg::Snapshot { runs, repos, queue } => {
                    self.runs = runs.into_iter().map(|r| (r.id, r)).collect();
                    self.repos = repos;
                    self.queue = queue;
                    if self.new_repo.is_empty() {
                        self.new_repo = self.repos.first().cloned().unwrap_or_default();
                    }
                    // A reconnect invalidates our transcript cache.
                    self.hydrated.clear();
                }
                ServerMsg::RunUpdated { run } => {
                    self.runs.insert(run.id, run);
                }
                ServerMsg::RunDeleted { run_id } => {
                    self.runs.remove(&run_id);
                    self.changes.remove(&run_id);
                    self.events.remove(&run_id);
                    self.hydrated.remove(&run_id);
                    if self.selected == Some(run_id) {
                        self.selected = None;
                    }
                }
                ServerMsg::Events { events } => {
                    for ev in events {
                        let list = self.events.entry(ev.run_id).or_default();
                        // The daemon can replay history, so drop duplicates by seq.
                        if list.last().map(|l| l.seq).unwrap_or(0) < ev.seq {
                            list.push(ev);
                        } else if !list.iter().any(|e| e.seq == ev.seq) {
                            list.push(ev);
                            list.sort_by_key(|e| e.seq);
                        }
                    }
                }
                ServerMsg::Queue { queue } => self.queue = queue,
                ServerMsg::ChangeUpdated { run_id, change } => {
                    // Keep the selected file if it still exists in the new diff.
                    if self.selected == Some(run_id) {
                        let still_there = self
                            .selected_file
                            .as_ref()
                            .map(|p| change.files.iter().any(|f| &f.path == p))
                            .unwrap_or(false);
                        if !still_there {
                            self.selected_file = change.files.first().map(|f| f.path.clone());
                        }
                    }
                    self.changes.insert(run_id, change);
                }
                ServerMsg::RepoList { repos } => {
                    self.repos = repos;
                    if self.new_repo.is_empty() {
                        self.new_repo = self.repos.first().cloned().unwrap_or_default();
                    }
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

    fn select(&mut self, id: RunId) {
        self.selected = Some(id);
        self.selected_file = None;
        self.follow_up.clear();
        // Pull history and a fresh diff the first time we look at a run.
        if self.hydrated.insert(id) {
            self.net.send(ClientMsg::FetchEvents {
                run_id: id,
                since: 0,
            });
        }
        self.net.send(ClientMsg::RefreshChange { run_id: id });
    }

    fn selected_run(&self) -> Option<&Run> {
        self.selected.and_then(|id| self.runs.get(&id))
    }
}

impl eframe::App for App {
    // egui 0.35 hands the app a root `Ui` rather than a `Context`; panels nest
    // inside it.
    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        self.ingest();
        // Ages and "silent for Ns" have to keep ticking without new events.
        ui.ctx()
            .request_repaint_after(std::time::Duration::from_millis(1000));

        self.top_bar(ui);
        self.queue_panel(ui);
        self.detail_panel(ui);
        self.new_run_window(ui);
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
                ui.label(dot).on_hover_text(format!("{} — {}", self.net.url, tip));

                ui.separator();

                let needing: usize = self
                    .queue
                    .iter()
                    .filter(|i| i.reason.needs_human())
                    .count();
                let active = self
                    .runs
                    .values()
                    .filter(|r| !r.status.is_terminal())
                    .count();

                if needing > 0 {
                    ui.label(badge(&format!("{needing} need you"), AMBER));
                } else {
                    ui.label(dim("queue clear"));
                }
                ui.label(dim(format!("· {active} running")));

                let spend: f64 = self.runs.values().map(|r| r.cost_usd).sum();
                ui.label(dim(format!("· {} total", money(spend))));

                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    if ui.button("+ New run").clicked() {
                        self.show_new = true;
                    }
                });
            });

            if !self.errors.is_empty() {
                ui.horizontal_wrapped(|ui| {
                    ui.label(RichText::new("!").color(RED).strong());
                    ui.label(RichText::new(self.errors.last().unwrap()).color(RED).size(12.0));
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
            .default_size(360.0)
            .size_range(egui::Rangef::new(280.0, 520.0))
            .show(root, |ui| {
                ui.add_space(4.0);
                ui.label(RichText::new("ATTENTION").size(11.0).color(DIM).strong());
                ui.label(
                    dim("ranked: blocked → failed → review → stalled → burning")
                );
                ui.separator();

                let now = Utc::now();
                let queue = self.queue.clone();
                let mut to_select = None;

                egui::ScrollArea::vertical()
                    .auto_shrink([false, false])
                    .show(ui, |ui| {
                        let mut quiet_header_drawn = false;
                        for item in &queue {
                            let Some(run) = self.runs.get(&item.run_id) else {
                                continue;
                            };
                            // Visually separate "wants you" from mere history.
                            if !item.reason.needs_human()
                                && !matches!(item.reason, AttentionReason::Running)
                                && !quiet_header_drawn
                            {
                                quiet_header_drawn = true;
                                ui.add_space(8.0);
                                ui.label(RichText::new("QUIET").size(11.0).color(DIM).strong());
                                ui.separator();
                            }

                            let selected = self.selected == Some(item.run_id);
                            let resp = ui.push_id(item.run_id.0, |ui| {
                                egui::Frame::group(ui.style())
                                    .fill(if selected {
                                        ui.visuals().selection.bg_fill.linear_multiply(0.35)
                                    } else {
                                        Color32::TRANSPARENT
                                    })
                                    .show(ui, |ui| {
                                        ui.set_width(ui.available_width());
                                        queue_card(ui, run, item, now);
                                    })
                            });
                            if resp.response.interact(egui::Sense::click()).clicked() {
                                to_select = Some(item.run_id);
                            }
                            ui.add_space(2.0);
                        }

                        if queue.is_empty() {
                            ui.add_space(20.0);
                            ui.vertical_centered(|ui| {
                                ui.label(dim("no runs yet"));
                                ui.label(dim("press “+ New run” to start one"));
                            });
                        }
                    });

                if let Some(id) = to_select {
                    self.select(id);
                }
            });
    }
}

fn queue_card(ui: &mut egui::Ui, run: &Run, item: &AttentionItem, now: chrono::DateTime<Utc>) {
    ui.horizontal(|ui| {
        ui.label(badge(item.reason.label(), reason_color(item.reason)));
        ui.label(dim(fmt_dur(run.duration_secs(now))));
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            if run.cost_usd > 0.0 {
                ui.label(dim(money(run.cost_usd)));
            }
        });
    });
    ui.label(RichText::new(truncate(&run.intent, 110)).size(13.0));
    ui.horizontal_wrapped(|ui| {
        let repo = run
            .repo_path
            .rsplit('/')
            .next()
            .unwrap_or(&run.repo_path)
            .to_string();
        ui.label(dim(repo));
        if let Some(b) = &run.branch {
            ui.label(dim(format!("⑂ {}", b.trim_start_matches("mogeung/"))));
        }
        if run.files_changed > 0 {
            ui.label(dim(format!(
                "{} files +{} -{}",
                run.files_changed, run.insertions, run.deletions
            )));
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
            let Some(run) = self.selected_run().cloned() else {
                ui.centered_and_justified(|ui| {
                    ui.label(dim("select a run"));
                });
                return;
            };
            let now = Utc::now();

            ui.horizontal(|ui| {
                ui.label(badge(run.status.label(), status_color(run.status)));
                ui.label(RichText::new(truncate(&run.intent, 130)).size(14.0).strong());
            });

            ui.horizontal_wrapped(|ui| {
                ui.label(dim(format!("{} {}", run.agent.label(), run.model.clone().unwrap_or_default())));
                ui.label(dim(format!("· {}", fmt_dur(run.duration_secs(now)))));
                ui.label(dim(format!("· {}", money(run.cost_usd))));
                ui.label(dim(format!(
                    "· {} in / {} out",
                    tokens(run.tokens_in),
                    tokens(run.tokens_out)
                )));
                ui.label(dim(format!("· {} turns", run.num_turns)));
                if run.permission_denials > 0 {
                    ui.label(badge(
                        &format!("{} denied", run.permission_denials),
                        RED,
                    ));
                }
            });

            if let Some(err) = &run.error {
                ui.label(RichText::new(err).color(RED).size(12.0));
            }

            ui.horizontal_wrapped(|ui| {
                if !run.status.is_terminal() && ui.button("Cancel").clicked() {
                    self.net.send(ClientMsg::CancelRun { run_id: run.id });
                }
                if ui.button("Refresh diff").clicked() {
                    self.net.send(ClientMsg::RefreshChange { run_id: run.id });
                }
                ui.separator();
                ui.label(dim("open in"));
                for t in [
                    OpenTarget::Intellij,
                    OpenTarget::VsCode,
                    OpenTarget::Terminal,
                    OpenTarget::Finder,
                ] {
                    if ui.button(t.label()).clicked() {
                        if let Err(e) = ui::open_in(t, &run.cwd) {
                            self.errors.push(e);
                        }
                    }
                }
                ui.separator();
                if ui.button("Delete").on_hover_text("forget this run; also removes its worktree").clicked() {
                    self.net.send(ClientMsg::DeleteRun {
                        run_id: run.id,
                        remove_worktree: true,
                    });
                }
            });

            ui.label(mono(&run.cwd).color(DIM));
            ui.separator();

            let unread = self
                .changes
                .get(&run.id)
                .map(|c| c.unreviewed_hunks())
                .unwrap_or(0);
            ui.horizontal(|ui| {
                ui.selectable_value(&mut self.tab, Tab::Transcript, "Transcript");
                let changes_label = if unread > 0 {
                    format!("Changes ({unread} unread)")
                } else {
                    "Changes".to_string()
                };
                ui.selectable_value(&mut self.tab, Tab::Changes, changes_label);
                ui.selectable_value(&mut self.tab, Tab::Info, "Info");
            });
            ui.separator();

            match self.tab {
                Tab::Transcript => self.transcript_tab(ui, &run),
                Tab::Changes => self.changes_tab(ui, &run),
                Tab::Info => self.info_tab(ui, &run),
            }
        });
    }

    fn transcript_tab(&mut self, ui: &mut egui::Ui, run: &Run) {
        // Follow-up box is pinned to the bottom: it is the main way a review
        // turns back into work (CONCEPT.md B7).
        egui::Panel::bottom("followup")
            .resizable(false)
            .show(ui, |ui| {
                ui.add_space(4.0);
                let can_send = run.status.is_terminal() && run.session_id.is_some();
                ui.horizontal(|ui| {
                    ui.add_enabled(
                        can_send,
                        egui::TextEdit::multiline(&mut self.follow_up)
                            .desired_rows(2)
                            .hint_text(if can_send {
                                "follow up in this session — e.g. “the error path in auth.rs is wrong, fix it”"
                            } else {
                                "run is still active"
                            })
                            .desired_width(ui.available_width() - 90.0),
                    );
                    if ui
                        .add_enabled(
                            can_send && !self.follow_up.trim().is_empty(),
                            egui::Button::new("Send"),
                        )
                        .clicked()
                    {
                        self.net.send(ClientMsg::FollowUp {
                            run_id: run.id,
                            prompt: self.follow_up.trim().to_string(),
                        });
                        self.follow_up.clear();
                    }
                });
                ui.add_space(4.0);
            });

        let events = self.events.get(&run.id).cloned().unwrap_or_default();
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

    fn changes_tab(&mut self, ui: &mut egui::Ui, run: &Run) {
        let Some(change) = self.changes.get(&run.id).cloned() else {
            ui.label(dim("computing diff…"));
            return;
        };
        if let Some(err) = &change.error {
            ui.label(RichText::new(err).color(AMBER).size(12.0));
        }
        if change.files.is_empty() {
            ui.label(dim("no file changes"));
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
            if ui
                .button("Mark all read")
                .on_hover_text("promotes the run out of the attention queue")
                .clicked()
            {
                self.net.send(ClientMsg::ReviewAll { run_id: run.id });
            }
        });
        ui.separator();

        if self.selected_file.is_none() {
            self.selected_file = change.files.first().map(|f| f.path.clone());
        }

        // File list on the left, ordered by risk rather than alphabetically.
        egui::Panel::left("files")
            .default_size(300.0)
            .show(ui, |ui| {
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

        // Hunks for the selected file.
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
                                    run_id: run.id,
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
                        // Cap rendering so a runaway hunk cannot stall the frame.
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

    fn info_tab(&mut self, ui: &mut egui::Ui, run: &Run) {
        egui::ScrollArea::vertical()
            .auto_shrink([false, false])
            .show(ui, |ui| {
                let mut row = |k: &str, v: String| {
                    ui.horizontal(|ui| {
                        ui.label(dim(format!("{k:>16}")));
                        ui.label(mono(v));
                    });
                };
                row("run id", run.id.to_string());
                row("session root", run.root.to_string());
                row(
                    "parent",
                    run.parent.map(|p| p.to_string()).unwrap_or_else(|| "—".into()),
                );
                row(
                    "agent session",
                    run.session_id.clone().unwrap_or_else(|| "—".into()),
                );
                row("repo", run.repo_path.clone());
                row("cwd", run.cwd.clone());
                row("worktree", run.worktree.to_string());
                row(
                    "branch",
                    run.branch.clone().unwrap_or_else(|| "—".into()),
                );
                row(
                    "base commit",
                    run.base_sha.clone().unwrap_or_else(|| "—".into()),
                );
                row("permission mode", run.permission_mode.as_cli().to_string());
                row("created", run.created_at.to_rfc3339());
                row(
                    "ended",
                    run.ended_at.map(|t| t.to_rfc3339()).unwrap_or_else(|| "—".into()),
                );

                ui.add_space(8.0);
                ui.label(dim("full intent"));
                ui.label(RichText::new(&run.intent).size(12.5));
            });
    }
}

fn diff_line(ui: &mut egui::Ui, line: &str) {
    let text = RichText::new(line).monospace().size(11.5);
    let styled = match line.chars().next() {
        Some('+') => text.color(Color32::from_rgb(0x8F, 0xE0, 0xA6)).background_color(ADD_BG),
        Some('-') => text.color(Color32::from_rgb(0xF0, 0x9C, 0xA0)).background_color(DEL_BG),
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
        EventKind::ToolUse {
            name, summary, ..
        } => {
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
// New run
// ---------------------------------------------------------------------------

impl App {
    fn new_run_window(&mut self, root: &mut egui::Ui) {
        if !self.show_new {
            return;
        }
        let ctx = root.ctx().clone();
        let mut open = true;
        let mut start = false;
        egui::Window::new("New run")
            .open(&mut open)
            .default_width(560.0)
            .collapsible(false)
            .show(&ctx, |ui| {
                ui.label(dim("repository"));
                egui::ComboBox::from_id_salt("repo")
                    .selected_text(if self.new_repo.is_empty() {
                        "— none registered —".to_string()
                    } else {
                        self.new_repo.clone()
                    })
                    .width(ui.available_width() - 20.0)
                    .show_ui(ui, |ui| {
                        for r in &self.repos {
                            ui.selectable_value(&mut self.new_repo, r.clone(), r);
                        }
                    });

                ui.horizontal(|ui| {
                    ui.add(
                        egui::TextEdit::singleline(&mut self.add_repo_path)
                            .hint_text("add a repo path, e.g. ~/projects/foo")
                            .desired_width(ui.available_width() - 70.0),
                    );
                    if ui.button("Add").clicked() && !self.add_repo_path.trim().is_empty() {
                        self.net.send(ClientMsg::AddRepo {
                            path: self.add_repo_path.trim().to_string(),
                        });
                        self.add_repo_path.clear();
                    }
                });

                ui.add_space(8.0);
                ui.label(dim("what should the agent do?"));
                ui.add(
                    egui::TextEdit::multiline(&mut self.new_intent)
                        .desired_rows(6)
                        .desired_width(f32::INFINITY),
                );

                ui.add_space(8.0);
                ui.horizontal(|ui| {
                    ui.label(dim("model"));
                    ui.add(
                        egui::TextEdit::singleline(&mut self.new_model)
                            .desired_width(120.0)
                            .hint_text("sonnet"),
                    );
                    ui.separator();
                    ui.label(dim("permission"));
                    egui::ComboBox::from_id_salt("perm")
                        .selected_text(self.new_perm.as_cli())
                        .show_ui(ui, |ui| {
                            for m in PermissionMode::ALL {
                                ui.selectable_value(&mut self.new_perm, m, m.as_cli());
                            }
                        });
                });

                ui.checkbox(
                    &mut self.new_worktree,
                    "run in a dedicated git worktree (recommended)",
                );
                ui.label(dim(
                    "a worktree isolates this run so parallel agents cannot collide",
                ));

                ui.add_space(10.0);
                ui.horizontal(|ui| {
                    let ready = !self.new_repo.is_empty() && !self.new_intent.trim().is_empty();
                    if ui.add_enabled(ready, egui::Button::new("Start run")).clicked() {
                        start = true;
                    }
                    if ui.button("Cancel").clicked() {
                        self.show_new = false;
                    }
                });
            });

        if start {
            self.net.send(ClientMsg::StartRun(NewRunSpec {
                repo_path: self.new_repo.clone(),
                intent: self.new_intent.trim().to_string(),
                model: if self.new_model.trim().is_empty() {
                    None
                } else {
                    Some(self.new_model.trim().to_string())
                },
                permission_mode: self.new_perm,
                worktree: self.new_worktree,
            }));
            self.new_intent.clear();
            self.show_new = false;
        }
        if !open {
            self.show_new = false;
        }
    }
}

/// Unused today, kept so the compiler reminds us if RunStatus grows a variant
/// the UI has no colour for.
#[allow(dead_code)]
fn _exhaustive(s: RunStatus) -> Color32 {
    status_color(s)
}
