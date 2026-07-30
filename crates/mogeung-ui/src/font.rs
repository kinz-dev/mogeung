//! Which font the terminal panes draw in. `R-B32`.
//!
//! egui ships two families — Proportional and Monospace — and the monospace one
//! is Hack. Hack is a fine terminal font and carries none of the Powerline or
//! Nerd Font glyphs an oh-my-zsh prompt is built out of, so a p10k prompt drawn
//! in it is a row of empty boxes. Nothing about that is fixable inside egui's
//! font set: the glyphs are in *your* font, and egui does no system font lookup
//! at all.
//!
//! So this module does the lookup. It finds the installed families, resolves a
//! name to a file, and registers that file under a third family — `terminal` —
//! which the terminal panes ask for and nothing else does. The rest of the
//! window is untouched, which is the point: this is a terminal setting, not a
//! theme.
//!
//! # Why it reads the font files by hand
//!
//! Two reasons, and the second is the real one.
//!
//! A crate would do it (`fontdb`, `font-kit`), and would also pull a font stack
//! into a tree that has one because epaint needs it. But the whole job here is
//! four fields — family, monospaced, bold, italic — that live in three tables
//! near the front of the file.
//!
//! And reading them by hand means reading *only* them. There are 385 font files
//! and 220 MB under `/usr/share/fonts` on the machine this was written on; a
//! crate that parses each face loads every one of those bytes, which is a
//! visible hitch on a warm cache and a second-long freeze on a cold one. This
//! seeks to three tables per file and touches a few kilobytes.
//!
//! Everything here returns `Option` and nothing panics — the same rule the
//! transcript parsers follow, for the same reason: these files are written by
//! other people's tools and a malformed one must cost you a font, not a window.

use std::io::{Read, Seek, SeekFrom};
use std::path::{Path, PathBuf};
use std::sync::OnceLock;

/// The egui family name the user's terminal font is registered under.
///
/// A third family beside Proportional and Monospace, rather than a replacement
/// for Monospace: diffs, the transcript and the Editor are monospace too, and
/// they want the bundled font that is known to carry the icon glyphs.
pub const FAMILY: &str = "terminal";

/// The key the font *data* is stored under in [`egui::FontDefinitions`].
const KEY: &str = "terminal-user";

/// Give up rather than walk a font tree that is clearly not one. Both bounds
/// are far above any real installation — the point is that a symlink loop or a
/// `~/.fonts` pointed at a home directory cannot hang the window.
const MAX_DEPTH: usize = 8;
const MAX_FILES: usize = 4000;

/// One face found on this machine.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Face {
    pub family: String,
    pub path: PathBuf,
    /// Which face inside the file — always 0 outside a `.ttc` collection.
    pub index: u32,
    pub monospaced: bool,
    pub bold: bool,
    pub italic: bool,
}

/// Every face on this machine, scanned once per process.
///
/// Once, because the answer changes only when you install a font, and a rescan
/// per frame — or per menu open — would put a directory walk in the paint loop.
/// Installing a font while mogeung is running therefore needs a restart to show
/// up, which is the same deal every terminal emulator offers.
pub fn faces() -> &'static [Face] {
    static FACES: OnceLock<Vec<Face>> = OnceLock::new();
    FACES.get_or_init(scan)
}

/// The monospaced families, sorted, deduplicated — what the picker offers.
///
/// Monospaced only. A terminal grid is laid out from the width of `m`, so a
/// proportional font does not render badly here, it renders *wrongly*: every
/// row drifts out of its cells. Offering one would be offering a bug.
pub fn monospace_families() -> Vec<String> {
    let mut out: Vec<String> = faces()
        .iter()
        .filter(|f| f.monospaced)
        .map(|f| f.family.clone())
        .collect();
    out.sort_by_key(|s| s.to_lowercase());
    out.dedup();
    out
}

/// What "automatic" means: the families to try when nothing has been chosen.
///
/// `MesloLGS NF` is the font Powerlevel10k's own configuration wizard installs,
/// so having it is a strong signal that the prompt about to be drawn in it
/// needs the glyphs the bundled font does not have. Defaulting to it turns the
/// common case — an oh-my-zsh user opening the terminal for the first time —
/// from a row of boxes into a working prompt, with no setting to find first.
///
/// Ordered, and short on purpose. A long list of "fonts we like" would make
/// which font you get depend on what else happens to be installed, which is a
/// worse surprise than the bundled default.
pub const PREFERRED: &[&str] = &["MesloLGS NF"];

/// What "automatic" resolves to on this machine, if anything.
pub fn default_family() -> Option<&'static Face> {
    PREFERRED.iter().find_map(|n| pick(n))
}

/// The face actually in force: what was chosen, else the automatic pick.
///
/// The one function the UI should ask, because "which font am I looking at"
/// has a different answer from "which font did I choose" whenever the answer
/// to the second is *nothing*.
pub fn effective(chosen: Option<&str>) -> Option<&'static Face> {
    match chosen.map(str::trim).filter(|s| !s.is_empty()) {
        Some(n) => pick(n),
        None => default_family(),
    }
}

/// The face to use for a family name, or `None` if nothing matches.
///
/// Case-insensitive, because nobody types `MesloLGS NF` the same way twice, and
/// upright-and-regular wins: a family whose bold face happens to be scanned
/// first must not make the whole terminal bold.
pub fn pick(family: &str) -> Option<&'static Face> {
    let want = family.trim().to_lowercase();
    let mut best: Option<&Face> = None;
    for f in faces() {
        if f.family.to_lowercase() != want {
            continue;
        }
        let plain = !f.bold && !f.italic;
        match best {
            None => best = Some(f),
            Some(b) if !(!b.bold && !b.italic) && plain => best = Some(f),
            _ => {}
        }
    }
    best
}

/// The font set to hand egui: the defaults, plus a `terminal` family.
///
/// Split out from [`install`] and pure, because the property worth testing is
/// the *chain*. The user's font goes first and the bundled fonts stay behind
/// it, so a glyph the chosen font lacks — an emoji in a commit message, one of
/// mogeung's own icons — still draws instead of becoming a box. A font setting
/// that silently deletes glyphs is worse than no font setting.
pub fn definitions(user: Option<(Vec<u8>, u32)>) -> egui::FontDefinitions {
    let mut defs = egui::FontDefinitions::default();
    let mut chain = defs
        .families
        .get(&egui::FontFamily::Monospace)
        .cloned()
        .unwrap_or_default();
    if let Some((bytes, index)) = user {
        let mut data = egui::FontData::from_owned(bytes);
        data.index = index;
        defs.font_data.insert(KEY.to_string(), std::sync::Arc::new(data));
        chain.insert(0, KEY.to_string());
    }
    // Registered even with no font chosen, so `FontFamily::Name("terminal")` is
    // always a family egui knows. An unregistered name is not a fallback in
    // egui, it is a panic in the paint loop.
    defs.families
        .insert(egui::FontFamily::Name(FAMILY.into()), chain);
    defs
}

/// Point the `terminal` family at `family`, or back at the bundled monospace.
///
/// Returns a warning to show the user rather than an error to handle: a font
/// that has been uninstalled since it was chosen must leave a working terminal
/// in a readable font and *say so*, because the alternative — quietly drawing
/// in Hack — looks like the setting never worked.
/// A `None` family is *automatic*, not "bundled": it resolves through
/// [`PREFERRED`], and only falls back to the bundled monospace when none of
/// those is installed. Silence in that case is correct — nothing was asked for,
/// so nothing failed.
pub fn install(ctx: &egui::Context, family: Option<&str>) -> Option<String> {
    let name = family.map(str::trim).filter(|s| !s.is_empty());
    let mut warning = None;
    let face = match name {
        None => default_family(),
        Some(n) => {
            let found = pick(n);
            if found.is_none() {
                warning = Some(format!(
                    "no monospaced font family called \"{n}\" is installed — \
                     the terminal is using the bundled monospace"
                ));
            }
            found
        }
    };
    let user = face.and_then(|f| match std::fs::read(&f.path) {
        Ok(bytes) => Some((bytes, f.index)),
        Err(e) => {
            warning = Some(format!(
                "terminal font {} is at {} and could not be read ({e}) — \
                 using the bundled monospace",
                f.family,
                f.path.display()
            ));
            None
        }
    });
    ctx.set_fonts(definitions(user));
    warning
}

/// Where fonts live, in the order a system font cache would look.
///
/// Not read from fontconfig. Parsing `fonts.conf` — includes, globs, per-user
/// overrides — is a bigger and less predictable job than the list of places
/// fonts have actually been installed for twenty years, and a directory that is
/// missing simply yields nothing.
fn dirs() -> Vec<PathBuf> {
    let home = std::env::var("HOME").ok().map(PathBuf::from);
    let mut out = Vec::new();
    if let Some(h) = &home {
        if let Ok(xdg) = std::env::var("XDG_DATA_HOME") {
            out.push(PathBuf::from(xdg).join("fonts"));
        } else {
            out.push(h.join(".local/share/fonts"));
        }
        out.push(h.join(".fonts"));
        out.push(h.join("Library/Fonts"));
    }
    out.push(PathBuf::from("/usr/share/fonts"));
    out.push(PathBuf::from("/usr/local/share/fonts"));
    // Flatpak: the host's fonts are bind-mounted here and nowhere else.
    out.push(PathBuf::from("/run/host/fonts"));
    out.push(PathBuf::from("/Library/Fonts"));
    out.push(PathBuf::from("/System/Library/Fonts"));
    out.push(PathBuf::from("/System/Library/Fonts/Supplemental"));
    out
}

fn scan() -> Vec<Face> {
    let mut out = Vec::new();
    let mut seen = 0usize;
    for dir in dirs() {
        let mut stack = vec![(dir, 0usize)];
        while let Some((dir, depth)) = stack.pop() {
            if depth > MAX_DEPTH || seen >= MAX_FILES {
                break;
            }
            let Ok(entries) = std::fs::read_dir(&dir) else {
                continue;
            };
            for e in entries.flatten() {
                let path = e.path();
                // `file_type` and not `metadata`: the latter follows symlinks,
                // and a font directory symlinked to its own parent is a walk
                // that never ends.
                let Ok(ft) = e.file_type() else { continue };
                if ft.is_dir() {
                    stack.push((path, depth + 1));
                    continue;
                }
                if !is_font(&path) {
                    continue;
                }
                seen += 1;
                if seen >= MAX_FILES {
                    break;
                }
                out.extend(read_faces(&path));
            }
        }
    }
    out
}

fn is_font(path: &Path) -> bool {
    let Some(ext) = path.extension().and_then(|e| e.to_str()) else {
        return false;
    };
    matches!(ext.to_lowercase().as_str(), "ttf" | "otf" | "ttc" | "otc")
}

/// The faces in one file. A collection holds several; everything else holds one.
fn read_faces(path: &Path) -> Vec<Face> {
    let Ok(mut f) = std::fs::File::open(path) else {
        return Vec::new();
    };
    let Some(head) = read_at(&mut f, 0, 12) else {
        return Vec::new();
    };
    let mut out = Vec::new();
    if &head[0..4] == b"ttcf" {
        let n = be32(&head, 8).min(64) as usize;
        let Some(offsets) = read_at(&mut f, 12, n * 4) else {
            return out;
        };
        for i in 0..n {
            if let Some(face) = read_face(&mut f, be32(&offsets, i * 4) as u64, path, i as u32) {
                out.push(face);
            }
        }
    } else if let Some(face) = read_face(&mut f, 0, path, 0) {
        out.push(face);
    }
    out
}

/// One face, given where its table directory starts.
///
/// Table offsets are from the start of the *file* even inside a collection,
/// which is why only the directory position varies here.
fn read_face(f: &mut std::fs::File, dir: u64, path: &Path, index: u32) -> Option<Face> {
    let head = read_at(f, dir, 12)?;
    let num_tables = be16(&head, 4) as usize;
    // 512 tables is already absurd; a bogus count is how a corrupt file turns
    // into a multi-megabyte read.
    if num_tables == 0 || num_tables > 512 {
        return None;
    }
    let records = read_at(f, dir + 12, num_tables * 16)?;

    let mut name = None;
    let mut post = None;
    let mut headt = None;
    for i in 0..num_tables {
        let at = i * 16;
        let tag = &records[at..at + 4];
        let off = be32(&records, at + 8) as u64;
        let len = be32(&records, at + 12) as usize;
        match tag {
            b"name" => name = Some((off, len)),
            b"post" => post = Some(off),
            b"head" => headt = Some(off),
            _ => {}
        }
    }

    let (name_off, name_len) = name?;
    // A name table over a megabyte is not a name table.
    let family = family_of(&read_at(f, name_off, name_len.min(1 << 20))?)?;

    // isFixedPitch — the same field every font tool reads to answer "is this
    // monospaced". Absent means no, which is the safe direction: the picker
    // shows one font too few rather than one that would break the grid.
    let monospaced = post
        .and_then(|off| read_at(f, off + 12, 4))
        .map(|b| be32(&b, 0) != 0)
        .unwrap_or(false);

    // head.macStyle: bit 0 bold, bit 1 italic.
    let style = headt
        .and_then(|off| read_at(f, off + 44, 2))
        .map(|b| be16(&b, 0))
        .unwrap_or(0);

    Some(Face {
        family,
        path: path.to_path_buf(),
        index,
        monospaced,
        bold: style & 1 != 0,
        italic: style & 2 != 0,
    })
}

/// The family name out of a `name` table.
///
/// Name ID 16 (typographic family) is preferred over ID 1 where a font carries
/// both: ID 1 splits a family across its weights — "JetBrains Mono ExtraBold"
/// is its own ID 1 — which would fill the picker with entries that are all the
/// same font, and make choosing "JetBrains Mono" impossible.
fn family_of(table: &[u8]) -> Option<String> {
    if table.len() < 6 {
        return None;
    }
    let count = be16(table, 2) as usize;
    let storage = be16(table, 4) as usize;
    let mut id1 = None;
    let mut id16 = None;
    for i in 0..count {
        let at = 6 + i * 12;
        if at + 12 > table.len() {
            break;
        }
        let platform = be16(table, at);
        let encoding = be16(table, at + 2);
        let name_id = be16(table, at + 6);
        if name_id != 1 && name_id != 16 {
            continue;
        }
        let len = be16(table, at + 8) as usize;
        let off = storage + be16(table, at + 10) as usize;
        if off + len > table.len() {
            continue;
        }
        let Some(s) = decode(platform, encoding, &table[off..off + len]) else {
            continue;
        };
        let s = s.trim().to_string();
        if s.is_empty() {
            continue;
        }
        if name_id == 16 {
            id16.get_or_insert(s);
        } else {
            id1.get_or_insert(s);
        }
    }
    id16.or(id1)
}

/// A name record's bytes as text.
///
/// Windows (3) and Unicode (0) records are UTF-16BE; Macintosh (1) records are
/// MacRoman, which agrees with ASCII over the range font names actually use.
/// Anything else is skipped rather than guessed at — a mojibake family name in
/// the picker is worse than one fewer font, because you cannot tell what it is.
fn decode(platform: u16, encoding: u16, raw: &[u8]) -> Option<String> {
    match platform {
        0 | 3 => {
            // Windows Symbol (3,0) and BMP (3,1) are both UTF-16BE. Later
            // Windows encodings are legacy codepages we do not decode.
            if platform == 3 && encoding > 1 {
                return None;
            }
            if raw.len() % 2 != 0 {
                return None;
            }
            let units: Vec<u16> = raw
                .chunks_exact(2)
                .map(|c| u16::from_be_bytes([c[0], c[1]]))
                .collect();
            String::from_utf16(&units).ok()
        }
        1 if encoding == 0 => {
            if raw.iter().any(|b| !b.is_ascii()) {
                return None;
            }
            Some(String::from_utf8_lossy(raw).into_owned())
        }
        _ => None,
    }
}

fn read_at(f: &mut std::fs::File, off: u64, len: usize) -> Option<Vec<u8>> {
    if len == 0 {
        return None;
    }
    f.seek(SeekFrom::Start(off)).ok()?;
    let mut buf = vec![0u8; len];
    f.read_exact(&mut buf).ok()?;
    Some(buf)
}

fn be16(d: &[u8], at: usize) -> u16 {
    if at + 2 > d.len() {
        return 0;
    }
    u16::from_be_bytes([d[at], d[at + 1]])
}

fn be32(d: &[u8], at: usize) -> u32 {
    if at + 4 > d.len() {
        return 0;
    }
    u32::from_be_bytes([d[at], d[at + 1], d[at + 2], d[at + 3]])
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A minimal `name` table: version 0, one record per entry.
    fn name_table(entries: &[(u16, u16, u16, &str)]) -> Vec<u8> {
        let mut storage: Vec<u8> = Vec::new();
        let mut records: Vec<u8> = Vec::new();
        for (platform, encoding, id, text) in entries {
            let bytes: Vec<u8> = if *platform == 1 {
                text.as_bytes().to_vec()
            } else {
                text.encode_utf16().flat_map(|u| u.to_be_bytes()).collect()
            };
            records.extend(platform.to_be_bytes());
            records.extend(encoding.to_be_bytes());
            records.extend(0u16.to_be_bytes()); // language
            records.extend(id.to_be_bytes());
            records.extend((bytes.len() as u16).to_be_bytes());
            records.extend((storage.len() as u16).to_be_bytes());
            storage.extend(bytes);
        }
        let count = entries.len() as u16;
        let mut out = Vec::new();
        out.extend(0u16.to_be_bytes()); // version
        out.extend(count.to_be_bytes());
        out.extend((6 + records.len() as u16).to_be_bytes()); // storage offset
        out.extend(records);
        out.extend(storage);
        out
    }

    /// The name a Nerd Font actually carries, in the encoding it actually
    /// carries it in. UTF-16BE read as bytes would give "M\0e\0s\0l\0o\0…",
    /// which is not a family anyone can select.
    #[test]
    fn a_windows_family_name_is_read_as_utf16() {
        let t = name_table(&[(3, 1, 1, "MesloLGS NF")]);
        assert_eq!(family_of(&t).as_deref(), Some("MesloLGS NF"));
    }

    /// ID 16 wins where a font carries both, or the picker lists one entry per
    /// weight — "JetBrains Mono ExtraBold" and friends — and the family itself
    /// cannot be chosen at all.
    #[test]
    fn the_typographic_family_beats_the_split_one() {
        let t = name_table(&[
            (3, 1, 1, "JetBrains Mono ExtraBold"),
            (3, 1, 16, "JetBrains Mono"),
        ]);
        assert_eq!(family_of(&t).as_deref(), Some("JetBrains Mono"));

        // With only ID 1 present, that is the answer — most fonts stop there.
        let only1 = name_table(&[(3, 1, 1, "Hack")]);
        assert_eq!(family_of(&only1).as_deref(), Some("Hack"));
    }

    /// A Macintosh record is ASCII, not UTF-16 — decoding it as UTF-16 yields
    /// CJK gibberish, which is exactly what a wrong branch here would produce.
    #[test]
    fn a_macintosh_name_is_read_as_ascii() {
        let t = name_table(&[(1, 0, 1, "Menlo")]);
        assert_eq!(family_of(&t).as_deref(), Some("Menlo"));
    }

    /// These files come from other people's tools, so truncation is a fact of
    /// life, not a bug report. Every one of these must be a `None`.
    #[test]
    fn a_malformed_name_table_yields_nothing_rather_than_panicking() {
        let full = name_table(&[(3, 1, 1, "MesloLGS NF")]);
        for cut in 0..full.len() {
            let _ = family_of(&full[..cut]);
        }
        assert_eq!(family_of(&[]), None);
        assert_eq!(family_of(&[0, 0, 9, 9, 9, 9]), None, "count past the end");

        // A record pointing outside the table is skipped, not read.
        let mut bad = full.clone();
        let at = 6 + 10;
        bad[at] = 0xff;
        bad[at + 1] = 0xff;
        assert_eq!(family_of(&bad), None);
    }

    /// The chain is the whole safety property: a Nerd Font that lacks an emoji
    /// — or one of mogeung's own icon glyphs — must fall through to the fonts
    /// that carry them, not draw a box.
    #[test]
    fn the_user_font_leads_and_the_bundled_ones_still_follow() {
        let defs = definitions(Some((vec![1, 2, 3], 0)));
        let chain = defs
            .families
            .get(&egui::FontFamily::Name(FAMILY.into()))
            .expect("the terminal family must exist");
        assert_eq!(chain.first().map(String::as_str), Some(KEY));
        assert!(chain.len() > 1, "the bundled fallbacks were dropped: {chain:?}");

        let bundled = egui::FontDefinitions::default();
        let want = bundled.families.get(&egui::FontFamily::Monospace).unwrap();
        assert_eq!(&chain[1..], &want[..], "the fallback order changed");
    }

    /// With nothing chosen the family must still exist. egui does not fall back
    /// on an unknown family name — it panics inside the paint loop, so every
    /// frame after the setting is cleared would take the window down.
    #[test]
    fn the_terminal_family_exists_even_with_no_font_chosen() {
        let defs = definitions(None);
        let chain = defs
            .families
            .get(&egui::FontFamily::Name(FAMILY.into()))
            .expect("registered unconditionally");
        assert!(!chain.is_empty());
        assert!(!defs.font_data.contains_key(KEY), "nothing to register");
    }

    /// A collection index must survive into the font data, or a `.ttc` gives
    /// you whichever face happens to be first.
    #[test]
    fn a_collection_index_is_carried_through() {
        let defs = definitions(Some((vec![0; 4], 3)));
        assert_eq!(defs.font_data.get(KEY).expect("registered").index, 3);
    }

    /// Only monospaced faces are offered, and the list is sorted and unique —
    /// a family with four weights is one entry, not four. Runs against whatever
    /// this machine has installed, so it asserts shape rather than contents.
    #[test]
    fn the_offered_families_are_monospaced_sorted_and_unique() {
        let fams = monospace_families();
        let mut sorted = fams.clone();
        sorted.sort_by_key(|s| s.to_lowercase());
        assert_eq!(fams, sorted, "the picker would list families out of order");
        let mut uniq = fams.clone();
        uniq.dedup();
        assert_eq!(fams, uniq, "one entry per weight instead of per family");
        assert!(fams.iter().all(|f| !f.trim().is_empty()));
        for f in &fams {
            assert!(
                pick(f).is_some(),
                "{f} is offered but cannot be resolved back to a face"
            );
        }
    }

    /// The picker's names are what `pick` takes, whatever case they arrive in
    /// — and a name nobody has installed is a `None`, not a panic or a
    /// silently wrong face.
    #[test]
    fn resolving_a_family_is_case_insensitive_and_refuses_the_unknown() {
        assert!(pick("no such family at all, surely").is_none());
        if let Some(first) = monospace_families().first() {
            assert!(pick(&first.to_uppercase()).is_some());
            assert!(pick(&format!("  {first}  ")).is_some());
        }
    }

    /// "Automatic" must mean a real preference, not "whatever sorts first".
    /// The list is ordered and every name in it has to be one `pick` could
    /// return — a typo here would be a default that silently never applies.
    #[test]
    fn the_automatic_default_prefers_meslo() {
        assert_eq!(PREFERRED.first().copied(), Some("MesloLGS NF"));
        assert!(!PREFERRED.is_empty());
        for name in PREFERRED {
            assert_eq!(name.trim(), *name, "{name} would never match");
            assert!(!name.is_empty());
        }
        // On a machine that has it, automatic resolves to it and to nothing
        // else; on one that does not, automatic is simply absent.
        match default_family() {
            Some(f) => assert!(PREFERRED.contains(&f.family.as_str()), "{}", f.family),
            None => assert!(PREFERRED.iter().all(|n| pick(n).is_none())),
        }
    }

    /// The distinction the picker's label depends on: with nothing chosen the
    /// font in force is the automatic one, and with something chosen it is
    /// that. Collapsing the two would let the menu claim you were looking at
    /// the bundled font while a Nerd Font was on screen.
    #[test]
    fn the_effective_family_falls_back_to_the_automatic_one() {
        assert_eq!(
            effective(None).map(|f| &f.family),
            default_family().map(|f| &f.family)
        );
        // Blank and whitespace are "nothing chosen", not "a family called ''".
        assert_eq!(effective(Some("   ")).map(|f| &f.family), effective(None).map(|f| &f.family));
        assert!(effective(Some("no such family at all, surely")).is_none());
        if let Some(first) = monospace_families().first() {
            assert_eq!(effective(Some(first)).map(|f| f.family.as_str()), Some(first.as_str()));
        }
    }

    /// The upright face wins. Scan order is directory order, so without this a
    /// family whose Bold file sorts first would make the whole terminal bold.
    #[test]
    fn a_regular_face_is_preferred_over_bold_and_italic() {
        for f in faces() {
            if !f.bold && !f.italic {
                continue;
            }
            let Some(chosen) = pick(&f.family) else { continue };
            let siblings = faces().iter().any(|s| {
                s.family.to_lowercase() == f.family.to_lowercase() && !s.bold && !s.italic
            });
            if siblings {
                assert!(
                    !chosen.bold && !chosen.italic,
                    "{} resolved to a {} face",
                    f.family,
                    if chosen.bold { "bold" } else { "italic" }
                );
            }
        }
    }
}
