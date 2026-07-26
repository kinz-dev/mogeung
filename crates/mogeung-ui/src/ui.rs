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
        // Blocked on a permission prompt: the most urgent thing on the board,
        // and visually distinct from "finished, waiting for a new task".
        AttentionReason::AwaitingPermission => Color32::from_rgb(0xFF, 0x6B, 0x35),
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

/// Glyphs that egui's bundled fonts can actually draw.
///
/// egui ships Ubuntu-Light, Hack, NotoEmoji and emoji-icon-font. Anything
/// outside their combined coverage renders as an empty box, **silently** — the
/// layout is fine, the click works, and only a human looking at the window can
/// tell. Four glyphs shipped that way before this list existed: `✎`, `⌁`, `✓`
/// and `⑂`, including the read-marker in the file list.
///
/// So icons come from here and nowhere else, and `icons_are_renderable` in
/// `tests` checks every one against the real font files. Adding a glyph
/// straight into a widget is how the boxes come back.
pub mod icon {
    pub const RESCAN: &str = "🔄";
    pub const AMBIENT: &str = "🖥";
    pub const NEW_SESSION: &str = "➕";
    pub const KEYBOARD: &str = "⌨";
    pub const HEALTH: &str = "👁";
    pub const WARN: &str = "⚠";
    pub const FLAG: &str = "⚑";
    pub const BLAST: &str = "🔍";
    pub const READ: &str = "✔";
    pub const UNREAD: &str = "●";
    pub const BRANCH: &str = "⎇";
    pub const CLIPBOARD: &str = "📋";
    pub const SNOOZE: &str = "🕐";
    pub const TERMINAL: &str = "→";
    /// Dismiss a finished session from the queue.
    pub const HIDE: &str = "✕";

    /// Every icon, for the test that proves they render.
    ///
    /// Unused outside tests by design — it exists so the check cannot drift
    /// from the constants above by someone adding one and forgetting the list.
    #[allow(dead_code)]
    pub const ALL: &[(&str, &str)] = &[
        ("RESCAN", RESCAN),
        ("AMBIENT", AMBIENT),
        ("NEW_SESSION", NEW_SESSION),
        ("KEYBOARD", KEYBOARD),
        ("HEALTH", HEALTH),
        ("WARN", WARN),
        ("FLAG", FLAG),
        ("BLAST", BLAST),
        ("READ", READ),
        ("UNREAD", UNREAD),
        ("BRANCH", BRANCH),
        ("CLIPBOARD", CLIPBOARD),
        ("SNOOZE", SNOOZE),
        ("TERMINAL", TERMINAL),
        ("HIDE", HIDE),
    ];
}

/// A square icon button with the tooltip carrying the words.
///
/// Icons alone are a guessing game, so every one of these gets a label *and*
/// its shortcut on hover. `active` draws it as engaged, for the toggles.
pub fn icon_button(
    ui: &mut egui::Ui,
    glyph: &str,
    tooltip: &str,
    active: bool,
    tint: Option<Color32>,
) -> egui::Response {
    let color = tint.unwrap_or(if active { Color32::WHITE } else { DIM });
    let text = RichText::new(glyph).size(15.0).color(color);
    let button = egui::Button::new(text)
        .min_size(egui::vec2(28.0, 24.0))
        .frame(active || tint.is_some());
    ui.add(button).on_hover_text(tooltip)
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

#[cfg(test)]
mod tests {
    use super::icon;

    /// Read the cmap tables of egui's bundled fonts and assert every icon is
    /// covered.
    ///
    /// This is the only way to catch the failure, because a missing glyph does
    /// not error, warn or misbehave — it draws an empty box, and nothing short
    /// of a human looking at the window notices. Four such glyphs shipped
    /// before this existed.
    ///
    /// Parsing the fonts here rather than trusting a hand-kept list means the
    /// check stays true across an egui upgrade that changes the bundled fonts.
    #[test]
    fn icons_are_renderable() {
        let fonts = font_files();
        assert!(
            !fonts.is_empty(),
            "found no bundled fonts to check against — the vendored path moved"
        );

        let mut covered = std::collections::HashSet::new();
        for f in &fonts {
            covered.extend(cmap_codepoints(f));
        }

        let mut missing = Vec::new();
        for (name, glyph) in icon::ALL {
            for ch in glyph.chars() {
                if !covered.contains(&(ch as u32)) {
                    missing.push(format!("{name} ({glyph}, U+{:04X})", ch as u32));
                }
            }
        }
        assert!(
            missing.is_empty(),
            "these icons would render as empty boxes: {}",
            missing.join(", ")
        );
    }

    fn font_files() -> Vec<std::path::PathBuf> {
        // epaint's default fonts live beside it in the cargo registry.
        let Ok(reg) = std::env::var("CARGO_HOME")
            .map(std::path::PathBuf::from)
            .or_else(|_| std::env::var("HOME").map(|h| std::path::PathBuf::from(h).join(".cargo")))
        else {
            return Vec::new();
        };
        let src = reg.join("registry").join("src");
        let mut out = Vec::new();
        let Ok(hosts) = std::fs::read_dir(&src) else {
            return out;
        };
        for host in hosts.flatten() {
            let Ok(crates) = std::fs::read_dir(host.path()) else {
                continue;
            };
            for c in crates.flatten() {
                let name = c.file_name();
                let name = name.to_string_lossy();
                if !name.starts_with("epaint_default_fonts-") {
                    continue;
                }
                let Ok(files) = std::fs::read_dir(c.path().join("fonts")) else {
                    continue;
                };
                for f in files.flatten() {
                    if f.path().extension().and_then(|e| e.to_str()) == Some("ttf") {
                        out.push(f.path());
                    }
                }
            }
        }
        out
    }

    /// Minimal TrueType cmap reader: formats 4 and 12 are all these fonts use.
    fn cmap_codepoints(path: &std::path::Path) -> std::collections::HashSet<u32> {
        let mut out = std::collections::HashSet::new();
        let Ok(d) = std::fs::read(path) else {
            return out;
        };
        let u16at = |d: &[u8], i: usize| u16::from_be_bytes([d[i], d[i + 1]]) as usize;
        let u32at = |d: &[u8], i: usize| {
            u32::from_be_bytes([d[i], d[i + 1], d[i + 2], d[i + 3]]) as usize
        };
        if d.len() < 12 {
            return out;
        }

        let num_tables = u16at(&d, 4);
        let mut cmap = None;
        for i in 0..num_tables {
            let off = 12 + i * 16;
            if off + 16 > d.len() {
                return out;
            }
            if &d[off..off + 4] == b"cmap" {
                cmap = Some(u32at(&d, off + 8));
            }
        }
        let Some(cmap) = cmap else { return out };
        if cmap + 4 > d.len() {
            return out;
        }

        for i in 0..u16at(&d, cmap + 2) {
            let rec = cmap + 4 + i * 8;
            if rec + 8 > d.len() {
                break;
            }
            let sub = cmap + u32at(&d, rec + 4);
            if sub + 4 > d.len() {
                continue;
            }
            match u16at(&d, sub) {
                4 => {
                    let seg = u16at(&d, sub + 6) / 2;
                    for s in 0..seg {
                        let end = u16at(&d, sub + 14 + s * 2);
                        let start = u16at(&d, sub + 16 + seg * 2 + s * 2);
                        if end == 0xFFFF {
                            continue;
                        }
                        for c in start..=end {
                            out.insert(c as u32);
                        }
                    }
                }
                12 => {
                    let groups = u32at(&d, sub + 12);
                    for g in 0..groups {
                        let o = sub + 16 + g * 12;
                        if o + 12 > d.len() {
                            break;
                        }
                        let (start, end) = (u32at(&d, o), u32at(&d, o + 4));
                        // Guard against a corrupt range eating all memory.
                        if end < start || end - start > 0x10000 {
                            continue;
                        }
                        for c in start..=end {
                            out.insert(c as u32);
                        }
                    }
                }
                _ => {}
            }
        }
        out
    }

    /// The specific glyphs that shipped as boxes. Named so a regression points
    /// at the history rather than just failing.
    #[test]
    fn the_glyphs_that_shipped_broken_stay_out() {
        let mut covered = std::collections::HashSet::new();
        for f in font_files() {
            covered.extend(cmap_codepoints(&f));
        }
        if covered.is_empty() {
            return; // fonts not vendored here; the other test reports that
        }
        for bad in ['✎', '⌁', '✓', '⑂'] {
            assert!(
                !covered.contains(&(bad as u32)),
                "{bad} renders now — it can go back into the icon list"
            );
        }
    }
}
