//! The detail pane's arrangement: which views are on screen and where. `R-B20`.
//!
//! # Why a tree and not a tab index
//!
//! The detail pane used to be one `Tab` at a time. Reading a diff while
//! watching the transcript that produced it meant alternating between two tabs
//! and holding one of them in your head — which is exactly the work a second
//! pane does for you.
//!
//! `egui_tiles` supplies the tree; this module owns the three things that are
//! ours: what the *default* arrangement is, how it survives a restart, and how
//! the keyboard addresses a pane that may or may not currently exist.
//!
//! # The default is the old tab bar
//!
//! A single tab container holding all five panes renders and behaves exactly
//! like the tab bar it replaces. That is deliberate and worth defending: a
//! docking system that greets you with a novel arrangement makes you undo
//! something before you can start working.
//!
//! # Where it lives
//!
//! `~/.mogeung/layout.json`, client-side, for the same reason as the keymap and
//! the preferences ([ADR-0001]) — none of it is daemon state, and a second
//! client would rightly arrange itself differently.

use crate::app::Tab;
use std::path::PathBuf;

pub type Tree = egui_tiles::Tree<Tab>;

/// Every pane, in the order the tab bar used to show them.
///
/// Also the order focus cycling walks, so `next` means the same thing however
/// the panes have been dragged around. Cycling in *layout* order would change
/// meaning every time you moved something.
/// [`Tab::Terminal`] is deliberately absent: the shell moved out of the tree
/// and into the bottom panel on 2026-07-30 (`R-B33`), and is reached by
/// ``Ctrl+` `` rather than by being a pane. The variant survives only so that a
/// layout saved before then still parses — see [`strip_retired`].
pub const ALL_PANES: [Tab; 8] = [
    Tab::Changes,
    Tab::Transcript,
    Tab::Info,
    Tab::Debt,
    Tab::Agent,
    Tab::Explorer,
    Tab::Git,
    Tab::Insight,
];

/// Panes that no longer belong in the tree, and are dropped from a layout that
/// still holds them.
const RETIRED: [Tab; 1] = [Tab::Terminal];

/// The id is fixed rather than generated: `egui_tiles` keys its per-tile
/// interaction state off it, so a changing id would silently reset drag and
/// resize state on every launch.
const TREE_ID: &str = "detail-tiles";

pub fn default_tree() -> Tree {
    Tree::new_tabs(TREE_ID, ALL_PANES.to_vec())
}

pub fn path() -> PathBuf {
    let home = std::env::var("HOME").unwrap_or_else(|_| ".".into());
    PathBuf::from(home).join(".mogeung").join("layout.json")
}

/// Load the saved arrangement, falling back to the default.
///
/// The stored shape is `egui_tiles`' own, not ours, so a version bump can
/// change it under us — and the layout is the one piece of client state that
/// cannot be re-derived from the daemon. Any failure therefore yields the
/// default and a warning, never an error that stops the window opening. Same
/// rule the keymap and the preferences already follow.
pub fn load() -> (Tree, Option<String>) {
    let path = path();
    let Ok(text) = std::fs::read_to_string(&path) else {
        return (default_tree(), None);
    };
    match serde_json::from_str::<Tree>(&text) {
        // An empty tree is loadable and useless — it would present a blank
        // pane with no way back short of deleting the file by hand.
        Ok(mut tree) if !tree.is_empty() => {
            strip_retired(&mut tree);
            if tree.is_empty() {
                return (default_tree(), None);
            }
            (tree, None)
        }
        Ok(_) => (
            default_tree(),
            Some(format!("{} held an empty layout — using the default", path.display())),
        ),
        Err(e) => (
            default_tree(),
            Some(format!(
                "{} is unreadable ({e}) — using the default layout",
                path.display()
            )),
        ),
    }
}

/// Drop panes that have left the tree, and tidy up what that leaves behind.
///
/// The shell used to be the ninth pane. Deleting the variant outright would
/// have been cleaner and was not an option: `"Shell"` is written into every
/// saved `layout.json`, serde would refuse the file, and [`load`] degrades an
/// unparseable layout to the default — so the reward for upgrading would have
/// been losing an arrangement you had built by hand.
///
/// Removing a tile can leave a container holding one child or none, which
/// renders as a tab bar with a single tab or an empty split; `simplify` is what
/// makes the pane's departure invisible rather than a scar in the layout.
fn strip_retired(tree: &mut Tree) {
    let mut removed = false;
    for tab in RETIRED {
        while let Some(id) = tree.tiles.find_pane(&tab) {
            tree.tiles.remove(id);
            removed = true;
        }
    }
    if removed {
        tree.simplify(&egui_tiles::SimplificationOptions::default());
    }
}

pub fn save(tree: &Tree) -> Result<(), String> {
    let path = path();
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir).map_err(|e| e.to_string())?;
    }
    let json = serde_json::to_string_pretty(tree).map_err(|e| e.to_string())?;
    std::fs::write(&path, json).map_err(|e| e.to_string())
}

/// Whether nothing has been split yet — the root is still a plain tab bar.
///
/// Used to decide whether to show the "drag a tab out" hint. A hint that stays
/// up after you have obviously learnt the thing it teaches is just clutter, so
/// this lets it retire itself the moment you split anything.
pub fn is_unsplit(tree: &Tree) -> bool {
    matches!(
        tree.root().and_then(|r| tree.tiles.get(r)),
        Some(egui_tiles::Tile::Container(egui_tiles::Container::Tabs(_)))
    )
}

/// Which panes exist in the tree at all, in [`ALL_PANES`] order.
///
/// Not the same as "on screen": a pane that is an inactive tab is still here,
/// because pressing its shortcut should bring it forward rather than report it
/// missing.
pub fn panes_in(tree: &Tree) -> Vec<Tab> {
    ALL_PANES
        .iter()
        .copied()
        .filter(|t| tree.tiles.find_pane(t).is_some())
        .collect()
}

/// Bring `tab` forward, adding it back if it has been closed.
///
/// Re-adding is the important half. Closing a pane is one click and would
/// otherwise be permanent short of resetting the whole layout — so the tab
/// shortcut doubles as the way back, and nothing needs to be discoverable
/// beyond the key that already showed that pane.
pub fn focus(tree: &mut Tree, tab: Tab) {
    if tree.tiles.find_pane(&tab).is_none() {
        let new = tree.tiles.insert_pane(tab);
        match tree.root() {
            Some(root) => {
                if let Some(egui_tiles::Tile::Container(c)) = tree.tiles.get_mut(root) {
                    c.add_child(new);
                } else {
                    // The root is a bare pane, so there is no container to add
                    // to. Wrap both in tabs, which is the arrangement the user
                    // started from.
                    let tabs = tree.tiles.insert_tab_tile(vec![root, new]);
                    tree.root = Some(tabs);
                }
            }
            None => tree.root = Some(new),
        }
    }
    tree.make_active(|_, tile| matches!(tile, egui_tiles::Tile::Pane(p) if *p == tab));
}

/// The next pane after `from`, wrapping, among those that exist.
///
/// Returns `from` unchanged when it is the only pane, so cycling in a
/// single-pane layout is a no-op rather than a panic.
pub fn cycle(tree: &Tree, from: Tab, delta: i32) -> Tab {
    let panes = panes_in(tree);
    if panes.is_empty() {
        return from;
    }
    let at = panes.iter().position(|t| *t == from).unwrap_or(0) as i32;
    panes[(at + delta).rem_euclid(panes.len() as i32) as usize]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_default_layout_holds_every_pane() {
        let tree = default_tree();
        assert_eq!(panes_in(&tree), ALL_PANES.to_vec());
    }

    /// The layout is the one bit of client state that cannot be rebuilt from
    /// the daemon, and its stored shape belongs to a dependency. So a bad file
    /// must degrade, never stop the window opening.
    #[test]
    fn a_corrupt_layout_yields_the_default_and_a_warning() {
        let (tree, warning) = match serde_json::from_str::<Tree>("{ not json at all") {
            Ok(t) => (t, None),
            Err(_) => (default_tree(), Some(())),
        };
        assert!(warning.is_some());
        assert_eq!(panes_in(&tree), ALL_PANES.to_vec());
    }

    /// A tree round-trips through the format we actually store it in. Catches
    /// the case where an `egui_tiles` upgrade changes the shape — the test
    /// fails rather than every user silently losing their arrangement.
    #[test]
    fn a_layout_survives_being_written_and_read_back() {
        let tree = default_tree();
        let json = serde_json::to_string(&tree).expect("serialises");
        let back: Tree = serde_json::from_str(&json).expect("deserialises");
        assert_eq!(panes_in(&back), ALL_PANES.to_vec());
    }

    /// `Tab::Agent` was `Tab::Terminal` until 2026-07-29, and every saved
    /// `layout.json` still spells it the old way. `load` degrades a file it
    /// cannot parse to the default arrangement — correct for corruption, and
    /// exactly wrong here, because losing your panes to a rename would look
    /// like the docking system forgetting things at random.
    #[test]
    fn the_agent_pane_still_reads_and_writes_its_old_name() {
        assert_eq!(
            serde_json::to_string(&Tab::Agent).unwrap(),
            "\"Terminal\"",
            "a layout written now must stay readable by a build from before the rename"
        );
        assert_eq!(
            serde_json::from_str::<Tab>("\"Terminal\"").unwrap(),
            Tab::Agent,
            "a layout saved before the rename must still restore"
        );
    }

    /// And the shell pane must not be tempted to take the stored name it now
    /// displays. Two panes writing `"Terminal"` is a layout file that loads
    /// one of them twice and the other never — silently, because the shape
    /// stays valid.
    #[test]
    fn the_two_terminal_panes_do_not_share_a_stored_name() {
        let agent = serde_json::to_string(&Tab::Agent).unwrap();
        let shell = serde_json::to_string(&Tab::Terminal).unwrap();
        assert_ne!(agent, shell, "both panes claimed the same name on disk");
        assert_eq!(shell, "\"Shell\"");
        assert_eq!(serde_json::from_str::<Tab>("\"Shell\"").unwrap(), Tab::Terminal);
    }

    /// A layout saved while the shell was a pane must still *parse* — the
    /// whole reason the variant outlived the pane. What it must not do is
    /// restore the pane: it now lives in the bottom panel, and a ghost tab
    /// labelled Terminal beside it is the confusion the Agent rename just
    /// finished clearing up.
    #[test]
    fn a_layout_holding_the_retired_shell_pane_loads_without_it() {
        let mut tree = Tree::new_tabs(TREE_ID, {
            let mut v = ALL_PANES.to_vec();
            v.insert(5, Tab::Terminal);
            v
        });
        assert!(tree.tiles.find_pane(&Tab::Terminal).is_some(), "present to begin with");

        strip_retired(&mut tree);
        assert!(tree.tiles.find_pane(&Tab::Terminal).is_none(), "the pane is gone");
        assert_eq!(panes_in(&tree), ALL_PANES.to_vec(), "and took nothing with it");
    }

    /// The pathological version: a layout whose *only* pane was the shell. It
    /// strips to nothing, and an empty tree is a blank window with no way back
    /// short of deleting the file by hand.
    #[test]
    fn a_layout_that_was_only_the_shell_falls_back_to_the_default() {
        let mut tree = Tree::new_tabs(TREE_ID, vec![Tab::Terminal]);
        strip_retired(&mut tree);
        let tree = if tree.is_empty() { default_tree() } else { tree };
        assert_eq!(panes_in(&tree), ALL_PANES.to_vec());
    }

    #[test]
    fn cycling_visits_every_pane_and_wraps() {
        let tree = default_tree();
        let mut at = ALL_PANES[0];
        for _ in 0..ALL_PANES.len() {
            at = cycle(&tree, at, 1);
        }
        assert_eq!(at, ALL_PANES[0], "a full lap returns to the start");
        assert_eq!(
            cycle(&tree, ALL_PANES[0], -1),
            ALL_PANES[ALL_PANES.len() - 1],
            "backwards from the first wraps to the last"
        );
    }

    /// Closing a pane must not be permanent. The shortcut that used to show it
    /// is the way back, so this is the property that keeps the close button
    /// from being a trap.
    #[test]
    fn a_closed_pane_comes_back_when_it_is_asked_for() {
        let mut tree = default_tree();
        let id = tree.tiles.find_pane(&Tab::Debt).expect("present by default");
        tree.tiles.remove(id);
        assert!(!panes_in(&tree).contains(&Tab::Debt), "removed");

        focus(&mut tree, Tab::Debt);
        assert!(panes_in(&tree).contains(&Tab::Debt), "asked for, so back");
    }

    /// Cycling with one pane left must terminate rather than divide by zero or
    /// spin — the state you reach by closing four of the five.
    #[test]
    fn cycling_a_single_pane_layout_is_a_no_op() {
        let mut tree = default_tree();
        for tab in ALL_PANES.iter().filter(|t| **t != Tab::Changes) {
            let id = tree.tiles.find_pane(tab).unwrap();
            tree.tiles.remove(id);
        }
        assert_eq!(cycle(&tree, Tab::Changes, 1), Tab::Changes);
        assert_eq!(cycle(&tree, Tab::Changes, -1), Tab::Changes);
    }
}
