//! Mods Core link, protocol version 1.
//!
//! Copy this one file into your Cinnabar mod to appear in Mods Core's list. Mods Core shows
//! your mod's name, version and description, and when a player clicks it, your menu opens.
//! It uses the `events` cue channel (no grant) and your own panel (the `controls` grant).
//!
//! ```ignore
//! mod mods_core;
//! // Once:
//! let mut link = mods_core::Link::new("my-mod", "My Mod", "1.0.0", "What my mod does.");
//! // In every frame callback, with whether your panel is open and how to build it whole:
//! match link.frame(controls.panel_open, || my_whole_panel_json()) {
//!     mods_core::Event::Opened => { /* send your current values, if your panel needs it */ }
//!     _ => {}
//! }
//! // Your own menu key, if the player set one:
//! if my_key_pressed { link.toggle(controls.panel_open); }
//! ```
//!
//! # Why the panel comes and goes
//!
//! Cinnabar shows one mod menu at a time: the one the player last chose with its key (Mods
//! Core, with F10), or, when that mod has no panel, the first mod in load order that has one.
//! A mod cannot pick itself. So mods in Mods Core keep **no panel while their menu is closed**:
//! to open a menu, Mods Core drops its own panel and the chosen mod sets its panel and opens
//! it, which makes it the only menu there is. When the menu closes, the mod drops its panel
//! again and Mods Core takes its own back. `Link` does all of this for you.
//!
//! # The wire format
//!
//! * Info: your mod emits `mcore.t.<slug>` with 8 numbers, one chunk of your text at a time.
//!   The text is UTF-8 `name` + U+001F + `version` + U+001F + `description`, at most
//!   [`MAX_TEXT`] bytes. It is cut into chunks of 18 bytes; each chunk is sent as
//!   `[index + 256 * count, checksum, d0, d1, d2, d3, d4, d5]`, where every `d` packs three
//!   bytes big-endian (`b0 * 65536 + b1 * 256 + b2`, exact in an `f32`) and the last chunk is
//!   padded with zero bytes. `checksum` is [`checksum`] of the whole text.
//! * Open: `mcore.open.<slug>` (no numbers) asks that mod to open its menu. Mods Core sends it
//!   on a click; a mod sends it for itself when the player presses its own key. Every mod in
//!   the protocol that has a panel at that moment drops it.
//! * Busy: `mcore.busy.<slug>` (no numbers), every frame while that mod's menu is open. Mods
//!   Core takes its panel back once no mod has been busy for a moment.
//! * A slug is your mod's id: 1 to [`MAX_SLUG`] bytes of `a-z`, `0-9` and `-`.
//! * Keep sending info: Mods Core forgets a mod it has not heard from for a few seconds, so a
//!   mod that is turned off leaves the list on its own.

#![allow(dead_code)] // A mod uses only its own side of the protocol.

use mod_api::bindings::cinnabar::extension::{events, panel};

/// Cue name prefix of info chunks.
pub const INFO_PREFIX: &str = "mcore.t.";
/// Cue name prefix of open requests.
pub const OPEN_PREFIX: &str = "mcore.open.";
/// Cue name prefix of "my menu is open".
pub const BUSY_PREFIX: &str = "mcore.busy.";
/// Longest slug: Cinnabar cue names are at most 32 bytes.
pub const MAX_SLUG: usize = 32 - OPEN_PREFIX.len(); // busy has the same length
/// Longest text (name, version and description together).
pub const MAX_TEXT: usize = 252;
/// Bytes in one chunk: six numbers of three bytes.
pub const CHUNK: usize = 18;
/// Separates the name, the version and the description.
pub const SEPARATOR: char = '\u{1f}';
/// Frames between two chunks.
const EVERY: u32 = 2;

/// Whether `slug` can be used in cue names.
pub fn valid_slug(slug: &str) -> bool {
    !slug.is_empty() && slug.len() <= MAX_SLUG && slug.bytes().all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'-')
}

/// A 16-bit checksum of the text, so a changed text is noticed.
pub fn checksum(bytes: &[u8]) -> u32 {
    bytes.iter().fold(7u32, |sum, b| (sum.wrapping_mul(31) + u32::from(*b)) & 0xffff)
}

/// The text a mod sends: name, version and description, made safe and cut to [`MAX_TEXT`].
pub fn text(name: &str, version: &str, description: &str) -> Vec<u8> {
    let clean = |s: &str| s.chars().filter(|c| !c.is_control()).collect::<String>();
    let mut out = format!("{}{SEPARATOR}{}{SEPARATOR}{}", clean(name), clean(version), clean(description));
    while out.len() > MAX_TEXT {
        out.pop();
    }
    out.into_bytes()
}

/// The chunks of a text, ready to emit as cue values.
pub fn chunks(text: &[u8]) -> Vec<[f32; 8]> {
    let count = text.len().div_ceil(CHUNK).max(1);
    let sum = checksum(text) as f32;
    (0..count)
        .map(|index| {
            let mut values = [0.0f32; 8];
            values[0] = (index + 256 * count) as f32;
            values[1] = sum;
            for d in 0..6 {
                let at = index * CHUNK + d * 3;
                let byte = |k: usize| u32::from(text.get(at + k).copied().unwrap_or(0));
                values[2 + d] = (byte(0) << 16 | byte(1) << 8 | byte(2)) as f32;
            }
            values
        })
        .collect()
}

/// One received info chunk.
#[derive(Clone, Debug, PartialEq)]
pub struct Chunk<'a> {
    pub slug: &'a str,
    pub index: usize,
    pub count: usize,
    pub checksum: u32,
    pub bytes: [u8; CHUNK],
}

/// Reads an info cue; `None` for anything else or anything malformed.
pub fn read_chunk<'a>(name: &'a str, values: &[f32]) -> Option<Chunk<'a>> {
    let slug = name.strip_prefix(INFO_PREFIX).filter(|s| valid_slug(s))?;
    if values.len() != 8 || !values.iter().all(|v| v.is_finite() && *v >= 0.0 && v.fract() == 0.0 && *v < 16_777_216.0) {
        return None;
    }
    let head = values[0] as usize;
    let (index, count) = (head % 256, head / 256);
    if count == 0 || index >= count || count * CHUNK > MAX_TEXT + CHUNK {
        return None;
    }
    let mut bytes = [0u8; CHUNK];
    for d in 0..6 {
        let v = values[2 + d] as u32;
        bytes[d * 3] = (v >> 16) as u8;
        bytes[d * 3 + 1] = (v >> 8) as u8;
        bytes[d * 3 + 2] = v as u8;
    }
    Some(Chunk { slug, index, count, checksum: values[1] as u32, bytes })
}

/// Name, version and description from a whole text; `None` when the checksum or the UTF-8
/// does not hold.
pub fn parse(mut bytes: Vec<u8>, sum: u32) -> Option<(String, String, String)> {
    while bytes.last() == Some(&0) {
        bytes.pop();
    }
    if checksum(&bytes) != sum {
        return None;
    }
    let text = String::from_utf8(bytes).ok()?;
    let mut parts = text.splitn(3, SEPARATOR);
    let name = parts.next()?.trim().to_owned();
    let version = parts.next().unwrap_or("").trim().to_owned();
    let description = parts.next().unwrap_or("").trim().to_owned();
    (!name.is_empty()).then_some((name, version, description))
}

/// The open request Mods Core sends for a slug.
pub fn open_cue(slug: &str) -> String {
    format!("{OPEN_PREFIX}{slug}")
}

/// What `Link::frame` did this frame.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Event {
    Nothing,
    /// Your panel was set and opened: send your current values if it needs them.
    Opened,
    /// Your menu closed and your panel was dropped.
    Closed,
}

/// Your mod's side: announces it to Mods Core and opens and drops your panel.
pub struct Link {
    info: String,
    open: String,
    busy: String,
    chunks: Vec<[f32; 8]>,
    next: usize,
    frame: u32,
    /// Your panel is set (the bundled one at load, or one set to open).
    has_panel: bool,
    /// The menu has been seen open since it was set.
    seen_open: bool,
    /// Frames since the menu was set to open.
    opening: u32,
}

/// Frames a menu set to open may take to show before the link opens it again, and gives up.
const RETRY_AFTER: u32 = 4;
const GIVE_UP_AFTER: u32 = 60;

impl Link {
    /// `slug` is your mod's id (see [`valid_slug`]); the rest is what the list shows.
    pub fn new(slug: &str, name: &str, version: &str, description: &str) -> Self {
        debug_assert!(valid_slug(slug), "Mods Core slug: 1 to {MAX_SLUG} bytes of a-z, 0-9 and -");
        Self {
            info: format!("{INFO_PREFIX}{slug}"),
            open: open_cue(slug),
            busy: format!("{BUSY_PREFIX}{slug}"),
            chunks: chunks(&text(name, version, description)),
            next: 0,
            frame: 0,
            has_panel: true,
            seen_open: false,
            opening: 0,
        }
    }

    /// Call once per frame callback. `panel_open` is whether your menu is open now, and
    /// `whole_panel` builds your complete panel JSON (it is only called to open the menu).
    pub fn frame(&mut self, panel_open: bool, whole_panel: impl FnOnce() -> String) -> Event {
        self.frame = self.frame.wrapping_add(1);
        if self.frame % EVERY == 0 && !self.chunks.is_empty() {
            let _ = events::emit(&self.info, &self.chunks[self.next]);
            self.next = (self.next + 1) % self.chunks.len();
        }
        let cues = events::poll();
        let mine = cues.iter().any(|cue| cue.name == self.open);
        let other = cues.iter().any(|cue| cue.name.starts_with(OPEN_PREFIX) && cue.name != self.open);
        // `panel_open` is whether the shown menu is open, whoever owns it: it is yours only
        // while you have a panel.
        let open = panel_open && self.has_panel;
        if open {
            let _ = events::emit(&self.busy, &[]);
        }
        if mine && !self.has_panel {
            if panel::set_content(&whole_panel()).is_ok() && panel::set_open(true).is_ok() {
                self.has_panel = true;
                self.seen_open = false;
                self.opening = 0;
                return Event::Opened;
            }
            return Event::Nothing;
        }
        if !self.has_panel {
            return Event::Nothing;
        }
        if other {
            // Another menu is opening: step aside.
            return self.drop_panel();
        }
        if open {
            self.seen_open = true;
            self.opening = 0;
            return Event::Nothing;
        }
        if self.seen_open {
            // The player closed the menu.
            return self.drop_panel();
        }
        // Set but not shown yet (or the bundled panel at load): give Cinnabar a few frames to
        // show it, ask again, and give up after a while.
        self.opening += 1;
        if self.opening >= GIVE_UP_AFTER || (self.frame == self.opening && self.opening == 1) {
            return self.drop_panel();
        }
        if self.opening % RETRY_AFTER == 0 {
            let _ = panel::set_open(true);
        }
        Event::Nothing
    }

    fn drop_panel(&mut self) -> Event {
        let was_seen = self.seen_open;
        if panel::set_content("").is_ok() {
            self.has_panel = false;
            self.seen_open = false;
            self.opening = 0;
        }
        if was_seen { Event::Closed } else { Event::Nothing }
    }

    /// Whether your panel is set now (your menu is open or opening). `panel_open` in your
    /// controls is about whichever menu is shown, so check this before sending your values.
    pub fn has_panel(&self) -> bool {
        self.has_panel
    }

    /// Your own menu key: opens your menu (through Mods Core's hand-over) or closes it.
    pub fn toggle(&mut self, panel_open: bool) {
        if panel_open && self.has_panel {
            let _ = panel::set_open(false);
        } else {
            let _ = events::emit(&self.open, &[]);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_text_survives_the_trip() {
        let bytes = text("Technic Tweaks", "3.1.0", "Chunk borders, slime chunks, light spots and hitboxes. ñ ✓");
        let sum = checksum(&bytes);
        let mut whole = Vec::new();
        for values in chunks(&bytes) {
            let chunk = read_chunk("mcore.t.technic-tweaks", &values).unwrap();
            assert_eq!((chunk.slug, chunk.checksum), ("technic-tweaks", sum));
            whole.extend_from_slice(&chunk.bytes);
        }
        let (name, version, description) = parse(whole, sum).unwrap();
        assert_eq!((name.as_str(), version.as_str()), ("Technic Tweaks", "3.1.0"));
        assert!(description.ends_with("ñ ✓"));
    }

    #[test]
    fn limits_and_rubbish() {
        assert!(valid_slug("day-counter") && !valid_slug("Day") && !valid_slug(""));
        assert_eq!(OPEN_PREFIX.len() + MAX_SLUG, 32);
        assert!(text("a", "b", &"x".repeat(999)).len() <= MAX_TEXT);
        assert!(read_chunk("mcore.t.x", &[0.5; 8]).is_none());
        assert!(read_chunk("other.t.x", &[256.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0]).is_none());
        assert!(parse(b"bad".to_vec(), 1).is_none());
    }
}
