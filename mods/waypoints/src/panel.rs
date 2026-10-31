//! The Waypoints menu: a custom screen (see tools/gen_surface.py) with four views — the list,
//! "New Waypoint", "Edit Waypoint" and Settings. The screen document never changes; the mod
//! only sends `#bindings` (text, colours, which view shows) and reacts to the buttons, which
//! press backing controls by index.

use crate::config::{
    self, COLOR_JSON, COLOR_NAMES, COLORS, ICON_COUNT, Kind, MAX_NAME, SETS, Settings, Sort, Waypoint, valid_key,
};
use crate::json::{escape_into, num, push_i, push_u, q};
use crate::surface_controls::{CONTROLS, DOCUMENT_ESCAPED, HEIGHT, Kind as ControlKind, WIDTH};

/// Where you are, for menu actions that add or move waypoints.
#[derive(Clone, Debug, Default)]
pub struct Context {
    pub world: Option<String>,
    pub dim: i32,
    pub feet: [i32; 3],
    pub clock: f64,
    /// Bumped by the mod whenever waypoints change, so the cached list order is found again.
    pub revision: u64,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct Effect {
    pub save: bool,
    /// The menu's values changed and are sent again.
    pub republish: bool,
    /// Waypoints changed: the HUD and drawings are rebuilt.
    pub changed: bool,
    pub message: Option<String>,
}

impl Effect {
    pub fn merge(mut self, other: Effect) -> Effect {
        self.save |= other.save;
        self.republish |= other.republish;
        self.changed |= other.changed;
        if other.message.is_some() {
            self.message = other.message;
        }
        self
    }

    fn redraw() -> Effect {
        Effect { save: true, republish: true, changed: true, message: None }
    }

    fn show() -> Effect {
        Effect { republish: true, ..Effect::default() }
    }

    fn say(text: String) -> Effect {
        Effect { republish: true, message: Some(text), ..Effect::default() }
    }

    fn setting() -> Effect {
        Effect { save: true, republish: true, changed: true, message: None }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Bind {
    Menu,
    Add,
}

impl Bind {
    const ALL: [Bind; 2] = [Bind::Menu, Bind::Add];
    fn id(self) -> &'static str {
        match self {
            Self::Menu => "menu_key",
            Self::Add => "add_key",
        }
    }
    fn slot(self, settings: &mut Settings) -> &mut String {
        match self {
            Self::Menu => &mut settings.menu_key,
            Self::Add => &mut settings.add_key,
        }
    }
    fn key(self, settings: &Settings) -> &str {
        match self {
            Self::Menu if settings.menu_key.is_empty() => "None",
            Self::Menu => &settings.menu_key,
            Self::Add => &settings.add_key,
        }
    }
}

/// A key no keyboard sends: the panel's toggle key while the player has set none, so the
/// menu opens only from Mods Core (F10 belongs to Mods Core).
pub const NO_KEY: &str = "F24";

/// Whether the player set a menu key of their own (F10 belongs to Mods Core).
pub fn has_own_key(key: &str) -> bool {
    !key.is_empty() && key != "None" && key != "F10" && key != NO_KEY
}

/// List rows on one page of the list view.
pub const ROWS: usize = 7;
const STEPS: [i32; 3] = [1, 10, 100];

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum View {
    #[default]
    List,
    New,
    Edit,
    Settings,
}

/// The waypoint being made on the "New Waypoint" screen; added with "Create here".
#[derive(Clone, Debug, PartialEq)]
pub struct Draft {
    pub name: String,
    pub color: u8,
    pub icon: u8,
    pub set: u8,
    pub hidden: bool,
}

#[derive(Default)]
pub struct PanelState {
    capturing: Option<Bind>,
    pub view: View,
    /// The waypoint on the edit screen.
    pub selected: Option<u32>,
    draft: Option<Draft>,
    page: usize,
    step: usize,
    confirm_delete: bool,
    /// The waypoint shown in each list row, as last sent.
    slots: [Option<u32>; ROWS],
    order: Option<(OrderKey, Vec<usize>)>,
    /// The menu key changed: the whole menu is sent again.
    pub rebuild: bool,
    /// The name box has the keyboard: keys type into it.
    pub typing: bool,
    /// The Settings screen's values, rebuilt only when a setting changes.
    settings_block: Option<(SettingsKey, String)>,
    /// Until when (mod clock) the "look at the game chat" note shows.
    pub kofi_until: f64,
}

#[derive(Clone, Debug, PartialEq)]
struct SettingsKey {
    flags: [bool; 6],
    numbers: [u64; 2],
}

impl SettingsKey {
    fn of(s: &Settings) -> Self {
        SettingsKey {
            flags: [s.show, s.beams, s.beams_through_walls, s.death_point, s.death_clear, s.show_names],
            numbers: [s.icon_size.to_bits(), s.beam_width.to_bits()],
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
struct OrderKey {
    revision: u64,
    world: Option<String>,
    dim: i32,
    sort: Sort,
    filter: Option<u8>,
    near: [i32; 3],
}

fn distance_sq(a: [i32; 3], b: [i32; 3]) -> i64 {
    (0..3).map(|i| i64::from(a[i] - b[i]).pow(2)).sum()
}

fn distance(a: [i32; 3], b: [i32; 3]) -> u32 {
    (distance_sq(a, b) as f64).sqrt().round() as u32
}

/// Up to 24 bytes of a name as three numbers: comparing them sorts names like comparing the
/// text, for a fraction of the fuel.
fn name_key(name: &str) -> [u64; 3] {
    let mut bytes = [0u8; 24];
    let n = name.len().min(24);
    bytes[..n].copy_from_slice(&name.as_bytes()[..n]);
    let word = |i: usize| u64::from_be_bytes(bytes[i * 8..i * 8 + 8].try_into().unwrap_or([0; 8]));
    [word(0), word(1), word(2)]
}

/// This dimension's waypoints in the chosen set and order, as indices.
fn compute_order(settings: &Settings, ctx: &Context) -> Vec<usize> {
    let Some(world) = ctx.world.as_deref().and_then(|w| settings.world(w)) else { return Vec::new() };
    let wanted = |p: &Waypoint| p.dim == ctx.dim && settings.filter.is_none_or(|set| p.set == set);
    let [fx, fy, fz] = ctx.feet.map(i64::from);
    let near = |p: &Waypoint| {
        let (dx, dy, dz) = (i64::from(p.pos[0]) - fx, i64::from(p.pos[1]) - fy, i64::from(p.pos[2]) - fz);
        (dx * dx + dy * dy + dz * dz) as u64
    };
    if settings.sort == Sort::Name {
        let mut keyed: Vec<([u64; 3], u32)> = world
            .points
            .iter()
            .enumerate()
            .filter(|(_, p)| wanted(p))
            .map(|(i, p)| (name_key(&p.name), i as u32))
            .collect();
        keyed.sort_unstable();
        return keyed.into_iter().map(|(_, i)| i as usize).collect();
    }
    let mut keyed: Vec<(u64, u32)> = world
        .points
        .iter()
        .enumerate()
        .filter(|(_, p)| wanted(p))
        .map(|(i, p)| {
            let key = match settings.sort {
                Sort::Color => (u64::from(p.color) << 56) | near(p).min((1 << 56) - 1),
                Sort::Newest => u64::from(u32::MAX - p.id),
                _ => near(p),
            };
            (key, i as u32)
        })
        .collect();
    keyed.sort_unstable();
    keyed.into_iter().map(|(_, i)| i as usize).collect()
}

fn cycle(value: usize, len: usize, forward: bool) -> usize {
    if forward { (value + 1) % len } else { (value + len - 1) % len }
}

fn normal_count(settings: &Settings, ctx: &Context) -> usize {
    ctx.world
        .as_deref()
        .and_then(|w| settings.world(w))
        .map_or(0, |w| w.points.iter().filter(|p| p.kind == Kind::Normal).count())
}

fn default_draft(settings: &Settings, ctx: &Context) -> Draft {
    let count = normal_count(settings, ctx);
    let name = ctx.world.as_deref().map_or("Waypoint 1".to_string(), |w| settings.free_name(w, "Waypoint", 0));
    Draft {
        name,
        color: config::NEW_COLORS[count % config::NEW_COLORS.len()],
        icon: config::INITIALS,
        set: settings.filter.unwrap_or(0),
        hidden: false,
    }
}

/// Adds a waypoint where you stand (the hotkey).
pub fn add_here(settings: &mut Settings, ctx: &Context) -> (Effect, Option<u32>) {
    let draft = default_draft(settings, ctx);
    add_draft(settings, ctx, &draft)
}

fn add_draft(settings: &mut Settings, ctx: &Context, draft: &Draft) -> (Effect, Option<u32>) {
    let Some(world) = &ctx.world else {
        return (Effect::say("Waypoints: join a world first.".into()), None);
    };
    let typed = config::clean_name(&draft.name);
    let name = if typed.is_empty() { settings.free_name(world, "Waypoint", 0) } else { typed };
    let mut point = Waypoint::new(&name, ctx.dim, ctx.feet, draft.color, draft.icon, Kind::Normal);
    point.set = draft.set;
    point.hidden = draft.hidden;
    match settings.add(world, point) {
        Ok(id) => {
            let [x, y, z] = ctx.feet;
            let mut effect = Effect::redraw();
            effect.message = Some(format!("Waypoints: {name} added at {x} {y} {z}."));
            (effect, Some(id))
        }
        Err(why) => (Effect::say(format!("Waypoints: can't add it, {why}.")), None),
    }
}

/// The character a key types, if any (US key positions; `Semicolon` is the Spanish ñ key).
fn typed_char(key: &str, shift: bool) -> Option<char> {
    if let Some(letter) = key.strip_prefix("Key") {
        let c = letter.chars().next()?;
        if letter.len() == 1 && c.is_ascii_alphabetic() {
            return Some(if shift { c.to_ascii_uppercase() } else { c.to_ascii_lowercase() });
        }
    }
    if let Some(digit) = key.strip_prefix("Digit").or_else(|| key.strip_prefix("Numpad")) {
        if digit.len() == 1 {
            return digit.chars().next().filter(char::is_ascii_digit);
        }
    }
    Some(match key {
        "Space" => ' ',
        "Minus" => if shift { '_' } else { '-' },
        "Period" => '.',
        "Comma" => ',',
        "Semicolon" => if shift { 'Ñ' } else { 'ñ' },
        "Slash" => if shift { '?' } else { '/' },
        "Quote" => '\'',
        _ => return None,
    })
}

/// The same place in the other dimension: Nether coordinates are the Overworld's divided by 8.
fn copy_to_other_dimension(settings: &mut Settings, ctx: &Context, selected: u32) -> Effect {
    let Some(world) = &ctx.world else { return Effect::default() };
    let Some(point) = settings.find(world, selected) else { return Effect::default() };
    let (dim, pos) = match point.dim {
        0 => (1, [point.pos[0].div_euclid(8), point.pos[1], point.pos[2].div_euclid(8)]),
        1 => (0, [point.pos[0] * 8, point.pos[1], point.pos[2] * 8]),
        _ => return Effect::default(),
    };
    let mut copy = Waypoint::new(&point.name, dim, pos, point.color, point.icon, Kind::Normal);
    copy.hidden = point.hidden;
    copy.set = point.set;
    let name = copy.name.clone();
    match settings.add(world, copy) {
        Ok(_) => {
            let mut effect = Effect::redraw();
            let [x, y, z] = pos;
            effect.message =
                Some(format!("Waypoints: {name} copied to the {} at {x} {y} {z}.", config::dimension_name(dim)));
            effect
        }
        Err(why) => Effect::say(format!("Waypoints: can't copy it, {why}.")),
    }
}

fn order_key(settings: &Settings, ctx: &Context) -> OrderKey {
    OrderKey {
        revision: ctx.revision,
        world: ctx.world.clone(),
        dim: ctx.dim,
        sort: settings.sort,
        filter: settings.filter,
        near: if matches!(settings.sort, Sort::Distance | Sort::Color) {
            ctx.feet.map(|v| v.div_euclid(8))
        } else {
            [0; 3]
        },
    }
}

impl PanelState {
    /// Sorts the list again if it may have changed; true when it did. The mod calls this a
    /// frame before sending the values, since sorting many waypoints takes a share of a frame.
    pub fn prepare(&mut self, settings: &Settings, ctx: &Context) -> bool {
        let key = order_key(settings, ctx);
        if self.order.as_ref().is_some_and(|(shown, _)| *shown == key) {
            return false;
        }
        self.order = Some((key, compute_order(settings, ctx)));
        true
    }

    /// A key is being rebound, or the name box has the keyboard.
    pub fn is_capturing(&self) -> bool {
        self.capturing.is_some() || self.typing
    }

    /// Changes the waypoint on the edit screen with `f`.
    fn edit(&self, settings: &mut Settings, ctx: &Context, f: impl FnOnce(&mut Waypoint)) -> Effect {
        let Some(point) = ctx.world.as_deref().and_then(|w| settings.find_mut(w, self.selected?)) else {
            return Effect::default();
        };
        f(point);
        point.touch();
        Effect::redraw()
    }

    fn go(&mut self, view: View) -> Effect {
        self.view = view;
        self.confirm_delete = false;
        Effect::show()
    }

    pub fn event(&mut self, settings: &mut Settings, ctx: &Context, id: &str, _value: f32) -> Effect {
        if id == "name_box" {
            self.typing = !self.typing && matches!(self.view, View::New | View::Edit);
            self.capturing = None;
            return self.finish_name(settings, ctx);
        }
        // Anything else ends typing.
        let mut ended = Effect::default();
        if self.typing {
            self.typing = false;
            ended = self.finish_name(settings, ctx);
        }
        ended.merge(self.handle(settings, ctx, id))
    }

    /// An empty name gets "Waypoint N" back when typing ends.
    fn finish_name(&mut self, settings: &mut Settings, ctx: &Context) -> Effect {
        if self.typing {
            return Effect::show();
        }
        let Some(world) = ctx.world.clone() else { return Effect::show() };
        if self.view == View::New {
            if let Some(d) = &mut self.draft {
                if config::clean_name(&d.name).is_empty() {
                    d.name = settings.free_name(&world, "Waypoint", 0);
                }
            }
            return Effect::show();
        }
        if self.view == View::Edit {
            let Some(selected) = self.selected else { return Effect::show() };
            if settings.find(&world, selected).is_some_and(|p| p.name.trim().is_empty()) {
                let name = settings.free_name(&world, "Waypoint", selected);
                return self.edit(settings, ctx, |p| p.name = name);
            }
        }
        Effect::show()
    }

    /// Keys typed into the name box: letters, digits, space and a few marks; Backspace deletes,
    /// Enter or Escape finishes.
    pub fn type_keys(&mut self, settings: &mut Settings, ctx: &Context, pressed: &[String], shift: bool) -> Effect {
        if !self.typing {
            return Effect::default();
        }
        let mut name = match self.view {
            View::New => self.draft.as_ref().map(|d| d.name.clone()),
            View::Edit => ctx.world.as_deref().and_then(|w| settings.find(w, self.selected?)).map(|p| p.name.clone()),
            _ => None,
        }
        .unwrap_or_default();
        let before = name.clone();
        let mut finished = false;
        for key in pressed {
            match key.as_str() {
                "Backspace" => {
                    name.pop();
                }
                "Enter" | "NumpadEnter" | "Escape" | "Tab" => finished = true,
                _ => {
                    if let Some(c) = typed_char(key, shift) {
                        if name.chars().count() < MAX_NAME {
                            name.push(c);
                        }
                    }
                }
            }
        }
        let mut effect = Effect::default();
        if name != before {
            effect = match self.view {
                View::New => {
                    if let Some(d) = &mut self.draft {
                        d.name = name;
                    }
                    Effect::show()
                }
                _ => self.edit(settings, ctx, |p| p.name = name),
            };
        }
        if finished {
            self.typing = false;
            effect = effect.merge(self.finish_name(settings, ctx));
        }
        effect
    }

    fn handle(&mut self, settings: &mut Settings, ctx: &Context, id: &str) -> Effect {
        for bind in Bind::ALL {
            if id == bind.id() {
                self.capturing = if self.capturing == Some(bind) { None } else { Some(bind) };
                return Effect::show();
            }
        }
        if id != "delete" {
            self.confirm_delete = false;
        }
        if let Some(n) = id.strip_prefix("row_").and_then(|n| n.parse::<usize>().ok()) {
            let Some(picked) = self.slots.get(n).copied().flatten() else { return Effect::default() };
            self.selected = Some(picked);
            return self.go(View::Edit);
        }
        let new = self.view == View::New;
        if let Some(n) = id.strip_prefix("color_").and_then(|n| n.parse::<u8>().ok()) {
            let color = n % COLORS.len() as u8;
            if new {
                if let Some(d) = &mut self.draft {
                    d.color = color;
                }
                return Effect::show();
            }
            return self.edit(settings, ctx, |p| p.color = color);
        }
        if let Some(n) = id.strip_prefix("icon_").and_then(|n| n.parse::<u8>().ok()) {
            let icon = n % ICON_COUNT as u8;
            if new {
                if let Some(d) = &mut self.draft {
                    d.icon = icon;
                }
                return Effect::show();
            }
            return self.edit(settings, ctx, |p| p.icon = icon);
        }
        let step = STEPS[self.step % STEPS.len()];
        match id {
            // --- Navigation ---
            "open_settings" => self.go(View::Settings),
            "kofi" => {
                self.kofi_until = ctx.clock + 4.0;
                Effect {
                    republish: true,
                    message: Some("Like this mod? Buy me a coffee: https://ko-fi.com/emilchill (open the chat and click the link)".into()),
                    ..Effect::default()
                }
            }
            "back" | "back_cancel" | "done" | "back_settings" => {
                self.draft = None;
                self.go(View::List)
            }
            "open_new" => {
                self.draft = Some(default_draft(settings, ctx));
                self.go(View::New)
            }
            "create" => {
                let Some(draft) = self.draft.take() else { return Effect::default() };
                let (effect, added) = add_draft(settings, ctx, &draft);
                if added.is_none() {
                    self.draft = Some(draft);
                    return effect;
                }
                self.view = View::List;
                effect
            }
            // --- List ---
            "prev_page" => {
                self.page = self.page.saturating_sub(1);
                Effect::show()
            }
            "next_page" => {
                self.page += 1;
                Effect::show()
            }
            "sort_next" => {
                settings.sort = Sort::from_index((settings.sort.index() + 1) % Sort::NAMES.len());
                self.page = 0;
                Effect::setting()
            }
            "filter_next" => {
                settings.filter = match settings.filter {
                    None => Some(0),
                    Some(n) if usize::from(n) + 1 < SETS.len() => Some(n + 1),
                    Some(_) => None,
                };
                self.page = 0;
                Effect::setting()
            }
            // --- New / Edit form ---
            "set_prev" | "set_next" => {
                let forward = id == "set_next";
                if new {
                    if let Some(d) = &mut self.draft {
                        d.set = cycle(usize::from(d.set), SETS.len(), forward) as u8;
                    }
                    return Effect::show();
                }
                self.edit(settings, ctx, |p| p.set = cycle(usize::from(p.set), SETS.len(), forward) as u8)
            }
            "toggle_hidden" => {
                if new {
                    if let Some(d) = &mut self.draft {
                        d.hidden = !d.hidden;
                    }
                    return Effect::show();
                }
                self.edit(settings, ctx, |p| p.hidden = !p.hidden)
            }
            "step_next" => {
                self.step = (self.step + 1) % STEPS.len();
                Effect::show()
            }
            "x_minus" | "x_plus" | "y_minus" | "y_plus" | "z_minus" | "z_plus" => {
                let axis = usize::from(id.as_bytes()[0] - b'x');
                let delta = if id.ends_with("plus") { step } else { -step };
                self.edit(settings, ctx, |p| {
                    p.pos[axis] = p.pos[axis].saturating_add(delta).clamp(-30_000_000, 30_000_000);
                    p.armed = false;
                })
            }
            "move_here" => {
                let (dim, feet) = (ctx.dim, ctx.feet);
                self.edit(settings, ctx, |p| {
                    p.dim = dim;
                    p.pos = feet;
                    p.armed = false;
                })
            }
            "copy_dim" => match self.selected {
                Some(selected) => copy_to_other_dimension(settings, ctx, selected),
                None => Effect::default(),
            },
            "keep" => self.edit(settings, ctx, |p| {
                if p.kind != Kind::Normal {
                    p.kind = Kind::Normal;
                    if p.name == "Death" {
                        p.name = format!("Saved {}", p.name);
                    }
                }
            }),
            "delete" => {
                let (Some(world), Some(selected)) = (&ctx.world, self.selected) else { return Effect::default() };
                if !self.confirm_delete {
                    self.confirm_delete = true;
                    return Effect::show();
                }
                self.confirm_delete = false;
                self.selected = None;
                self.view = View::List;
                let mut effect = Effect::redraw();
                if let Some(point) = settings.remove(world, selected) {
                    effect.message = Some(format!("Waypoints: {} deleted.", point.name));
                }
                effect
            }
            // --- Settings ---
            "t_show" | "t_beams" | "t_walls" | "t_death" | "t_dclear" | "t_names" => {
                let flag = match id {
                    "t_show" => &mut settings.show,
                    "t_beams" => &mut settings.beams,
                    "t_walls" => &mut settings.beams_through_walls,
                    "t_death" => &mut settings.death_point,
                    "t_dclear" => &mut settings.death_clear,
                    _ => &mut settings.show_names,
                };
                *flag = !*flag;
                Effect::setting()
            }
            "icon_minus" | "icon_plus" | "beam_minus" | "beam_plus" => {
                let up = id.ends_with("plus");
                let sign = if up { 1.0 } else { -1.0 };
                match id.split('_').next().unwrap_or("") {
                    "icon" => settings.icon_size += 0.25 * sign,
                    _ => settings.beam_width += 0.5 * sign,
                }
                settings.sanitize();
                Effect::setting()
            }
            _ => Effect::default(),
        }
    }

    /// While a key is being rebound, the next key pressed becomes it; Escape cancels.
    pub fn capture(&mut self, settings: &mut Settings, pressed: &[String]) -> Effect {
        let Some(bind) = self.capturing.filter(|_| !self.typing) else {
            return Effect::default();
        };
        for key in pressed {
            if key == "Escape" {
                self.capturing = None;
                if bind == Bind::Menu {
                    // Escape clears the menu key: F10 opens the menu again.
                    settings.menu_key.clear();
                    self.rebuild = true;
                    return Effect { save: true, republish: true, ..Effect::default() };
                }
                return Effect::show();
            }
            if key.starts_with("Mouse") {
                continue;
            }
            if valid_key(key) {
                *bind.slot(settings) = key.clone();
                settings.sanitize();
                self.capturing = None;
                // The menu key is the panel's own toggle key: only a full menu changes it.
                self.rebuild |= bind == Bind::Menu;
                return Effect { save: true, republish: true, ..Effect::default() };
            }
        }
        Effect::default()
    }
}

// ------------------------------------------------------------------------------------------
// Output

/// The whole menu: backing controls, the screen document and its values. Sent once (and when
/// the menu key changes); afterwards `patch` sends only the values.
pub fn full(settings: &Settings, state: &mut PanelState, ctx: &Context) -> String {
    state.rebuild = false;
    let mut out = String::with_capacity(DOCUMENT_ESCAPED.len() + 12_000);
    out.push_str("{\"toggle_key\":");
    // The menu opens from Mods Core or with the player's own key, which the mod handles.
    let _ = &settings.menu_key;
    q(&mut out, NO_KEY);
    out.push_str(",\"motion\":true,\"capture_key\":");
    out.push_str(if state.is_capturing() { "true" } else { "false" });
    out.push_str(",\"reference_size\":[");
    push_u(&mut out, u64::from(WIDTH + 20));
    out.push(',');
    push_u(&mut out, u64::from(HEIGHT + 20));
    out.push_str("],\"controls\":[");
    for (n, (kind, id)) in CONTROLS.iter().enumerate() {
        if n > 0 {
            out.push(',');
        }
        match kind {
            ControlKind::Button => {
                out.push_str("{\"kind\":\"button\",\"id\":\"");
                out.push_str(id);
                out.push_str("\",\"label\":\"");
                out.push_str(id);
                out.push_str("\"}");
            }
            ControlKind::Keybind => {
                let bind = Bind::ALL.into_iter().find(|b| b.id() == *id).unwrap_or(Bind::Menu);
                out.push_str("{\"kind\":\"keybind\",\"id\":\"");
                out.push_str(id);
                out.push_str("\",\"label\":\"");
                out.push_str(id);
                out.push_str("\",\"key\":");
                q(&mut out, bind.key(settings));
                out.push_str(if state.capturing == Some(bind) { ",\"capturing\":true}" } else { ",\"capturing\":false}" });
            }
        }
    }
    out.push_str("],\"surface\":{\"screen\":\"waypoints.menu\",\"document\":\"");
    out.push_str(DOCUMENT_ESCAPED);
    // The values follow in a patch next frame: the document alone is a frame's worth of fuel.
    out.push_str("\",\"bindings\":{}}}");
    let _ = ctx;
    out
}

/// Only the values: what `update_content` takes.
pub fn patch(settings: &Settings, state: &mut PanelState, ctx: &Context) -> String {
    let mut out = String::with_capacity(6000);
    out.push_str("{\"capture_key\":");
    out.push_str(if state.is_capturing() { "true" } else { "false" });
    out.push_str(",\"controls\":[");
    for (n, bind) in Bind::ALL.into_iter().enumerate() {
        if n > 0 {
            out.push(',');
        }
        out.push_str("{\"id\":\"");
        out.push_str(bind.id());
        out.push_str("\",\"key\":");
        q(&mut out, bind.key(settings));
        out.push_str(if state.capturing == Some(bind) { ",\"capturing\":true}" } else { ",\"capturing\":false}" });
    }
    out.push_str("],");
    bindings(&mut out, settings, state, ctx);
    out.push('}');
    out
}

/// Writes `,"key":"text"`.
fn b_text(out: &mut String, key: &str, value: &str) {
    out.push_str(",\"");
    out.push_str(key);
    out.push_str("\":\"");
    escape_into(out, value);
    out.push('"');
}

fn b_bool(out: &mut String, key: &str, value: bool) {
    out.push_str(",\"");
    out.push_str(key);
    out.push_str(if value { "\":true" } else { "\":false" });
}

/// Writes `,"key":<json>` for already-formed JSON (a colour or an offset).
fn b_raw(out: &mut String, key: &str, json: &str) {
    out.push_str(",\"");
    out.push_str(key);
    out.push_str("\":");
    out.push_str(json);
}

const ON: &str = "[0.29,0.62,1,1]";
const OFF: &str = "[0.26,0.29,0.35,1]";
const KNOB_ON: &str = "[12,2]";
const KNOB_OFF: &str = "[2,2]";
const BLACK: &str = "[0.05,0.05,0.07,1]";
const WHITE: &str = "[1,1,1,1]";

/// Dark text on light tiles, white on dark ones.
fn glyph_color(color: usize) -> &'static str {
    let [r, g, b, _] = COLORS[color % COLORS.len()];
    if 0.3 * r + 0.59 * g + 0.11 * b > 0.6 { BLACK } else { WHITE }
}

/// A switch: its track colour and knob position.
fn switch(out: &mut String, key: &str, on: bool) {
    let mut name = String::with_capacity(key.len() + 5);
    name.push_str(key);
    name.push_str("_col");
    b_raw(out, &name, if on { ON } else { OFF });
    name.truncate(key.len());
    name.push_str("_knob");
    b_raw(out, &name, if on { KNOB_ON } else { KNOB_OFF });
}

fn coords(out: &mut String, pos: [i32; 3]) {
    push_i(out, i64::from(pos[0]));
    out.push_str(", ");
    push_i(out, i64::from(pos[1]));
    out.push_str(", ");
    push_i(out, i64::from(pos[2]));
}

fn offset(out: &mut String, x: usize, y: usize) {
    out.push('[');
    push_u(out, x as u64);
    out.push(',');
    push_u(out, y as u64);
    out.push(']');
}

/// Every `#binding` the screen reads, always the same set.
fn bindings(out: &mut String, settings: &Settings, state: &mut PanelState, ctx: &Context) {
    let points: &[Waypoint] = ctx.world.as_deref().and_then(|w| settings.world(w)).map_or(&[], |w| &w.points);
    // An edited waypoint that is gone (deleted, reached, another world) sends you to the list.
    let editing = state.selected.and_then(|id| points.iter().find(|p| p.id == id));
    if state.view == View::Edit && editing.is_none() {
        state.view = View::List;
    }
    if state.view == View::New && state.draft.is_none() {
        state.view = View::List;
    }
    let editing = if state.view == View::Edit { editing } else { None };

    out.push_str("\"bindings\":{\"#dim\":\"");
    out.push_str(if ctx.world.is_some() { config::dimension_name(ctx.dim) } else { "No world" });
    out.push('"');
    b_bool(out, "#v_list", state.view == View::List);
    b_bool(out, "#v_form", matches!(state.view, View::New | View::Edit));
    b_bool(out, "#v_settings", state.view == View::Settings);
    b_bool(out, "#kofi_sent", ctx.clock < state.kofi_until);

    // --- List ---
    state.prepare(settings, ctx);
    let order: &[usize] = state.order.as_ref().map_or(&[], |(_, o)| o);
    let pages = order.len().div_ceil(ROWS).max(1);
    state.page = state.page.min(pages - 1);
    let mut text = String::with_capacity(64);
    text.push_str("Sort: ");
    text.push_str(Sort::NAMES[settings.sort.index()]);
    b_text(out, "#sort", &text);
    text.clear();
    text.push_str("Set: ");
    text.push_str(settings.filter.map_or("All", |s| SETS[usize::from(s) % SETS.len()]));
    b_text(out, "#filter", &text);
    let mut key = String::with_capacity(12);
    for row in 0..ROWS {
        let point = order.get(state.page * ROWS + row).and_then(|i| points.get(*i));
        state.slots[row] = point.map(|p| p.id);
        let mut field = |out: &mut String, suffix: &str| {
            key.clear();
            key.push_str("#r");
            push_u(&mut key, row as u64);
            key.push_str(suffix);
            out.push_str(",\"");
            out.push_str(&key);
            out.push_str("\":");
        };
        field(out, "_on");
        out.push_str(if point.is_some() { "true" } else { "false" });
        let Some(p) = point else {
            for suffix in ["_name", "_info", "_glyph"] {
                field(out, suffix);
                out.push_str("\"\"");
            }
            field(out, "_col");
            out.push_str(WHITE);
            field(out, "_gc");
            out.push_str(WHITE);
            continue;
        };
        field(out, "_name");
        out.push('"');
        escape_into(out, &p.name);
        if p.hidden {
            out.push_str("  (hidden)");
        }
        match p.kind {
            Kind::Death => out.push_str("  (death)"),
            Kind::Normal => {}
        }
        out.push('"');
        field(out, "_info");
        out.push('"');
        crate::hud::distance_text(out, distance(p.pos, ctx.feet));
        out.push_str("     ");
        coords(out, p.pos);
        out.push('"');
        field(out, "_glyph");
        out.push('"');
        escape_into(out, p.glyph());
        out.push('"');
        field(out, "_col");
        out.push_str(COLOR_JSON[p.color()]);
        field(out, "_gc");
        out.push_str(glyph_color(p.color()));
    }
    b_bool(out, "#empty", order.is_empty());
    text.clear();
    text.push_str("Press + New Waypoint, or ");
    text.push_str(settings.add_key.strip_prefix("Key").unwrap_or(&settings.add_key));
    text.push_str(" while playing");
    b_text(out, "#empty_hint", &text);
    text.clear();
    text.push_str("Page ");
    push_u(&mut text, (state.page + 1) as u64);
    text.push_str(" / ");
    push_u(&mut text, pages as u64);
    b_text(out, "#page", &text);

    // --- Form (new or edit) ---
    let draft = if state.view == View::New { state.draft.as_ref() } else { None };
    let (name, color, icon, set, hidden): (String, usize, u8, u8, bool) = match (editing, draft) {
        (Some(p), _) => (p.name.clone(), p.color(), p.icon, p.set, p.hidden),
        (None, Some(d)) => (d.name.clone(), usize::from(d.color) % COLORS.len(), d.icon, d.set, d.hidden),
        _ => (String::new(), 15, 0, 0, false),
    };
    if !matches!(state.view, View::New | View::Edit) {
        state.typing = false;
    }
    let preview = Waypoint::new(&name, 0, [0; 3], color as u8, icon, Kind::Normal);
    b_text(
        out,
        "#form_title",
        match editing.map(|p| p.kind) {
            Some(Kind::Death) => "Edit Death Point",
            Some(Kind::Normal) => "Edit Waypoint",
            None => "New Waypoint",
        },
    );
    b_bool(out, "#f_new", draft.is_some());
    b_bool(out, "#f_edit", editing.is_some());
    b_raw(out, "#f_col", COLOR_JSON[color]);
    b_raw(out, "#f_gc", glyph_color(color));
    b_text(out, "#f_glyph", preview.glyph());
    text.clear();
    text.push_str(&name);
    if state.typing {
        text.push('_');
    }
    b_text(out, "#f_name", &text);
    b_text(
        out,
        "#f_name_tip",
        if state.typing { "Type the name, Enter to finish" } else { "Click the name to type a new one" },
    );
    b_raw(out, "#f_name_box", if state.typing { "[0.29,0.62,1,1]" } else { "[0.135,0.15,0.185,1]" });
    text.clear();
    match editing {
        Some(p) => {
            coords(&mut text, p.pos);
            text.push_str("   ");
            text.push_str(config::dimension_name(p.dim));
            text.push_str(",  ");
            crate::hud::distance_text(&mut text, distance(p.pos, ctx.feet));
            text.push_str(" away");
        }
        None => {
            text.push_str("Where you stand: ");
            coords(&mut text, ctx.feet);
        }
    }
    b_text(out, "#f_pos", &text);
    b_text(out, "#f_set", SETS[usize::from(set) % SETS.len()]);
    switch(out, "#f_hidden", hidden);
    text.clear();
    text.push_str("Step ");
    push_u(&mut text, STEPS[state.step % STEPS.len()] as u64);
    b_text(out, "#f_step", &text);
    text.clear();
    text.push_str("Color: ");
    text.push_str(COLOR_NAMES[color]);
    b_text(out, "#f_colname", &text);
    text.clear();
    offset(&mut text, 219 + (color % 8) * 24, 83 + (color / 8) * 18);
    b_raw(out, "#f_color_ring", &text);
    text.clear();
    text.push_str("Icon: ");
    text.push_str(preview.icon_name());
    b_text(out, "#f_iconname", &text);
    let icon = usize::from(icon) % ICON_COUNT;
    text.clear();
    offset(&mut text, 219 + (icon % 7) * 26, 137 + (icon / 7) * 20);
    b_raw(out, "#f_icon_ring", &text);
    text.clear();
    match editing.map(|p| (p.dim, p.pos)) {
        Some((0, pos)) => {
            text.push_str("To Nether ");
            push_i(&mut text, i64::from(pos[0].div_euclid(8)));
            text.push(' ');
            push_i(&mut text, i64::from(pos[2].div_euclid(8)));
        }
        Some((1, pos)) => {
            text.push_str("To Overworld ");
            push_i(&mut text, i64::from(pos[0] * 8));
            text.push(' ');
            push_i(&mut text, i64::from(pos[2] * 8));
        }
        _ => text.push_str("Copy: OW / Nether"),
    }
    b_text(out, "#f_copy", &text);
    b_bool(out, "#f_keep", editing.is_some_and(|p| p.kind != Kind::Normal));
    b_text(out, "#f_delete", if state.confirm_delete { "Sure? Click" } else { "Delete" });

    // --- Settings ---
    let key = SettingsKey::of(settings);
    if state.settings_block.as_ref().is_none_or(|(shown, _)| *shown != key) {
        state.settings_block = Some((key, settings_bindings(settings)));
    }
    if let Some((_, text)) = &state.settings_block {
        out.push_str(text);
    }
    out.push('}');
}

/// The Settings screen's values.
fn settings_bindings(settings: &Settings) -> String {
    let out = &mut String::with_capacity(900);
    let mut text = String::with_capacity(16);
    switch(out, "#s_show", settings.show);
    switch(out, "#s_names", settings.show_names);
    text.clear();
    num(&mut text, settings.icon_size);
    text.push('x');
    b_text(out, "#s_icon", &text);
    switch(out, "#s_beams", settings.beams);
    switch(out, "#s_walls", settings.beams_through_walls);
    text.clear();
    num(&mut text, settings.beam_width / 2.0);
    text.push('x');
    b_text(out, "#s_beam", &text);
    switch(out, "#s_death", settings.death_point);
    switch(out, "#s_dclear", settings.death_clear);
    std::mem::take(out)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::Value;

    static REVISION: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(1);

    fn ctx() -> Context {
        Context {
            world: Some("w".into()),
            dim: 0,
            feet: [10, 64, 10],
            clock: 100.0,
            revision: REVISION.fetch_add(1, std::sync::atomic::Ordering::Relaxed),
        }
    }

    fn values(s: &Settings, p: &mut PanelState, c: &Context) -> Value {
        let v: Value = serde_json::from_str(&patch(s, p, c)).unwrap();
        v["bindings"].clone()
    }

    /// The whole menu with its values filled in, as the client shows it after the first patch.
    fn preview(s: &Settings, p: &mut PanelState, c: &Context) -> String {
        let mut doc: Value = serde_json::from_str(&full(s, p, c)).unwrap();
        doc["surface"]["bindings"] = values(s, p, c);
        doc.to_string()
    }

    fn keys(v: &Value) -> Vec<String> {
        v.as_object().unwrap().keys().cloned().collect()
    }

    #[test]
    fn the_same_bindings_every_time_and_within_the_limit() {
        let mut s = Settings::default();
        let mut p = PanelState::default();
        let empty = values(&s, &mut p, &Context::default());
        assert!(keys(&empty).len() <= 128, "{}", keys(&empty).len());
        for n in 0..10 {
            s.add("w", Waypoint::new(&format!("Waypoint {n}"), 0, [n * 10, 64, 0], (n % 16) as u8, 0, Kind::Normal))
                .unwrap();
        }
        let listed = values(&s, &mut p, &ctx());
        assert_eq!(keys(&listed), keys(&empty));
        assert_eq!(listed["#page"], "Page 1 / 2");
        assert_eq!(listed["#r0_on"], true);
        p.event(&mut s, &ctx(), "open_new", 1.0);
        assert_eq!(keys(&values(&s, &mut p, &ctx())), keys(&empty));
        p.event(&mut s, &ctx(), "open_settings", 1.0);
        assert_eq!(keys(&values(&s, &mut p, &ctx())), keys(&empty));
        let doc: Value = serde_json::from_str(&full(&s, &mut p, &ctx())).unwrap();
        assert_eq!(doc["controls"].as_array().unwrap().len(), CONTROLS.len());
        let document: Value = serde_json::from_str(doc["surface"]["document"].as_str().unwrap()).unwrap();
        assert_eq!(document["namespace"], "waypoints");
        if std::env::var_os("WP_DUMP").is_some() {
            for (view, file) in [(View::List, "list"), (View::New, "new"), (View::Settings, "settings")] {
                p.view = view;
                if view == View::New {
                    p.event(&mut s, &ctx(), "open_new", 1.0);
                }
                std::fs::write(format!("/home/claude/wp-menu-{file}.json"), preview(&s, &mut p, &ctx())).unwrap();
            }
            p.event(&mut s, &ctx(), "back", 1.0);
            values(&s, &mut p, &ctx());
            p.event(&mut s, &ctx(), "row_1", 1.0);
            std::fs::write("/home/claude/wp-menu-edit.json", preview(&s, &mut p, &ctx())).unwrap();
        }
    }

    #[test]
    fn create_is_separate_from_edit() {
        let mut s = Settings::default();
        let mut p = PanelState::default();
        p.event(&mut s, &ctx(), "open_new", 1.0);
        assert_eq!(p.view, View::New);
        // Type a name: click the box, type, Enter.
        p.event(&mut s, &ctx(), "name_box", 1.0);
        assert!(p.typing && p.is_capturing());
        for _ in 0..10 {
            p.type_keys(&mut s, &ctx(), &["Backspace".into()], false);
        }
        let keys: Vec<String> = ["KeyC", "KeyA", "KeyS", "KeyA", "Space", "KeyD", "KeyE", "Space", "KeyE", "KeyM", "KeyI", "KeyL", "Semicolon", "Digit2"]
            .iter()
            .map(|k| k.to_string())
            .collect();
        p.type_keys(&mut s, &ctx(), &keys[..1], true);
        p.type_keys(&mut s, &ctx(), &keys[1..], false);
        let v = values(&s, &mut p, &ctx());
        assert_eq!(v["#f_name"], "Casa de emilñ2_");
        p.type_keys(&mut s, &ctx(), &["Enter".into()], false);
        assert!(!p.typing);
        p.event(&mut s, &ctx(), "color_6", 1.0);
        p.event(&mut s, &ctx(), "icon_3", 1.0);
        p.event(&mut s, &ctx(), "set_next", 1.0);
        let v = values(&s, &mut p, &ctx());
        assert_eq!((v["#form_title"].as_str(), v["#f_name"].as_str(), v["#f_new"].as_bool()), (Some("New Waypoint"), Some("Casa de emilñ2"), Some(true)));
        assert_eq!(v["#f_colname"], "Color: Gold");
        assert_eq!(v["#f_color_ring"], serde_json::json!([219 + 6 * 24, 83]));
        assert!(s.world("w").is_none(), "nothing is added before Create");
        let e = p.event(&mut s, &ctx(), "create", 1.0);
        assert!(e.message.unwrap().contains("Casa de emilñ2 added at 10 64 10"));
        assert_eq!(p.view, View::List);
        let w = &s.world("w").unwrap().points[0];
        assert_eq!((w.color, w.icon, w.set), (6, 3, 1));
        // Edit from the list, renaming by typing.
        values(&s, &mut p, &ctx());
        p.event(&mut s, &ctx(), "row_0", 1.0);
        assert_eq!(p.view, View::Edit);
        p.event(&mut s, &ctx(), "name_box", 1.0);
        for _ in 0..30 {
            p.type_keys(&mut s, &ctx(), &["Backspace".into()], false);
        }
        p.type_keys(&mut s, &ctx(), &["KeyX".into()], true);
        // Clicking anything else ends typing.
        p.event(&mut s, &ctx(), "x_plus", 1.0);
        assert!(!p.typing);
        assert_eq!(s.world("w").unwrap().points[0].name, "X");
        p.event(&mut s, &ctx(), "step_next", 1.0);
        p.event(&mut s, &ctx(), "y_minus", 1.0);
        assert_eq!(s.world("w").unwrap().points[0].pos, [11, 54, 10]);
        // An emptied name comes back as "Waypoint N".
        p.event(&mut s, &ctx(), "name_box", 1.0);
        p.type_keys(&mut s, &ctx(), &["Backspace".into(), "Escape".into()], false);
        assert_eq!(s.world("w").unwrap().points[0].name, "Waypoint 1");
        p.event(&mut s, &ctx(), "delete", 1.0);
        assert_eq!(values(&s, &mut p, &ctx())["#f_delete"], "Sure? Click");
        p.event(&mut s, &ctx(), "delete", 1.0);
        assert!(s.world("w").unwrap().points.is_empty());
        assert_eq!(p.view, View::List);
    }

    #[test]
    fn settings_screen_changes_settings() {
        let mut s = Settings::default();
        let mut p = PanelState::default();
        p.event(&mut s, &ctx(), "open_settings", 1.0);
        assert!(p.event(&mut s, &ctx(), "t_beams", 1.0).save);
        assert!(!s.beams);
        p.event(&mut s, &ctx(), "t_names", 1.0);
        p.event(&mut s, &ctx(), "icon_plus", 1.0);
        assert!(!s.show_names && s.icon_size == 1.75);
        let v = values(&s, &mut p, &ctx());
        assert_eq!(v["#s_beams_col"], serde_json::json!([0.26, 0.29, 0.35, 1]));
        p.event(&mut s, &ctx(), "menu_key", 1.0);
        p.capture(&mut s, &["KeyM".into()]);
        assert!(p.rebuild && s.menu_key == "KeyM");
    }

    #[test]
    fn hotkeys_add_without_the_menu() {
        let mut s = Settings::default();
        let (e, id) = add_here(&mut s, &ctx());
        assert!(id.is_some() && e.save && e.message.unwrap().contains("Waypoint 1"));
    }
}
