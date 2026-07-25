//! Small shared presentation helpers: colours, badges, formatting, and the
//! "open this somewhere else" actions.

use egui::{Color32, RichText};
use mogeung_core::attention::AttentionReason;
use mogeung_core::change::RiskLevel;


pub const RED: Color32 = Color32::from_rgb(0xE5, 0x48, 0x4F);
pub const AMBER: Color32 = Color32::from_rgb(0xE0, 0x9B, 0x24);
pub const BLUE: Color32 = Color32::from_rgb(0x4C, 0x8E, 0xDA);
pub const GREEN: Color32 = Color32::from_rgb(0x3F, 0xA8, 0x5E);
pub const DIM: Color32 = Color32::from_rgb(0x8A, 0x8A, 0x90);
pub const PURPLE: Color32 = Color32::from_rgb(0x9A, 0x6F, 0xD0);

/// Diff line tints, kept dark enough to read white text on in the default theme.
pub const ADD_BG: Color32 = Color32::from_rgb(0x14, 0x3A, 0x21);
pub const DEL_BG: Color32 = Color32::from_rgb(0x45, 0x1A, 0x1D);

pub fn reason_color(r: AttentionReason) -> Color32 {
    match r {
        AttentionReason::AwaitingInput => RED,
        AttentionReason::Failed => RED,
        AttentionReason::NeedsReview => AMBER,
        AttentionReason::Stalled => PURPLE,
        AttentionReason::Running => BLUE,
        AttentionReason::Idle => DIM,
    }
}

pub fn risk_color(r: RiskLevel) -> Color32 {
    match r {
        RiskLevel::High => RED,
        RiskLevel::Medium => AMBER,
        RiskLevel::Low => DIM,
        RiskLevel::Noise => Color32::from_rgb(0x5A, 0x5A, 0x60),
    }
}

pub fn badge(text: &str, color: Color32) -> RichText {
    RichText::new(format!(" {text} "))
        .color(Color32::WHITE)
        .background_color(color)
        .monospace()
        .size(11.0)
}

pub fn dim(text: impl Into<String>) -> RichText {
    RichText::new(text.into()).color(DIM).size(12.0)
}

pub fn mono(text: impl Into<String>) -> RichText {
    RichText::new(text.into()).monospace().size(12.0)
}

pub fn truncate(s: &str, n: usize) -> String {
    let one_line: String = s.chars().map(|c| if c == '\n' { ' ' } else { c }).collect();
    if one_line.chars().count() <= n {
        return one_line;
    }
    let cut: String = one_line.chars().take(n).collect();
    format!("{cut}…")
}

pub fn money(v: f64) -> String {
    if v >= 1.0 {
        format!("${v:.2}")
    } else {
        format!("{:.1}¢", v * 100.0)
    }
}

pub fn tokens(n: u64) -> String {
    if n >= 1_000_000 {
        format!("{:.1}M", n as f64 / 1e6)
    } else if n >= 1_000 {
        format!("{:.1}k", n as f64 / 1e3)
    } else {
        n.to_string()
    }
}

/// Where a run can be handed off to. Opening the real editor is a first-class
/// action, not a fallback — mogeung is not trying to replace it (CONCEPT.md F3).
#[derive(Clone, Copy, PartialEq)]
pub enum OpenTarget {
    Intellij,
    VsCode,
    Finder,
    Terminal,
}

impl OpenTarget {
    pub fn label(&self) -> &'static str {
        match self {
            OpenTarget::Intellij => "IntelliJ",
            OpenTarget::VsCode => "VS Code",
            OpenTarget::Finder => "Finder",
            OpenTarget::Terminal => "Terminal",
        }
    }
}

/// Best-effort launch. Returns an error string for the UI to surface rather
/// than failing silently, because a missing editor is a real thing to tell the
/// user about.
pub fn open_in(target: OpenTarget, path: &str) -> Result<(), String> {
    let spawn = |program: &str, args: &[&str]| -> Result<(), String> {
        std::process::Command::new(program)
            .args(args)
            .spawn()
            .map(|_| ())
            .map_err(|e| format!("{program}: {e}"))
    };

    match target {
        OpenTarget::Finder => spawn("open", &[path]),
        OpenTarget::Terminal => spawn("open", &["-a", "Terminal", path]),
        OpenTarget::VsCode => {
            // Prefer the CLI, fall back to the bundle if `code` is not installed.
            spawn("code", &[path]).or_else(|_| spawn("open", &["-a", "Visual Studio Code", path]))
        }
        OpenTarget::Intellij => spawn("idea", &[path])
            .or_else(|_| spawn("open", &["-a", "IntelliJ IDEA", path]))
            .or_else(|_| spawn("open", &["-a", "IntelliJ IDEA Ultimate", path])),
    }
}
