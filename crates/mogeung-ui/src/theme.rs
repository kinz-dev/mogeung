//! The two palettes, and the switch between them. `R-J6`.
//!
//! # Why a lookup rather than constants
//!
//! Every colour used to be a `const` in `ui.rs`, read directly at 232 call
//! sites. That is the right shape for one theme and impossible for two: a
//! `const` cannot ask what theme is running. The names survived the change —
//! `RED` became `pal().red` — so the diff is mechanical and the call sites
//! read the same.
//!
//! The active palette is process-global. One window means one theme, and the
//! alternative is threading a `&Palette` through every drawing function in the
//! app for a value that is the same everywhere.
//!
//! # Why two hand-written palettes and not one, transformed
//!
//! Inverting lightness is the obvious shortcut and it produces the washed-out
//! look that gives "light mode added later" away. The failures are specific:
//! amber at `#E09B24` is 2.2:1 on white and unreadable; a diff's dark green
//! tint becomes a garish mint; a 55%-multiplied blue selection turns into a
//! navy block. Each of those wants a different decision, not a different
//! formula, so both palettes are written out and the tests check the pairs
//! that have to hold.

use egui::Color32;
use std::sync::atomic::{AtomicU8, Ordering};

/// Which palette the window is drawing with.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Mode {
    /// The original, and still the default: this is a tool for watching
    /// terminals, and it sits next to them.
    #[default]
    Dark,
    Light,
    /// Whatever the desktop says, falling back to dark when it says nothing —
    /// which is common on Linux, where the answer comes from a portal that may
    /// not be running.
    System,
}

impl Mode {
    pub const ALL: [Mode; 3] = [Mode::Dark, Mode::Light, Mode::System];

    pub fn label(self) -> &'static str {
        match self {
            Mode::Dark => "dark",
            Mode::Light => "light",
            Mode::System => "system",
        }
    }

    /// The palette this mode resolves to. `system` is the only one that needs
    /// asking, and the answer is `None` whenever the desktop will not say.
    pub fn resolve(self, system: Option<egui::Theme>) -> Theme {
        match self {
            Mode::Dark => Theme::Dark,
            Mode::Light => Theme::Light,
            Mode::System => match system {
                Some(egui::Theme::Light) => Theme::Light,
                _ => Theme::Dark,
            },
        }
    }
}

/// A resolved theme — what `Mode` becomes once the desktop has been asked.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Theme {
    Dark,
    Light,
}

/// Every colour the window draws with.
///
/// Flat and explicit. A nested structure would read better and be worse to
/// use: these are referenced from drawing code where `pal().add_bg` beats
/// `pal().diff.background.added` at every one of the sites that reads it.
#[derive(Debug, Clone, Copy)]
pub struct Palette {
    // Surfaces, in ladder order. Depth comes from value, so borders are free
    // to mean focus and selection instead of merely drawing boxes.
    pub bg: Color32,
    pub bg_panel: Color32,
    pub bg_raised: Color32,
    /// Hover and zebra striping. Brighter than the surface under dark, darker
    /// under light — "further forward" is the constant, not the direction.
    pub bg_faint: Color32,

    pub text: Color32,
    pub text_strong: Color32,
    pub dim: Color32,

    pub red: Color32,
    pub amber: Color32,
    pub blue: Color32,
    pub green: Color32,
    pub purple: Color32,

    // Chrome.
    pub border: Color32,
    pub border_hover: Color32,
    pub window_stroke: Color32,
    /// Selection fill. Not a multiplied accent: under light that is a navy
    /// block with black text on it.
    pub selection_bg: Color32,
    pub selection_stroke: Color32,
    /// Lettering for a *dark* filled badge, and the brighter half of the pair
    /// `on_accent_for` chooses between.
    pub on_accent: Color32,
    /// Lettering for a *light* filled badge. Amber is why this exists: white
    /// on `#E09B24` is 2.36:1, which shipped, and is the one badge on the
    /// board nobody could read.
    pub on_accent_dark: Color32,

    // Diff.
    pub add_bg: Color32,
    pub del_bg: Color32,
    pub add_fg: Color32,
    pub del_fg: Color32,
    pub ctx_fg: Color32,
    /// The word-diff emphasis, sitting *on* `add_bg`/`del_bg` — so it has to
    /// be a step further from the page, not merely a different hue.
    pub add_emph: Color32,
    pub del_emph: Color32,
    /// Conflict markers get their own band regardless of diff sign.
    pub conflict_bg: Color32,

    // Syntax, for the diff's own tokenizer.
    pub syn_keyword: Color32,
    pub syn_string: Color32,
    pub syn_comment: Color32,
    pub syn_number: Color32,
    pub syn_type: Color32,

    // The rest.
    /// Blocked on a permission prompt: the most urgent thing on the board.
    pub urgent: Color32,
    /// Risk so low it is noise — quieter than `dim`, and the only colour whose
    /// job is to recede.
    pub noise: Color32,
    /// File rows in the changes list: read files step back, unread step
    /// forward, and the pair has to stay distinguishable in both palettes.
    pub read_dim: Color32,
    pub read_base: Color32,
    pub unread_base: Color32,
    /// Lane colours for the commit graph, cycled.
    pub graph: [Color32; 6],
}

const fn rgb(r: u8, g: u8, b: u8) -> Color32 {
    Color32::from_rgb(r, g, b)
}

/// The original palette, unchanged by `R-J6` — every value here is what the
/// window shipped with, so the light theme cannot have altered the dark one.
pub const DARK: Palette = Palette {
    bg: rgb(0x14, 0x15, 0x18),
    bg_panel: rgb(0x1A, 0x1C, 0x20),
    bg_raised: rgb(0x22, 0x25, 0x2A),
    bg_faint: rgb(0x2A, 0x2D, 0x33),

    text: rgb(0xD4, 0xD7, 0xDD),
    text_strong: rgb(0xF0, 0xF2, 0xF6),
    dim: rgb(0x8A, 0x8A, 0x90),

    red: rgb(0xE5, 0x48, 0x4F),
    amber: rgb(0xE0, 0x9B, 0x24),
    blue: rgb(0x4C, 0x8E, 0xDA),
    green: rgb(0x3F, 0xA8, 0x5E),
    purple: rgb(0x9A, 0x6F, 0xD0),

    border: rgb(0x2C, 0x2F, 0x35),
    border_hover: rgb(0x43, 0x48, 0x52),
    window_stroke: rgb(0x33, 0x37, 0x3E),
    selection_bg: rgb(0x2A, 0x4E, 0x78),
    selection_stroke: rgb(0xF0, 0xF2, 0xF6),
    on_accent: Color32::WHITE,
    on_accent_dark: rgb(0x12, 0x14, 0x1A),

    add_bg: rgb(0x14, 0x3A, 0x21),
    del_bg: rgb(0x45, 0x1A, 0x1D),
    add_fg: rgb(0x8F, 0xE0, 0xA6),
    del_fg: rgb(0xF0, 0x9C, 0xA0),
    ctx_fg: rgb(0xA8, 0xA8, 0xB0),
    add_emph: rgb(0x25, 0x6B, 0x3C),
    del_emph: rgb(0x74, 0x2B, 0x30),
    conflict_bg: rgb(0x6B, 0x25, 0x3C),

    syn_keyword: rgb(0xC5, 0x92, 0xE8),
    syn_string: rgb(0xB6, 0xD7, 0x8B),
    syn_comment: rgb(0x77, 0x7C, 0x88),
    syn_number: rgb(0xE8, 0xB0, 0x75),
    syn_type: rgb(0x7E, 0xC0, 0xE0),

    urgent: rgb(0xFF, 0x6B, 0x35),
    noise: rgb(0x5A, 0x5A, 0x60),
    read_dim: rgb(0x60, 0x60, 0x60),
    read_base: rgb(0x7A, 0x7A, 0x7A),
    unread_base: rgb(0xDC, 0xDC, 0xDC),
    graph: [
        rgb(0x4C, 0x8E, 0xDA),
        rgb(0x3F, 0xA8, 0x5E),
        rgb(0xE0, 0x9B, 0x24),
        rgb(0xB0, 0x6A, 0xD8),
        rgb(0x3F, 0xB3, 0xB3),
        rgb(0xD8, 0x6A, 0x9A),
    ],
};

/// The light palette.
///
/// Not a mirror of `DARK`. Two rules shaped it:
///
/// * **Accents darken.** A hue that reads bright against near-black is
///   illegible against near-white. Amber moves furthest — `#E09B24` is 2.2:1
///   on white, and the replacement is nearly brown at small sizes because the
///   alternative is a warning nobody can read.
/// * **The surface ladder inverts direction but not meaning.** Raised
///   surfaces stay *closer to the page's extreme* — white here, as
///   near-black there — and hover moves away from it in both.
///
/// The page is a warm off-white rather than pure white: this tool is read for
/// long stretches beside a terminal, and full-brightness white next to a dark
/// terminal is the thing people turn light mode off again to escape.
pub const LIGHT: Palette = Palette {
    bg: rgb(0xE8, 0xEA, 0xEE),
    bg_panel: rgb(0xF7, 0xF8, 0xFA),
    bg_raised: rgb(0xFF, 0xFF, 0xFF),
    bg_faint: rgb(0xE4, 0xE7, 0xEC),

    text: rgb(0x25, 0x28, 0x2E),
    text_strong: rgb(0x0D, 0x0F, 0x14),
    // 4.6:1 on the panel. Dim must still be *readable* — it marks secondary
    // information, not decoration.
    dim: rgb(0x67, 0x6C, 0x76),

    red: rgb(0xC0, 0x25, 0x2C),
    amber: rgb(0x8A, 0x5A, 0x00),
    blue: rgb(0x1B, 0x64, 0xB0),
    green: rgb(0x1B, 0x6E, 0x3C),
    purple: rgb(0x66, 0x3F, 0xA6),

    border: rgb(0xD5, 0xD9, 0xE0),
    border_hover: rgb(0xAE, 0xB5, 0xC0),
    window_stroke: rgb(0xC6, 0xCB, 0xD4),
    selection_bg: rgb(0xC5, 0xDC, 0xF5),
    selection_stroke: rgb(0x1B, 0x64, 0xB0),
    on_accent: Color32::WHITE,
    on_accent_dark: rgb(0x12, 0x14, 0x1A),

    add_bg: rgb(0xDD, 0xF4, 0xE3),
    del_bg: rgb(0xFB, 0xDF, 0xE1),
    add_fg: rgb(0x11, 0x50, 0x2C),
    del_fg: rgb(0x8C, 0x1D, 0x25),
    ctx_fg: rgb(0x3C, 0x40, 0x48),
    add_emph: rgb(0xA8, 0xE6, 0xBD),
    del_emph: rgb(0xF6, 0xB9, 0xBD),
    conflict_bg: rgb(0xF9, 0xCF, 0xDC),

    syn_keyword: rgb(0x7B, 0x28, 0xA8),
    syn_string: rgb(0x25, 0x62, 0x1B),
    syn_comment: rgb(0x6B, 0x70, 0x7B),
    syn_number: rgb(0x99, 0x4E, 0x00),
    syn_type: rgb(0x0E, 0x5E, 0x7E),

    urgent: rgb(0xC4, 0x42, 0x08),
    noise: rgb(0x99, 0x9E, 0xA8),
    read_dim: rgb(0x93, 0x98, 0xA2),
    read_base: rgb(0x74, 0x79, 0x83),
    unread_base: rgb(0x1B, 0x1E, 0x24),
    graph: [
        rgb(0x1B, 0x64, 0xB0),
        rgb(0x1B, 0x6E, 0x3C),
        rgb(0x8A, 0x5A, 0x00),
        rgb(0x76, 0x39, 0xA8),
        // Pushed clear of both blue and green: the first attempt put teal 76
        // channel-steps from blue, and two lanes that close cannot be told
        // apart where they cross.
        rgb(0x00, 0x80, 0x7F),
        rgb(0xA8, 0x33, 0x6C),
    ],
};

/// Lettering for a badge filled with `fill`: whichever of the palette's two
/// badge inks can actually be read on it.
///
/// Fixed white was the rule before `R-J6`, and it was wrong for one fill in
/// the palette we already had — amber, at 2.36:1, used by `NeedsReview` and
/// `RateLimited`, which is to say by the two badges most likely to be on
/// screen. Choosing by luminance fixes that and stops the next accent from
/// reintroducing it.
pub fn on_accent_for(fill: Color32) -> Color32 {
    let p = pal();
    // 0.30, not "whichever contrasts more". Maximum contrast would flip almost
    // every dark badge to dark ink — the dark accents are mid-tone, so dark
    // lettering wins on all of them — and that is a redesign of a theme this
    // work was not asked to touch. This line catches only the fills where
    // white genuinely fails: amber (2.36:1) and urgent (2.84:1).
    if relative_luminance(fill) > 0.30 {
        p.on_accent_dark
    } else {
        p.on_accent
    }
}

/// WCAG relative luminance. Used by `on_accent_for`, and by the tests to
/// check every pair this palette promises.
pub fn relative_luminance(c: Color32) -> f64 {
    fn channel(v: u8) -> f64 {
        let v = v as f64 / 255.0;
        if v <= 0.03928 {
            v / 12.92
        } else {
            ((v + 0.055) / 1.055).powf(2.4)
        }
    }
    0.2126 * channel(c.r()) + 0.7152 * channel(c.g()) + 0.0722 * channel(c.b())
}

/// 0 = dark, 1 = light. An atomic rather than a lock: this is read once per
/// colour per frame, from one thread, and a `Mutex` on that path would be a
/// contention story with no contention in it.
static ACTIVE: AtomicU8 = AtomicU8::new(0);

/// The palette in force. Every colour in the window comes through here.
#[inline]
pub fn pal() -> &'static Palette {
    match ACTIVE.load(Ordering::Relaxed) {
        0 => &DARK,
        _ => &LIGHT,
    }
}

pub fn active() -> Theme {
    match ACTIVE.load(Ordering::Relaxed) {
        0 => Theme::Dark,
        _ => Theme::Light,
    }
}

/// Switch. Returns whether anything changed, so the caller can avoid rebuilding
/// egui's styles on every frame.
pub fn set(theme: Theme) -> bool {
    let want = match theme {
        Theme::Dark => 0,
        Theme::Light => 1,
    };
    ACTIVE.swap(want, Ordering::Relaxed) != want
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The WCAG contrast ratio, over the luminance the palette itself uses to
    /// pick badge lettering — so the tests and `on_accent_for` cannot disagree.
    fn contrast(a: Color32, b: Color32) -> f64 {
        let (x, y) = (relative_luminance(a), relative_luminance(b));
        let (hi, lo) = if x > y { (x, y) } else { (y, x) };
        (hi + 0.05) / (lo + 0.05)
    }

    /// The test that makes a second palette safe to have.
    ///
    /// A wrong colour in a theme nobody is looking at is invisible until
    /// someone switches, and "is this legible" is not something the compiler,
    /// a screenshot, or a reviewer skimming a diff of hex codes will catch.
    /// 4.5:1 is WCAG AA for body text; 3:1 is the large-text and non-text
    /// threshold, which is what the accents are used for — badges, one-word
    /// labels, and lines.
    #[test]
    fn text_is_legible_on_every_surface_in_both_palettes() {
        for (name, p) in [("dark", &DARK), ("light", &LIGHT)] {
            for (surface_name, surface) in [
                ("bg", p.bg),
                ("bg_panel", p.bg_panel),
                ("bg_raised", p.bg_raised),
                ("bg_faint", p.bg_faint),
            ] {
                for (fg_name, fg) in [("text", p.text), ("text_strong", p.text_strong)] {
                    let c = contrast(fg, surface);
                    assert!(
                        c >= 4.5,
                        "{name}: {fg_name} on {surface_name} is {c:.2}:1, below AA for body text"
                    );
                }
                // Dim is secondary, never decoration: it still has to be read.
                let c = contrast(p.dim, surface);
                assert!(c >= 4.0, "{name}: dim on {surface_name} is {c:.2}:1");
            }
        }
    }

    /// Amber is the one that catches people out — it is a warning, and the
    /// dark palette's value is 2.2:1 on white.
    #[test]
    fn every_accent_carries_against_its_own_panel() {
        for (name, p) in [("dark", &DARK), ("light", &LIGHT)] {
            for (accent_name, accent) in [
                ("red", p.red),
                ("amber", p.amber),
                ("blue", p.blue),
                ("green", p.green),
                ("purple", p.purple),
                ("urgent", p.urgent),
            ] {
                for (surface_name, surface) in
                    [("bg_panel", p.bg_panel), ("bg_raised", p.bg_raised)]
                {
                    let c = contrast(accent, surface);
                    assert!(
                        c >= 3.0,
                        "{name}: {accent_name} on {surface_name} is {c:.2}:1 — \
                         a state colour nobody can distinguish is not a state colour"
                    );
                }
            }
        }
    }

    /// A diff is the densest thing here, and its text sits on a tint rather
    /// than on a surface. Getting this wrong is how light themes end up with
    /// unreadable green-on-green additions.
    #[test]
    fn diff_text_reads_against_its_own_tint() {
        for (name, p) in [("dark", &DARK), ("light", &LIGHT)] {
            for (what, fg, bg) in [
                ("added", p.add_fg, p.add_bg),
                ("deleted", p.del_fg, p.del_bg),
                // The word-diff emphasis is a brighter band *inside* the line,
                // so the same text has to survive it.
                ("added+emphasis", p.add_fg, p.add_emph),
                ("deleted+emphasis", p.del_fg, p.del_emph),
                ("conflict", p.text_strong, p.conflict_bg),
            ] {
                let c = contrast(fg, bg);
                assert!(c >= 4.0, "{name}: {what} is {c:.2}:1");
            }
            // And the tints must be distinguishable from the page, or a diff
            // looks like plain text with coloured letters.
            for (what, tint) in [("add_bg", p.add_bg), ("del_bg", p.del_bg)] {
                let c = contrast(tint, p.bg_panel);
                assert!(c > 1.05, "{name}: {what} is invisible against the panel ({c:.3})");
            }
        }
    }

    /// Syntax colours are the easiest to leave dark-only, because a diff still
    /// looks *fine* with them — just muddy and low-contrast in a way that is
    /// hard to name if you are not looking for it.
    #[test]
    fn syntax_colours_read_on_the_diff_tints() {
        for (name, p) in [("dark", &DARK), ("light", &LIGHT)] {
            for (tok, c) in [
                ("keyword", p.syn_keyword),
                ("string", p.syn_string),
                ("comment", p.syn_comment),
                ("number", p.syn_number),
                ("type", p.syn_type),
            ] {
                for (surface_name, surface) in
                    [("bg", p.bg), ("add_bg", p.add_bg), ("del_bg", p.del_bg)]
                {
                    // 3:1, the non-text threshold: comments are deliberately
                    // the quietest thing in a diff, and the dark palette's
                    // comment-on-added-tint sits at 3.03 — the tightest pair
                    // here, and the one a future tweak will break first.
                    let ratio = contrast(c, surface);
                    assert!(ratio >= 3.0, "{name}: {tok} on {surface_name} is {ratio:.2}:1");
                }
            }
        }
    }

    /// Badge lettering must be readable on every fill a badge can get.
    ///
    /// This failed when written, against the palette that had already shipped:
    /// white on amber is 2.36:1, and amber is `NeedsReview` and `RateLimited`
    /// — two of the most common badges on the board. `on_accent_for` now picks
    /// dark ink for amber (7.8:1) and urgent (6.5:1).
    ///
    /// The bar here is 3:1, and that is worth stating plainly rather than
    /// leaving to be discovered: badge text is 11px, so WCAG AA would want
    /// 4.5:1. The light palette clears 5:1 on every fill. The dark palette's
    /// mid-tone accents sit at 3.0–3.9 with white lettering, which is the look
    /// that shipped and was deliberately kept — raising it would mean either
    /// darkening every dark accent or inverting every badge, and both are
    /// redesigns rather than fixes.
    #[test]
    fn badge_text_survives_every_fill_it_is_put_on() {
        for (theme, p) in [(Theme::Dark, &DARK), (Theme::Light, &LIGHT)] {
            set(theme);
            for (fill_name, fill) in [
                ("red", p.red),
                ("amber", p.amber),
                ("blue", p.blue),
                ("green", p.green),
                ("purple", p.purple),
                ("urgent", p.urgent),
                ("dim", p.dim),
                ("noise", p.noise),
            ] {
                let c = contrast(on_accent_for(fill), fill);
                assert!(c >= 3.0, "{theme:?}: badge text on {fill_name} is {c:.2}:1");
            }
        }
        set(Theme::Dark);
    }

    /// Read and unread file rows differ only by colour, so if the two collapse
    /// the review state becomes invisible — in the pane whose entire job is
    /// showing it.
    #[test]
    fn read_and_unread_rows_stay_apart() {
        for (name, p) in [("dark", &DARK), ("light", &LIGHT)] {
            let c = contrast(p.unread_base, p.read_base);
            assert!(c >= 1.8, "{name}: read and unread are {c:.2}:1 apart");
            assert!(
                contrast(p.unread_base, p.bg_panel) >= 4.5,
                "{name}: unread rows must be fully legible"
            );
        }
    }

    /// Graph lanes are followed by eye across a commit graph; two that look
    /// alike make the graph lie about which line is which.
    #[test]
    fn graph_lanes_are_distinguishable_from_each_other_and_the_page() {
        for (name, p) in [("dark", &DARK), ("light", &LIGHT)] {
            for (i, a) in p.graph.iter().enumerate() {
                assert!(
                    contrast(*a, p.bg_panel) >= 2.5,
                    "{name}: lane {i} is {:.2}:1 against the panel",
                    contrast(*a, p.bg_panel)
                );
                for (j, b) in p.graph.iter().enumerate().skip(i + 1) {
                    // Channel distance, which is crude but catches the failure
                    // that matters: two lanes a glance cannot separate. 80
                    // rather than a rounder number because the dark palette's
                    // blue and teal sit at 89 and are, in fact, distinct.
                    let d = (a.r() as i32 - b.r() as i32).abs()
                        + (a.g() as i32 - b.g() as i32).abs()
                        + (a.b() as i32 - b.b() as i32).abs();
                    assert!(d >= 80, "{name}: lanes {i} and {j} are too close ({d})");
                }
            }
        }
    }

    /// `system` is the mode that can be asked a question with no answer —
    /// which is the normal case on a Linux desktop with no portal running.
    #[test]
    fn an_unanswered_system_theme_falls_back_to_dark() {
        assert_eq!(Mode::System.resolve(None), Theme::Dark);
        assert_eq!(Mode::System.resolve(Some(egui::Theme::Light)), Theme::Light);
        assert_eq!(Mode::System.resolve(Some(egui::Theme::Dark)), Theme::Dark);
        // And the explicit modes never ask.
        assert_eq!(Mode::Light.resolve(Some(egui::Theme::Dark)), Theme::Light);
        assert_eq!(Mode::Dark.resolve(Some(egui::Theme::Light)), Theme::Dark);
    }

    #[test]
    fn switching_reports_whether_it_changed_anything() {
        set(Theme::Dark);
        assert!(!set(Theme::Dark), "no change must not report one");
        assert!(set(Theme::Light));
        assert_eq!(active(), Theme::Light);
        assert!(pal().bg.r() > 0x80, "the light palette is actually light");
        set(Theme::Dark);
        assert!(pal().bg.r() < 0x40);
    }
}
