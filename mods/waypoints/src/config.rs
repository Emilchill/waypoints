//! Settings and the saved waypoints. Everything lives in the mod's one settings file (16 KB).
//! Waypoints are kept per world (server or save) and each one belongs to one dimension.
//!
//! A JSON library costs far more wasm fuel than a callback allows, so the file holds an array
//! of short strings in the mod's own format, read a little at a time:
//! `{"waypoints1": ["o show=1 labels=0 ...", "w <world id>", "p Home|0|12|64|-30|3|2|n", ...]}`.

use crate::json::{num, push_i, push_u};

/// The 16 Minecraft text colors.
pub const COLOR_NAMES: [&str; 16] = [
    "Black", "Dark Blue", "Dark Green", "Dark Aqua", "Dark Red", "Dark Purple", "Gold", "Gray",
    "Dark Gray", "Blue", "Green", "Aqua", "Red", "Light Purple", "Yellow", "White",
];
const ARGB: [u32; 16] = [
    0xFF000000, 0xFF0000AA, 0xFF00AA00, 0xFF00AAAA, 0xFFAA0000, 0xFFAA00AA, 0xFFFFAA00, 0xFFAAAAAA,
    0xFF555555, 0xFF5555FF, 0xFF55FF55, 0xFF55FFFF, 0xFFFF0000, 0xFFFF55FF, 0xFFFFFF55, 0xFFFFFFFF,
];
pub const COLORS: [[f32; 4]; 16] = {
    let mut out = [[0.0; 4]; 16];
    let mut i = 0;
    while i < 16 {
        let c = ARGB[i];
        out[i] = [((c >> 16) & 0xFF) as f32 / 255.0, ((c >> 8) & 0xFF) as f32 / 255.0, (c & 0xFF) as f32 / 255.0, 1.0];
        i += 1;
    }
    out
};
/// The palette as JSON, so the HUD does not format colours every time.
pub const COLOR_JSON: [&str; 16] = [
    "[0,0,0,1]", "[0,0,0.667,1]", "[0,0.667,0,1]", "[0,0.667,0.667,1]", "[0.667,0,0,1]", "[0.667,0,0.667,1]",
    "[1,0.667,0,1]", "[0.667,0.667,0.667,1]", "[0.333,0.333,0.333,1]", "[0.333,0.333,1,1]", "[0.333,1,0.333,1]",
    "[0.333,1,1,1]", "[1,0,0,1]", "[1,0.333,1,1]", "[1,1,0.333,1]", "[1,1,1,1]",
];
/// The same colours lightened where they are too dark to read as text over the world.
pub const TEXT_JSON: [&str; 16] = [
    "[0.6,0.6,0.6,1]", "[0.45,0.45,1,1]", "[0.3,0.85,0.3,1]", "[0.3,0.85,0.85,1]", "[1,0.35,0.35,1]", "[0.9,0.4,0.9,1]",
    "[1,0.667,0,1]", "[0.667,0.667,0.667,1]", "[0.6,0.6,0.6,1]", "[0.333,0.333,1,1]", "[0.333,1,0.333,1]",
    "[0.333,1,1,1]", "[1,0.2,0.2,1]", "[1,0.333,1,1]", "[1,1,0.333,1]", "[1,1,1,1]",
];

/// Icons drawn with Bedrock's font, picked with a slider in the menu.
pub const ICONS: [&str; 20] = [
    "★", "♥", "⌂", "⚑", "⚔", "☠", "✿", "☀", "☽", "♦", "♣", "♠", "♪", "✚", "▲", "●", "⚓", "☺", "⚡", "✖",
];
pub const ICON_NAMES: [&str; 20] = [
    "Star", "Heart", "House", "Flag", "Swords", "Skull", "Flower", "Sun", "Moon", "Diamond", "Club",
    "Spade", "Note", "Cross", "Arrow", "Dot", "Anchor", "Smile", "Bolt", "X",
];
/// Names picked with a slider in the menu (the menu has no text boxes); a number is added when
/// the name is already used.
pub const NAMES: [&str; 24] = [
    "Waypoint", "Home", "Base", "Farm", "Mine", "Portal", "Village", "Spawn", "Storage", "Nether Hub",
    "Fortress", "Bastion", "Stronghold", "End City", "Monument", "Outpost", "Trial Chamber",
    "Ancient City", "Temple", "Shipwreck", "Cave", "Diamonds", "Friend", "Build",
];
/// Icon 0 is the first two letters of the name in a box. The rest are `ICONS`.
pub const ICON_COUNT: usize = ICONS.len() + 1;
pub const INITIALS: u8 = 0;
/// Waypoint sets.
pub const SETS: [&str; 6] = ["Default", "Bases", "Farms", "Travel", "Mining", "Loot"];

/// How the menu orders the list.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Sort {
    #[default]
    Distance,
    Name,
    Color,
    Newest,
}

impl Sort {
    pub const NAMES: [&'static str; 4] = ["Distance", "Name (A-Z)", "Color", "Newest"];
    pub fn index(self) -> usize {
        self as usize
    }
    pub fn from_index(index: usize) -> Self {
        match index {
            1 => Self::Name,
            2 => Self::Color,
            3 => Self::Newest,
            _ => Self::Distance,
        }
    }
}

pub const DEATH_ICON: u8 = 6;
pub const TEMP_ICON: u8 = 4;
pub const RED: u8 = 12;
pub const YELLOW: u8 = 14;
/// Colours given to new waypoints in turn: bright ones, never red (the death point's).
pub const NEW_COLORS: [u8; 7] = [11, 10, 14, 6, 13, 9, 15];
/// Old (v1.0) colour indices in the new palette.
const OLD_COLORS: [u8; 8] = [15, 12, 6, 14, 10, 11, 9, 13];

pub const MAX_NAME: usize = 24;
/// Saved waypoints across every world; the settings file must stay under 16 KB.
pub const MAX_SAVED: usize = 120;
const MAX_SAVE_BYTES: usize = 15_000;
const FORMAT: &str = "waypoints2";
const OLD_FORMAT: &str = "waypoints1";

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Kind {
    #[default]
    Normal,
    Death,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct Waypoint {
    pub name: String,
    pub dim: i32,
    pub pos: [i32; 3],
    pub color: u8,
    pub icon: u8,
    pub kind: Kind,
    /// Kept but not shown.
    pub hidden: bool,
    /// Index into `SETS`.
    pub set: u8,
    /// Runtime identity for the menu; not saved.
    pub id: u32,
    /// Set once you have been away from it, so a new waypoint is not "reached" at once.
    pub armed: bool,
    /// The saved line, rebuilt when empty.
    line: String,
}

impl Waypoint {
    pub fn new(name: &str, dim: i32, pos: [i32; 3], color: u8, icon: u8, kind: Kind) -> Self {
        Waypoint { name: clean_name(name), dim, pos, color, icon, kind, ..Waypoint::default() }
    }

    /// The icon's text: a symbol, or the first two letters of the name in a box.
    pub fn glyph(&self) -> &str {
        if self.boxed() {
            let mut end = 0;
            for (n, (i, c)) in self.name.char_indices().enumerate() {
                if n == 2 {
                    break;
                }
                end = i + c.len_utf8();
            }
            if end == 0 { "?" } else { &self.name[..end] }
        } else {
            ICONS[(usize::from(self.icon) + ICONS.len() - 1) % ICONS.len()]
        }
    }

    /// Letters in a box.
    pub fn boxed(&self) -> bool {
        self.icon == INITIALS
    }

    pub fn icon_name(&self) -> &'static str {
        if self.boxed() { "Initials" } else { ICON_NAMES[(usize::from(self.icon) + ICONS.len() - 1) % ICONS.len()] }
    }

    pub fn color(&self) -> usize {
        usize::from(self.color) % COLORS.len()
    }

    /// Call after changing a saved field.
    pub fn touch(&mut self) {
        self.line.clear();
    }

    fn encode(&mut self) -> &str {
        if self.line.is_empty() {
            let line = &mut self.line;
            line.push_str(&self.name);
            line.push('|');
            push_i(line, i64::from(self.dim));
            for v in self.pos {
                line.push('|');
                push_i(line, i64::from(v));
            }
            line.push('|');
            push_u(line, u64::from(self.color));
            line.push('|');
            push_u(line, u64::from(self.icon));
            line.push_str(if self.kind == Kind::Death { "|d" } else { "|n" });
            if self.hidden {
                line.push_str("|h");
            }
            if self.set != 0 {
                line.push_str("|s");
                push_u(line, u64::from(self.set));
            }
        }
        &self.line
    }

    fn decode(line: &str, old: bool) -> Option<Waypoint> {
        let mut parts = line.split('|');
        let name = parts.next()?;
        let mut numbers = [0i32; 6];
        for slot in &mut numbers {
            *slot = parse_int(parts.next()?)?;
        }
        let kind = if parts.next() == Some("d") { Kind::Death } else { Kind::Normal };
        let mut hidden = false;
        let mut set = 0u8;
        for flag in parts {
            if flag == "h" {
                hidden = true;
            } else if let Some(n) = flag.strip_prefix('s').and_then(parse_int) {
                set = n.rem_euclid(SETS.len() as i32) as u8;
            }
        }
        let [dim, x, y, z, mut color, mut icon] = numbers;
        if old {
            // v1.0 had 8 colours and no initials icon.
            color = i32::from(OLD_COLORS[color.rem_euclid(8) as usize]);
            icon = icon.rem_euclid(ICONS.len() as i32) + 1;
        }
        let repaired = old;
        let color = color.rem_euclid(COLORS.len() as i32) as u8;
        let icon = icon.rem_euclid(ICON_COUNT as i32) as u8;
        let plain = !name.is_empty() && name.len() <= MAX_NAME && name.bytes().all(|b| b >= 0x20 && b != b'"' && b != b'\\');
        let mut point = Waypoint {
            name: if plain { name.to_string() } else { clean_name(name) },
            dim,
            pos: [x, y, z],
            color,
            icon,
            kind,
            hidden,
            set,
            ..Waypoint::default()
        };
        if point.name.is_empty() {
            point.name.push_str("Waypoint");
        }
        // The line read is already the saved form, unless something was repaired.
        if plain && !repaired && color == numbers[4] as u8 && icon == numbers[5] as u8 {
            point.line = line.to_string();
        }
        Some(point)
    }
}

fn parse_int(text: &str) -> Option<i32> {
    let (negative, digits) = match text.strip_prefix('-') {
        Some(rest) => (true, rest),
        None => (false, text),
    };
    if digits.is_empty() || digits.len() > 9 {
        return None;
    }
    let mut value: i32 = 0;
    for b in digits.bytes() {
        if !b.is_ascii_digit() {
            return None;
        }
        value = value * 10 + i32::from(b - b'0');
    }
    Some(if negative { -value } else { value })
}

fn parse_float(text: &str) -> Option<f64> {
    let (whole, fraction) = text.split_once('.').unwrap_or((text, ""));
    let mut value = f64::from(parse_int(whole)?.abs());
    let mut scale = 0.1;
    for b in fraction.bytes().take(3) {
        if !b.is_ascii_digit() {
            return None;
        }
        value += f64::from(b - b'0') * scale;
        scale /= 10.0;
    }
    Some(if whole.starts_with('-') { -value } else { value })
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct World {
    pub id: String,
    pub points: Vec<Waypoint>,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Labels {
    /// Name and distance appear when you look at the waypoint.
    #[default]
    Looking,
    Always,
    Never,
}

impl Labels {
    pub const NAMES: [&'static str; 3] = ["When Looking", "Always", "Never"];
    pub fn index(self) -> usize {
        match self {
            Self::Looking => 0,
            Self::Always => 1,
            Self::Never => 2,
        }
    }
    pub fn from_index(index: usize) -> Self {
        match index {
            0 => Self::Looking,
            1 => Self::Always,
            _ => Self::Never,
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct Settings {
    pub show: bool,
    pub labels: Labels,
    pub look_angle: f64,
    /// The name under each waypoint's box in the world (the distance always shows).
    pub show_names: bool,
    pub icon_size: f64,
    pub beams: bool,
    pub beams_through_walls: bool,
    pub beam_width: f64,
    pub death_point: bool,
    pub death_clear: bool,
    pub menu_key: String,
    pub add_key: String,
    /// Menu list order and which set it shows (`None` = all sets).
    pub sort: Sort,
    pub filter: Option<u8>,
    pub worlds: Vec<World>,
    pub next_id: u32,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            show: true,
            labels: Labels::Looking,
            look_angle: 8.0,
            show_names: true,
            icon_size: 1.5,
            beams: true,
            beams_through_walls: false,
            beam_width: 2.0,
            death_point: true,
            death_clear: true,
            // Empty: F10 opens the menu (Mod Menu).
            menu_key: String::new(),
            add_key: "KeyP".into(),
            sort: Sort::Distance,
            filter: None,
            worlds: Vec::new(),
            next_id: 0,
        }
    }
}

pub const MIN_ANGLE: f64 = 2.0;
pub const MAX_ANGLE: f64 = 30.0;
pub const MIN_ICON: f64 = 0.5;
pub const MAX_ICON: f64 = 3.0;
pub const MIN_BEAM: f64 = 1.0;
pub const MAX_BEAM: f64 = 6.0;

pub fn dimension_name(dim: i32) -> &'static str {
    match dim {
        0 => "Overworld",
        1 => "Nether",
        2 => "End",
        _ => "This dimension",
    }
}

/// Keeps what can be shown and saved: no control characters, no `|`, a bounded length.
pub fn clean_name(text: &str) -> String {
    text.chars()
        .filter(|c| !c.is_control() && *c != '|' && *c != '"' && *c != '\\')
        .take(MAX_NAME)
        .collect::<String>()
        .trim()
        .to_string()
}

fn clamp(value: f64, min: f64, max: f64) -> f64 {
    if value.is_finite() { value.clamp(min, max) } else { min }
}

/// Reads the settings file a little at a time: a big file costs more fuel than one frame has.
pub struct Loader {
    text: String,
    pos: usize,
    world: Option<usize>,
    /// A v1.0 file: colours and icons are moved to the new lists.
    old: bool,
}

/// Bytes read per step.
pub const LOAD_STEP: usize = 400;

impl Loader {
    pub fn new(text: String) -> Self {
        Loader { text, pos: 0, world: None, old: false }
    }

    /// The next string of the top-level array, unescaped; None at the end.
    fn next_string(&mut self) -> Option<std::borrow::Cow<'_, str>> {
        let bytes = self.text.as_bytes();
        while self.pos < bytes.len() && bytes[self.pos] != b'"' {
            self.pos += 1;
        }
        if self.pos >= bytes.len() {
            return None;
        }
        self.pos += 1;
        let start = self.pos;
        let mut plain = true;
        while self.pos < bytes.len() {
            match bytes[self.pos] {
                b'"' => break,
                b'\\' => {
                    plain = false;
                    self.pos += 2;
                }
                _ => self.pos += 1,
            }
        }
        let end = self.pos.min(bytes.len());
        self.pos = end + 1;
        let raw = self.text.get(start..end)?;
        if plain {
            return Some(std::borrow::Cow::Borrowed(raw));
        }
        let mut out = String::with_capacity(raw.len());
        let mut chars = raw.chars();
        while let Some(c) = chars.next() {
            if c != '\\' {
                out.push(c);
                continue;
            }
            match chars.next() {
                Some('n') => out.push('\n'),
                Some('t') => out.push('\t'),
                Some('u') => {
                    let hex: String = chars.by_ref().take(4).collect();
                    if let Some(c) = u32::from_str_radix(&hex, 16).ok().and_then(char::from_u32) {
                        out.push(c);
                    }
                }
                Some(other) => out.push(other),
                None => {}
            }
        }
        Some(std::borrow::Cow::Owned(out))
    }

    /// Reads up to `LOAD_STEP` bytes into `settings`; true once the whole file is read.
    pub fn step(&mut self, settings: &mut Settings) -> bool {
        let budget_end = self.pos + LOAD_STEP;
        while self.pos < budget_end {
            let mut world = self.world;
            let mut old = self.old;
            let Some(entry) = self.next_string() else {
                return true;
            };
            if entry == OLD_FORMAT {
                old = true;
            } else if let Some(rest) = entry.strip_prefix("o ") {
                settings.read_options(rest);
            } else if let Some(id) = entry.strip_prefix("w ") {
                let index = settings.worlds.iter().position(|w| w.id == id).unwrap_or_else(|| {
                    settings.worlds.push(World { id: id.to_string(), points: Vec::new() });
                    settings.worlds.len() - 1
                });
                world = Some(index);
            } else if let (Some(line), Some(index)) = (entry.strip_prefix("p "), world) {
                if let Some(mut point) = Waypoint::decode(line, old) {
                    settings.next_id += 1;
                    point.id = settings.next_id;
                    point.armed = true;
                    // Repaired or old lines are written now, a few per frame, not all at once
                    // on the first save.
                    point.encode();
                    settings.worlds[index].points.push(point);
                }
            }
            drop(entry);
            self.world = world;
            self.old = old;
        }
        self.pos >= self.text.len()
    }
}

impl Settings {
    /// Reads everything at once; fine for tests and small files.
    pub fn from_text(text: &str) -> Self {
        let mut settings = Settings::default();
        let mut loader = Loader::new(text.to_string());
        while !loader.step(&mut settings) {}
        settings.sanitize();
        settings
    }

    fn read_options(&mut self, text: &str) {
        for pair in text.split(' ') {
            let Some((key, value)) = pair.split_once('=') else { continue };
            let on = value == "1";
            let number = parse_float(value);
            match key {
                "show" => self.show = on,
                "names" => self.show_names = on,
                "labels" => self.labels = Labels::from_index(number.unwrap_or(0.0) as usize),
                "look" => self.look_angle = number.unwrap_or(self.look_angle),
                "icon" => self.icon_size = number.unwrap_or(self.icon_size),
                "beams" => self.beams = on,
                "walls" => self.beams_through_walls = on,
                "width" => self.beam_width = number.unwrap_or(self.beam_width),
                "death" => self.death_point = on,
                "dclear" => self.death_clear = on,
                "menu" if valid_key(value) => self.menu_key = value.to_string(),
                // v1.6 saves the key as `addkey`: older files had B, now P.
                "addkey" if valid_key(value) => self.add_key = value.to_string(),
                "sort" => self.sort = Sort::from_index(number.unwrap_or(0.0) as usize),
                "filter" => {
                    self.filter = number.map(|n| n as u8).filter(|n| usize::from(*n) < SETS.len());
                }
                _ => {}
            }
        }
        self.sanitize();
    }

    /// The saved form: empty worlds are left out.
    pub fn save_text(&mut self) -> String {
        let mut out = String::with_capacity(512 + self.saved_count() * 40);
        let flag = |out: &mut String, key: &str, on: bool| {
            out.push(' ');
            out.push_str(key);
            out.push_str(if on { "=1" } else { "=0" });
        };
        // The host keeps settings as a JSON object; the entries are one array inside it.
        out.push_str("{\"");
        out.push_str(FORMAT);
        out.push_str("\":[\"o show=");
        out.push(if self.show { '1' } else { '0' });
        flag(&mut out, "names", self.show_names);
        out.push_str(" labels=");
        push_u(&mut out, self.labels.index() as u64);
        out.push_str(" look=");
        num(&mut out, self.look_angle);
        out.push_str(" icon=");
        num(&mut out, self.icon_size);
        flag(&mut out, "beams", self.beams);
        flag(&mut out, "walls", self.beams_through_walls);
        out.push_str(" width=");
        num(&mut out, self.beam_width);
        flag(&mut out, "death", self.death_point);
        flag(&mut out, "dclear", self.death_clear);
        out.push_str(" menu=");
        out.push_str(&self.menu_key);
        out.push_str(" addkey=");
        out.push_str(&self.add_key);
        out.push_str(" sort=");
        push_u(&mut out, self.sort.index() as u64);
        if let Some(filter) = self.filter {
            out.push_str(" filter=");
            push_u(&mut out, u64::from(filter));
        }
        out.push('"');
        for world in &mut self.worlds {
            if world.points.is_empty() {
                continue;
            }
            out.push_str(",\"w ");
            crate::json::escape_into(&mut out, &world.id);
            out.push('"');
            for point in &mut world.points {
                out.push_str(",\"p ");
                out.push_str(point.encode());
                out.push('"');
            }
        }
        out.push_str("]}");
        out
    }

    pub fn sanitize(&mut self) {
        self.look_angle = clamp(self.look_angle, MIN_ANGLE, MAX_ANGLE);
        self.icon_size = clamp(self.icon_size, MIN_ICON, MAX_ICON);
        self.beam_width = clamp(self.beam_width, MIN_BEAM, MAX_BEAM);
        let d = Settings::default();
        if !valid_key(&self.menu_key) || self.menu_key == "F10" {
            self.menu_key.clear();
        }
        for (key, default) in [(&mut self.add_key, d.add_key)] {
            if !valid_key(key) {
                *key = default;
            }
        }
    }

    pub fn world(&self, id: &str) -> Option<&World> {
        self.worlds.iter().find(|world| world.id == id)
    }

    fn world_mut(&mut self, id: &str) -> &mut World {
        if let Some(index) = self.worlds.iter().position(|world| world.id == id) {
            return &mut self.worlds[index];
        }
        self.worlds.push(World { id: id.to_string(), points: Vec::new() });
        self.worlds.last_mut().unwrap()
    }

    pub fn find(&self, world: &str, id: u32) -> Option<&Waypoint> {
        self.world(world)?.points.iter().find(|point| point.id == id)
    }

    pub fn find_mut(&mut self, world: &str, id: u32) -> Option<&mut Waypoint> {
        self.worlds.iter_mut().find(|w| w.id == world)?.points.iter_mut().find(|point| point.id == id)
    }

    fn saved_count(&self) -> usize {
        self.worlds.iter().map(|w| w.points.len()).sum()
    }

    fn saved_bytes(&mut self) -> usize {
        let mut bytes = 300;
        for world in &mut self.worlds {
            bytes += world.id.len() + 8;
            for point in &mut world.points {
                bytes += point.encode().len() + 6;
            }
        }
        bytes
    }

    /// A name not used yet in this world: "Home", then "Home 1", "Home 2", ... (one pass: string
    /// compares cost fuel).
    pub fn free_name(&self, world: &str, base: &str, except: u32) -> String {
        let mut base_used = base == "Waypoint";
        let mut taken = [false; 256];
        if let Some(w) = self.world(world) {
            for p in w.points.iter().filter(|p| p.id != except) {
                let Some(rest) = p.name.strip_prefix(base) else { continue };
                if rest.is_empty() {
                    base_used = true;
                } else if let Some(n) = rest.strip_prefix(' ').and_then(parse_int) {
                    if (0..256).contains(&n) {
                        taken[n as usize] = true;
                    }
                }
            }
        }
        if !base_used {
            return base.to_string();
        }
        let n = (1..256).find(|n| !taken[*n]).unwrap_or(256);
        let mut name = String::with_capacity(base.len() + 4);
        name.push_str(base);
        name.push(' ');
        push_u(&mut name, n as u64);
        name
    }

    /// Adds a waypoint and returns its id, or why it could not be added.
    pub fn add(&mut self, world: &str, mut point: Waypoint) -> Result<u32, &'static str> {
        if point.kind == Kind::Normal && self.saved_count() >= MAX_SAVED {
            return Err("too many waypoints saved; delete some first");
        } else if self.saved_bytes() + 60 > MAX_SAVE_BYTES + if point.kind == Kind::Death { 1000 } else { 0 } {
            return Err("the waypoint file is full; delete some first");
        }
        self.next_id += 1;
        point.id = self.next_id;
        point.touch();
        let id = point.id;
        self.world_mut(world).points.push(point);
        Ok(id)
    }

    pub fn remove(&mut self, world: &str, id: u32) -> Option<Waypoint> {
        let w = self.worlds.iter_mut().find(|w| w.id == world)?;
        let index = w.points.iter().position(|p| p.id == id)?;
        Some(w.points.remove(index))
    }

    /// Keeps only the newest death point of a world.
    pub fn set_death(&mut self, world: &str, dim: i32, pos: [i32; 3]) -> Result<u32, &'static str> {
        self.world_mut(world).points.retain(|p| p.kind != Kind::Death);
        self.add(world, Waypoint::new("Death", dim, pos, RED, DEATH_ICON, Kind::Death))
    }

    pub fn remove_kind(&mut self, world: &str, kind: Kind) -> usize {
        let Some(w) = self.worlds.iter_mut().find(|w| w.id == world) else {
            return 0;
        };
        let before = w.points.len();
        w.points.retain(|p| p.kind != kind);
        before - w.points.len()
    }
}

pub fn valid_key(key: &str) -> bool {
    !key.is_empty() && key.len() <= 32 && key.bytes().all(|b| b.is_ascii_alphanumeric())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn point(name: &str, kind: Kind) -> Waypoint {
        Waypoint::new(name, 0, [1, 64, -3], 2, 4, kind)
    }

    #[test]
    fn worlds_and_kinds_are_kept_apart() {
        let mut s = Settings::default();
        let home = s.add("server \"a\"", point("Home", Kind::Normal)).unwrap();
        s.add("server-b", Waypoint { dim: 1, ..point("Fortress", Kind::Normal) }).unwrap();
        s.set_death("server \"a\"", 0, [5, 12, 5]).unwrap();
        s.set_death("server \"a\"", 1, [-7, -40, 7]).unwrap();
        s.labels = Labels::Always;
        s.look_angle = 12.5;
        s.add_key = "KeyH".into();
        let a = s.world("server \"a\"").unwrap();
        assert_eq!(a.points.iter().filter(|p| p.kind == Kind::Death).count(), 1);
        let text = s.save_text();
        let parsed: serde_json::Value = serde_json::from_str(&text).unwrap();
        assert!(parsed.is_object() && parsed[FORMAT].is_array(), "{text}");
        let back = Settings::from_text(&text);
        let a = back.world("server \"a\"").unwrap();
        assert_eq!(a.points.len(), 2);
        let death = a.points.iter().find(|p| p.kind == Kind::Death).unwrap();
        assert_eq!((death.dim, death.pos), (1, [-7, -40, 7]));
        assert_eq!(back.world("server-b").unwrap().points[0].dim, 1);
        assert_eq!((back.labels, back.look_angle, back.add_key.as_str()), (Labels::Always, 12.5, "KeyH"));
        let h = &a.points[0];
        assert_eq!((h.name.as_str(), h.color, h.icon, h.pos), ("Home", 2, 4, [1, 64, -3]));
        assert_eq!(s.find("server \"a\"", home).unwrap().name, "Home");
        assert_eq!(s.free_name("server \"a\"", "Home", 0), "Home 1");
        assert_eq!(s.free_name("server \"a\"", "Farm", 0), "Farm");
        assert_eq!(s.free_name("server \"a\"", "Waypoint", 0), "Waypoint 1");
        assert!(s.remove("server \"a\"", home).is_some());
    }

    #[test]
    fn initials_hidden_and_old_files() {
        let mut s = Settings::default();
        let id = s.add("w", Waypoint::new("Home", 0, [1, 2, 3], 6, INITIALS, Kind::Normal)).unwrap();
        s.find_mut("w", id).unwrap().hidden = true;
        s.find_mut("w", id).unwrap().set = 3;
        s.sort = Sort::Name;
        s.filter = Some(2);
        s.find_mut("w", id).unwrap().touch();
        let back = Settings::from_text(&s.save_text());
        let p = &back.world("w").unwrap().points[0];
        assert!(p.hidden && p.boxed() && p.glyph() == "Ho" && COLOR_NAMES[p.color()] == "Gold");
        assert_eq!((SETS[usize::from(p.set)], back.sort, back.filter), ("Travel", Sort::Name, Some(2)));
        assert_eq!(Waypoint::new("É", 0, [0; 3], 0, INITIALS, Kind::Normal).glyph(), "É");
        assert_eq!(Waypoint::new("x", 0, [0; 3], 0, DEATH_ICON, Kind::Normal).glyph(), "☠");
        assert_eq!(Waypoint::new("x", 0, [0; 3], 0, TEMP_ICON, Kind::Normal).glyph(), "⚑");
        // A v1.0 file: yellow star becomes the new yellow, star icon.
        let old = Settings::from_text(r#"["waypoints1","o show=1","w w","p Farm|0|1|2|3|3|0|n"]"#);
        let p = &old.world("w").unwrap().points[0];
        assert_eq!((COLOR_NAMES[p.color()], p.glyph()), ("Yellow", "★"));
        assert_eq!(COLORS[6], [1.0, 0.6666667, 0.0, 1.0]);
    }

    #[test]
    fn limits_hold_and_bad_files_are_survived() {
        let mut s = Settings::default();
        let mut added = 0;
        while s.add("w", point("Ancient City 1234567890", Kind::Normal)).is_ok() {
            added += 1;
        }
        assert!(added >= 100, "{added}");
        // The death point is still saved when the list is full.
        assert!(s.set_death("w", 0, [1, 2, 3]).is_ok());
        assert!(s.save_text().len() <= MAX_SAVE_BYTES + 100, "{}", s.save_text().len());
        let bad_file = "[\"o look=900 menu= \",\"p orphan|0|1|2|3|4|5|n\",\"w x\",\"p bad|a\",\"p ok|2|1|2|3|99|-1|d\"]";
        for bad in ["", "{}", "[1,2", "[\"unterminated", bad_file] {
            let s = Settings::from_text(bad);
            assert_eq!(s.menu_key, "");
            if bad.contains("ok|") {
                assert_eq!(s.look_angle, MAX_ANGLE);
                let p = &s.world("x").unwrap().points;
                assert_eq!(p.len(), 1);
                assert_eq!((p[0].color, p[0].icon, p[0].kind), (3, 20, Kind::Death));
            }
        }
    }
}
