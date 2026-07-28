//! Small shared presentation helpers: colours, badges, formatting, and the
//! "open this somewhere else" actions.

use egui::{Color32, RichText};
use mogeung_core::attention::AttentionReason;
use mogeung_core::change::RiskLevel;


// ---------------------------------------------------------------------------
// Theme
// ---------------------------------------------------------------------------
//
// One place that decides what the app looks like.
//
// Before this, the window was egui's stock dark theme with our own colours
// sprinkled on top, which is why it read as "a Rust GUI" rather than as a
// designed thing: stock radii, stock spacing, stock type sizes, and greys that
// had no relationship to the accent colours sitting on them.
//
// The surfaces are a deliberate three-step ladder — window, panel, raised —
// so depth comes from value rather than from borders. Borders are then free to
// mean something (focus, selection) instead of merely drawing boxes.

/// Behind everything.
pub const BG: Color32 = Color32::from_rgb(0x14, 0x15, 0x18);
/// Panels and the top bar.
pub const BG_PANEL: Color32 = Color32::from_rgb(0x1A, 0x1C, 0x20);
/// Cards and popups: the layer that sits *on* a panel.
pub const BG_RAISED: Color32 = Color32::from_rgb(0x22, 0x25, 0x2A);
/// Hover, and zebra striping.
pub const BG_FAINT: Color32 = Color32::from_rgb(0x2A, 0x2D, 0x33);
/// Body text.
pub const TEXT: Color32 = Color32::from_rgb(0xD4, 0xD7, 0xDD);
/// Headings and anything that must win the eye.
pub const TEXT_STRONG: Color32 = Color32::from_rgb(0xF0, 0xF2, 0xF6);

pub const RED: Color32 = Color32::from_rgb(0xE5, 0x48, 0x4F);
pub const AMBER: Color32 = Color32::from_rgb(0xE0, 0x9B, 0x24);
pub const BLUE: Color32 = Color32::from_rgb(0x4C, 0x8E, 0xDA);
pub const GREEN: Color32 = Color32::from_rgb(0x3F, 0xA8, 0x5E);
pub const DIM: Color32 = Color32::from_rgb(0x8A, 0x8A, 0x90);
pub const PURPLE: Color32 = Color32::from_rgb(0x9A, 0x6F, 0xD0);

/// Diff line tints, kept dark enough to read white text on in the default theme.
pub const ADD_BG: Color32 = Color32::from_rgb(0x14, 0x3A, 0x21);
pub const DEL_BG: Color32 = Color32::from_rgb(0x45, 0x1A, 0x1D);

/// The badge colours a user label can land on. `R-B26`.
///
/// Deliberately none of the semantic colours: a label must not be mistakable
/// for a state (`RED` = waiting, `AMBER` = needs you), so these hues sit
/// between them — all dark enough to carry the badge's white text.
const LABEL_COLORS: [Color32; 8] = [
    Color32::from_rgb(0x2E, 0x8B, 0x8B), // teal
    Color32::from_rgb(0x5B, 0x6A, 0xC4), // indigo
    Color32::from_rgb(0xB0, 0x4E, 0x8E), // magenta
    Color32::from_rgb(0x6E, 0x7F, 0x2A), // olive
    Color32::from_rgb(0x9E, 0x63, 0x38), // sienna
    Color32::from_rgb(0x4A, 0x7F, 0xB5), // slate blue
    Color32::from_rgb(0x3E, 0x86, 0x4F), // forest
    Color32::from_rgb(0x8E, 0x5B, 0xA6), // plum
];

/// A stable colour for a label: same text, same colour, on every card and
/// across restarts — hashed, not stored, so there is nothing to persist or
/// to pick. FNV-1a because it is three lines and this is not cryptography.
pub fn label_color(label: &str) -> Color32 {
    let mut h: u64 = 0xcbf2_9ce4_8422_2325;
    for b in label.as_bytes() {
        h ^= u64::from(*b);
        h = h.wrapping_mul(0x0000_0100_0000_01B3);
    }
    LABEL_COLORS[(h % LABEL_COLORS.len() as u64) as usize]
}

/// The one character a collapsed-queue chip wears: the first letter or digit,
/// uppercased — "UNITY1" is `U`, "the risky one" is `T`. Punctuation is
/// skipped so "~/scratch" does not become a `~` chip, and anything with no
/// honest character at all gets `?` rather than a blank square.
pub fn chip_char(name: &str) -> char {
    name.chars()
        .find(|c| c.is_alphanumeric())
        .map(|c| c.to_ascii_uppercase())
        .unwrap_or('?')
}

/// Install the theme. Called once, at start-up.
///
/// Everything here is a decision, not a default:
///
/// * **Type scale.** Five sizes with real gaps between them (10.5 / 12 / 13 /
///   15 / 19). egui's defaults sit too close together to establish hierarchy,
///   so everything looked equally important — which in a triage tool is the
///   opposite of the point.
/// * **Radius.** One value, 5px, on everything except windows (9px). Mixed
///   radii are the fastest way to make an interface look assembled rather than
///   designed.
/// * **Borders.** Nearly invisible at rest. A visible outline means focus or
///   selection and nothing else, so those states read instantly.
/// * **Spacing.** Slightly roomier than stock, because this is a reading tool.
pub fn apply_theme(ctx: &egui::Context) {
    use egui::{CornerRadius, FontFamily, FontId, Stroke, TextStyle};

    let mut visuals = egui::Visuals::dark();
    let radius = CornerRadius::same(5);

    visuals.panel_fill = BG_PANEL;
    visuals.window_fill = BG_RAISED;
    visuals.extreme_bg_color = BG;
    visuals.faint_bg_color = BG_FAINT;
    visuals.code_bg_color = BG;
    visuals.override_text_color = None;
    visuals.window_corner_radius = CornerRadius::same(9);
    visuals.window_stroke = Stroke::new(1.0, Color32::from_rgb(0x33, 0x37, 0x3E));
    visuals.hyperlink_color = BLUE;
    visuals.warn_fg_color = AMBER;
    visuals.error_fg_color = RED;

    // Selection is the accent, and the only saturated thing in the chrome.
    visuals.selection.bg_fill = BLUE.linear_multiply(0.55);
    visuals.selection.stroke = Stroke::new(1.0, TEXT_STRONG);

    let w = &mut visuals.widgets;
    w.noninteractive.bg_fill = BG_PANEL;
    w.noninteractive.weak_bg_fill = BG_PANEL;
    w.noninteractive.bg_stroke = Stroke::new(1.0, Color32::from_rgb(0x2C, 0x2F, 0x35));
    w.noninteractive.fg_stroke = Stroke::new(1.0, TEXT);
    w.noninteractive.corner_radius = radius;

    w.inactive.bg_fill = BG_RAISED;
    w.inactive.weak_bg_fill = BG_RAISED;
    w.inactive.bg_stroke = Stroke::NONE;
    w.inactive.fg_stroke = Stroke::new(1.0, TEXT);
    w.inactive.corner_radius = radius;

    w.hovered.bg_fill = BG_FAINT;
    w.hovered.weak_bg_fill = BG_FAINT;
    w.hovered.bg_stroke = Stroke::new(1.0, Color32::from_rgb(0x43, 0x48, 0x52));
    w.hovered.fg_stroke = Stroke::new(1.0, TEXT_STRONG);
    w.hovered.corner_radius = radius;
    // Stock egui grows a widget on hover. It reads as the layout twitching.
    w.hovered.expansion = 0.0;

    w.active.bg_fill = BLUE.linear_multiply(0.45);
    w.active.weak_bg_fill = BLUE.linear_multiply(0.45);
    w.active.bg_stroke = Stroke::new(1.0, BLUE);
    w.active.fg_stroke = Stroke::new(1.0, TEXT_STRONG);
    w.active.corner_radius = radius;
    w.active.expansion = 0.0;

    w.open.bg_fill = BG_FAINT;
    w.open.weak_bg_fill = BG_FAINT;
    w.open.corner_radius = radius;

    // Pinned to dark rather than following the system: the diff tints, the
    // reason badges and the risk colours are all chosen against a dark ground,
    // and a light theme would need its own set rather than the same values
    // reused.
    ctx.set_theme(egui::ThemePreference::Dark);
    ctx.set_visuals_of(egui::Theme::Dark, visuals);

    ctx.all_styles_mut(|style| {
        style.text_styles = [
            (TextStyle::Heading, FontId::new(19.0, FontFamily::Proportional)),
            (TextStyle::Body, FontId::new(13.0, FontFamily::Proportional)),
            (TextStyle::Button, FontId::new(12.5, FontFamily::Proportional)),
            (TextStyle::Small, FontId::new(10.5, FontFamily::Proportional)),
            (TextStyle::Monospace, FontId::new(12.0, FontFamily::Monospace)),
        ]
        .into();

        style.spacing.item_spacing = egui::vec2(7.0, 5.0);
        style.spacing.button_padding = egui::vec2(8.0, 3.0);
        style.spacing.menu_margin = egui::Margin::same(6);
        style.spacing.indent = 16.0;
        style.spacing.scroll.bar_width = 8.0;
        // A scrollbar that is only there while you are using it. The queue is
        // the permanent furniture; its scrollbar is not.
        style.spacing.scroll.floating = true;

        // Tooltips carry the keyboard hints, so they should appear promptly
        // rather than after the pause that reads as "nothing here".
        style.interaction.tooltip_delay = 0.25;
    });
}

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

    // -- status bar. Short, unambiguous, and each one earns its place by
    // replacing a word that was being repeated on every session.
    /// How long the session has been going.
    pub const CLOCK: &str = "🕐";
    /// Conversation turns.
    pub const TURNS: &str = "⇄";
    /// Tool calls.
    pub const TOOLS: &str = "⚙";
    /// Tokens out.
    pub const TOKENS: &str = "◆";
    /// Working directory.
    pub const FOLDER: &str = "🗀";

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
        ("CLOCK", CLOCK),
        ("TURNS", TURNS),
        ("TOOLS", TOOLS),
        ("TOKENS", TOKENS),
        ("FOLDER", FOLDER),
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
            // The file manager answers to its platform's name; "Finder" on
            // Linux is a button that reads as broken before it is clicked.
            #[cfg(target_os = "macos")]
            OpenTarget::Finder => "Finder",
            #[cfg(not(target_os = "macos"))]
            OpenTarget::Finder => "Files",
            OpenTarget::Terminal => "Terminal",
        }
    }
}

/// One way a target might launch: program, arguments, and an optional
/// working directory (for terminals whose only "open here" is inheritance).
type Attempt = (String, Vec<String>, Option<String>);

/// The launch attempts for a target, in order — the first program that
/// spawns wins. Split from the spawning so the per-platform tables are
/// testable on any platform; the app began life on macOS, and every one of
/// these buttons was quietly `open -a` until Linux dogfooding found them.
fn attempts(target: OpenTarget, path: &str) -> Vec<Attempt> {
    let p = path.to_string();
    #[cfg(target_os = "macos")]
    return match target {
        OpenTarget::Finder => vec![("open".to_string(), vec![p], None)],
        OpenTarget::Terminal => vec![("open".to_string(), vec!["-a".into(), "Terminal".into(), p], None)],
        // Prefer the CLI, fall back to the bundle if `code` is not installed.
        OpenTarget::VsCode => vec![
            ("code".to_string(), vec![p.clone()], None),
            ("open".to_string(), vec!["-a".into(), "Visual Studio Code".into(), p], None),
        ],
        OpenTarget::Intellij => vec![
            ("idea".to_string(), vec![p.clone()], None),
            ("open".to_string(), vec!["-a".into(), "IntelliJ IDEA".into(), p.clone()], None),
            ("open".to_string(), vec!["-a".into(), "IntelliJ IDEA Ultimate".into(), p], None),
        ],
    };
    #[cfg(not(target_os = "macos"))]
    match target {
        OpenTarget::Finder => vec![("xdg-open".to_string(), vec![p], None)],
        // No portable "terminal here" exists; walk the common ones. The
        // Debian alternatives symlink goes first — it is the user's own
        // choice — and takes its directory by inheritance, having no flag.
        OpenTarget::Terminal => vec![
            ("x-terminal-emulator".to_string(), vec![], Some(p.clone())),
            ("gnome-terminal".to_string(), vec![format!("--working-directory={p}")], None),
            ("konsole".to_string(), vec!["--workdir".into(), p.clone()], None),
            ("xfce4-terminal".to_string(), vec![format!("--working-directory={p}")], None),
            ("alacritty".to_string(), vec!["--working-directory".into(), p.clone()], None),
            ("xterm".to_string(), vec![], Some(p)),
        ],
        OpenTarget::VsCode => vec![("code".to_string(), vec![p], None)],
        // JetBrains installs land differently by channel: `idea` on PATH
        // when the user set Toolbox's shell scripts up, the Toolbox scripts
        // directory when they did not (found live: Toolbox installed, PATH
        // untouched), and the long names for snaps.
        OpenTarget::Intellij => {
            let toolbox = format!(
                "{}/.local/share/JetBrains/Toolbox/scripts/idea",
                std::env::var("HOME").unwrap_or_default()
            );
            vec![
                ("idea".to_string(), vec![p.clone()], None),
                (toolbox, vec![p.clone()], None),
                ("intellij-idea-ultimate".to_string(), vec![p.clone()], None),
                ("intellij-idea-community".to_string(), vec![p], None),
            ]
        }
    }
}

/// Best-effort launch. Returns an error string for the UI to surface rather
/// than failing silently, because a missing editor is a real thing to tell the
/// user about.
pub fn open_in(target: OpenTarget, path: &str) -> Result<(), String> {
    let list = attempts(target, path);
    let mut tried = Vec::new();
    for (program, args, cwd) in &list {
        let mut cmd = std::process::Command::new(program);
        cmd.args(args);
        if let Some(dir) = cwd {
            cmd.current_dir(dir);
        }
        match cmd.spawn() {
            Ok(mut child) => {
                // Reaped off-frame, or every launch leaves a zombie behind
                // (one `[open] <defunct>` was found live doing exactly that).
                std::thread::spawn(move || {
                    let _ = child.wait();
                });
                return Ok(());
            }
            Err(_) => tried.push(program.as_str()),
        }
    }
    Err(format!(
        "none of these launchers exist here: {}",
        tried.join(", ")
    ))
}

#[cfg(test)]
mod tests {
    use super::icon;
    use super::{attempts, OpenTarget};

    /// The handoff buttons must speak the platform they run on — every
    /// target needs at least one attempt, none may reach for the other
    /// OS's launcher, and a terminal with no workdir flag must get the
    /// directory by inheritance instead.
    #[test]
    fn open_targets_have_native_launchers() {
        for target in [
            OpenTarget::Finder,
            OpenTarget::Terminal,
            OpenTarget::VsCode,
            OpenTarget::Intellij,
        ] {
            let list = attempts(target, "/some/repo");
            assert!(!list.is_empty());
            for (program, args, cwd) in &list {
                #[cfg(not(target_os = "macos"))]
                assert_ne!(*program, "open", "`open -a` is macOS-speak");
                // Every attempt must carry the path somewhere: an argument
                // mentioning it, or the working directory.
                let in_args = args.iter().any(|a| a.contains("/some/repo"));
                let in_cwd = cwd.as_deref() == Some("/some/repo");
                assert!(
                    in_args || in_cwd,
                    "{program} would launch without the path"
                );
            }
        }
        #[cfg(not(target_os = "macos"))]
        {
            assert_eq!(attempts(OpenTarget::Finder, "/x")[0].0, "xdg-open");
            let term = attempts(OpenTarget::Terminal, "/x");
            assert_eq!(term[0].0, "x-terminal-emulator");
            assert_eq!(term[0].2.as_deref(), Some("/x"), "no flag means inherit the cwd");
            assert_eq!(OpenTarget::Finder.label(), "Files");
        }
    }

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

    /// The whole contract of `label_color`: deterministic per text, always
    /// from the palette, and not accidentally constant.
    #[test]
    fn label_colors_are_stable_and_from_the_palette() {
        use super::{label_color, LABEL_COLORS};
        assert_eq!(label_color("risky"), label_color("risky"));
        assert!(LABEL_COLORS.contains(&label_color("anything at all")));
        let distinct: std::collections::HashSet<_> = ["a", "b", "c", "d", "e", "f"]
            .iter()
            .map(|l| label_color(l).to_array())
            .collect();
        assert!(distinct.len() > 1, "every label hashed to one colour");
    }

    /// The examples from the ask, plus the edges: leading punctuation is
    /// skipped, lowercase is raised, and nothing yields an empty chip.
    #[test]
    fn chip_chars_are_the_first_honest_letter() {
        use super::chip_char;
        assert_eq!(chip_char("UNITY1"), 'U');
        assert_eq!(chip_char("the risky one"), 'T');
        assert_eq!(chip_char("~/scratch"), 'S');
        assert_eq!(chip_char("42-experiments"), '4');
        assert_eq!(chip_char("—"), '?');
        assert_eq!(chip_char(""), '?');
    }
}
