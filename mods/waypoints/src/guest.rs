//! Host glue: reads where you are, runs the menu and the hotkey, and draws the waypoints.
//!
//! Everything in the world is drawn with world drawing, not HUD labels: the client keeps the
//! floating labels of only one mod, so with other mods installed they could vanish. Each
//! waypoint gets a beacon (beam, gem, floor mark) at its block and a badge (box with the icon,
//! name and distance) that always faces you, drawn through walls and pulled in to a fixed
//! distance along your line of sight, so it shows from any distance in its dimension.

use crate::config::{COLORS, Kind, Loader, Settings, Waypoint};
use crate::label;
use crate::panel::{self, Context, Effect, PanelState};
use crate::shapes;
use mod_api::bindings::{
    Guest,
    cinnabar::extension::{hud as host_hud, input, panel as host_panel, player_state, settings, touch},
    cinnabar::world_draw::drawing as draw,
};
use std::cell::RefCell;

struct Mod;

/// Beam height above the waypoint, within the 64-block reach of a drawing.
const BEAM_HEIGHT: f32 = 60.0;
/// Where the gem floats over the block.
const GEM_HEIGHT: f32 = 3.4;
/// Where the badge sits over the block.
const BADGE_HEIGHT: f64 = 5.2;
/// The key Cinnabar also opens every mod menu with.
/// Beacons (beam, gem, floor mark) farther than this many blocks are not drawn.
const BEACON_RANGE: f64 = 75.0;
/// Waypoints drawn at once, nearest first.
const MAX_MARKS: usize = 12;
/// Badges farther than this are drawn this far away along the same line of sight: same place
/// on screen, but inside the drawing range.
const LABEL_RANGE: f64 = 32.0;
/// Badge pixels per block of distance (at label size 1.5): keeps it the same size on screen.
const LABEL_SCALE: f64 = 0.0022;
/// Drawing calls per frame; the host allows 32 per callback.
const DRAW_CALLS: usize = 24;
const DRAW_CALLS_BUSY: usize = 6;
/// Calls per frame for removing meshes no longer used (24 + 4 stays under the host's 32).
const RETIRE_CALLS: usize = 4;
/// Blocks you must be away from the death point before reaching it removes it.
const ARM_DISTANCE: i32 = 8;
const REACH_DISTANCE: i32 = 3;
const EYE_HEIGHT: f64 = 1.62;

/// Keys withheld from other mods while the menu is open (the host allows 64 in all).
const TYPING_KEYS: [&str; 57] = [
    "KeyA", "KeyB", "KeyC", "KeyD", "KeyE", "KeyF", "KeyG", "KeyH", "KeyI", "KeyJ", "KeyK", "KeyL", "KeyM",
    "KeyN", "KeyO", "KeyP", "KeyQ", "KeyR", "KeyS", "KeyT", "KeyU", "KeyV", "KeyW", "KeyX", "KeyY", "KeyZ",
    "Digit0", "Digit1", "Digit2", "Digit3", "Digit4", "Digit5", "Digit6", "Digit7", "Digit8", "Digit9",
    "Numpad0", "Numpad1", "Numpad2", "Numpad3", "Numpad4", "Numpad5", "Numpad6", "Numpad7", "Numpad8", "Numpad9",
    "Space", "Minus", "Period", "Comma", "Semicolon", "Slash", "Quote", "Backspace", "Enter", "NumpadEnter", "Tab",
];

struct Shapes {
    beam: draw::Handle,
    core: draw::Handle,
    gem: draw::Handle,
    floor: draw::Handle,
}

/// A geometry handle as plain numbers, so drawings can be compared and copied.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct Key(u64, u64, u64);

impl Key {
    fn of(handle: &draw::Handle) -> Key {
        Key(handle.owner, handle.generation, handle.slot)
    }
    fn handle(self) -> draw::Handle {
        draw::Handle { owner: self.0, generation: self.1, slot: self.2 }
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
enum Geo {
    /// The wide, soft part of the beacon beam (glows: additive).
    Beam,
    /// Its bright core.
    Core,
    Gem,
    Floor,
    /// A badge or distance mesh made for one waypoint.
    Mesh(Key),
}

/// One drawing as the mod wants it; compared to what is on screen to skip unchanged ones.
#[derive(Clone, Copy, Debug, PartialEq)]
struct Want {
    geo: Geo,
    dimension: i32,
    origin: [f64; 3],
    tint: [f32; 4],
    rotation: [f32; 4],
    scale: [f32; 3],
    translation: [f32; 3],
    always: bool,
}

impl Want {
    fn at(geo: Geo, dimension: i32, origin: [f64; 3], tint: [f32; 4], always: bool) -> Want {
        Want {
            geo,
            dimension,
            origin,
            tint,
            rotation: [0.0, 0.0, 0.0, 1.0],
            scale: [1.0; 3],
            translation: [0.0; 3],
            always,
        }
    }

    fn build(&self, shapes: &Shapes) -> draw::Instance {
        let geometry = match self.geo {
            Geo::Beam => shapes.beam,
            Geo::Core => shapes.core,
            Geo::Gem => shapes.gem,
            Geo::Floor => shapes.floor,
            Geo::Mesh(key) => key.handle(),
        };
        let glow = matches!(self.geo, Geo::Beam | Geo::Core);
        let [r, g, b, a] = self.tint;
        let [qx, qy, qz, qw] = self.rotation;
        draw::Instance {
            geometry,
            material: None,
            params: Vec::new(),
            anchor: draw::Anchor::World(draw::WorldAnchor {
                dimension: self.dimension,
                origin: (self.origin[0], self.origin[1], self.origin[2]),
            }),
            transform: draw::Transform {
                translation: (self.translation[0], self.translation[1], self.translation[2]),
                rotation: (qx, qy, qz, qw),
                scale: (self.scale[0], self.scale[1], self.scale[2]),
            },
            // Premultiplied blending: colour channels already carry the alpha.
            tint: (r * a, g * a, b * a, a),
            depth: if self.always { draw::Depth::Always } else { draw::Depth::Test },
            blend: if glow { draw::Blend::Additive } else { draw::Blend::Premultiplied },
            width: 1.0,
            width_mode: draw::WidthMode::Pixels,
            join: draw::Join::Bevel,
            cap: draw::Cap::Butt,
            clip: None,
        }
    }
}

/// Retained drawings brought to the wanted list a few host calls at a time.
#[derive(Default)]
struct Pool {
    handles: Vec<draw::Handle>,
    shown: Vec<Want>,
    wanted: Vec<Want>,
    cursor: usize,
    dirty: bool,
}

impl Pool {
    fn want(&mut self, wanted: Vec<Want>) {
        if wanted != self.wanted {
            self.wanted = wanted;
            self.cursor = 0;
            self.dirty = true;
        }
    }

    /// Whether a drawing on screen, or about to be, still uses this mesh.
    fn uses(&self, key: Key) -> bool {
        self.shown.iter().chain(&self.wanted).any(|w| w.geo == Geo::Mesh(key))
    }

    /// Returns whether drawing through walls was refused.
    fn sync(&mut self, shapes: &Shapes, calls: &mut usize, error: &mut Option<String>) -> bool {
        let mut denied = false;
        while self.dirty && *calls > 0 {
            let index = self.cursor;
            if index < self.wanted.len() {
                let want = self.wanted[index];
                if index < self.handles.len() {
                    if self.shown[index] != want {
                        *calls -= 1;
                        match draw::update(self.handles[index], &want.build(shapes)) {
                            Ok(()) => self.shown[index] = want,
                            Err(failure) => {
                                denied |= want.always && failure.code == draw::ErrorCode::Denied;
                                *error = Some(failure.detail);
                            }
                        }
                    }
                } else {
                    *calls -= 1;
                    match draw::create(&want.build(shapes)) {
                        Ok(handle) => {
                            self.handles.push(handle);
                            self.shown.push(want);
                        }
                        Err(failure) => {
                            denied |= want.always && failure.code == draw::ErrorCode::Denied;
                            *error = Some(failure.detail);
                            self.wanted.truncate(index);
                            continue;
                        }
                    }
                }
                self.cursor += 1;
            } else if self.handles.len() > self.wanted.len() {
                if let Some(handle) = self.handles.pop() {
                    self.shown.pop();
                    *calls -= 1;
                    let _ = draw::remove(handle);
                }
            } else {
                self.dirty = false;
            }
        }
        denied
    }
}

/// The meshes made for one waypoint's badge.
struct Label {
    id: u32,
    /// What the box shows: its letters, icon and colour.
    badge_for: Option<(String, u8, u8)>,
    badge: Option<Key>,
    /// The name the name meshes show, one mesh per chunk (built a chunk a frame).
    name_for: Option<String>,
    name: Vec<Key>,
    distance_for: Option<(String, u8, bool)>,
    distance: Option<Key>,
}

#[derive(Default)]
struct State {
    settings: Settings,
    panel: PanelState,
    clock: f64,
    frames: u32,
    /// Where you are: world, session, dimension, feet block and exact position.
    world: Option<String>,
    session: u64,
    dimension: i32,
    feet: [i32; 3],
    position: [f64; 3],
    alive: bool,
    /// Bumped whenever waypoints or settings change, so drawings and the menu are rebuilt.
    revision: u64,
    shapes: Option<Shapes>,
    shapes_failed: bool,
    pool: Pool,
    draw_error: Option<String>,
    /// Drawing through walls was refused: badges are drawn normally instead.
    no_through_walls: bool,
    /// The settings file, still being read.
    loader: Option<Loader>,
    /// Heavy work waiting for a frame of its own: a frame has 100k fuel.
    panel_pending: bool,
    save_pending: bool,
    /// The text of a save in progress.
    save_text: Option<String>,
    panel_open: bool,
    panel_feet: [i32; 3],
    /// Something this frame (a death, a removed waypoint) already used part of its fuel.
    busy: bool,
    /// The nearest waypoints of this dimension, as indices into the world's list; found again
    /// every few blocks, since that means sorting them all.
    near: Vec<usize>,
    near_key: Option<(u64, i32, [i32; 3], bool)>,
    labels: Vec<Label>,
    /// Meshes replaced by newer ones, removed once no drawing uses them.
    retired: Vec<Key>,
    /// Where the drawings were last placed from, and what for.
    placed: Option<([f64; 3], u64, u32)>,
    meshes_made: u32,
    /// Last frame did heavy work: this one places the drawings first.
    rest: bool,
    save_error: bool,
    /// The menu was sent once, so its key opens it; later it is only rebuilt while open.
    panel_sent: bool,
    /// The whole menu (document and controls) was sent; later sends are value patches.
    panel_full_sent: bool,
    /// What the reserved keys were made for: menu open, and the hotkey.
    reserved: Option<(bool, String)>,
    /// The on-screen "+ WP" button for touch controls is set.
    touch_set: bool,

}

thread_local! {
    /// This mod's place in Mods Core's list.
    static CORE: RefCell<crate::mods_core::Link> = RefCell::new(crate::mods_core::Link::new(
        "waypoints",
        "Waypoints",
        env!("CARGO_PKG_VERSION"),
        "Waypoints with beacons, icons and distance, seen through blocks in every dimension.",
    ));

    static STATE: RefCell<State> = RefCell::new(State::default());
}

impl Guest for Mod {
    fn init() {
        STATE.with_borrow_mut(|state| {
            // Reading a big file costs more fuel than one callback has: it is read over frames.
            if let Ok(text) = settings::load() {
                state.loader = Some(Loader::new(text));
            }
            state.alive = true;
        });
    }

    fn frame() {
        STATE.with_borrow_mut(State::frame);
    }
}

/// The touch button that adds a waypoint on phones and tablets.
const TOUCH_ADD: &str = "waypoints-add";

/// Badge meshes are made in font pixels; uploaded at this size so even a long name stays inside
/// the 64-unit bound of a geometry, and drawn this much larger.
const BADGE_UNIT: f32 = 0.25;

/// A badge part's vertex for the host.
fn vertex(p: [f32; 3], c: [f32; 4]) -> draw::Vertex {
    draw::Vertex { position: (p[0], p[1], p[2]), color: (c[0], c[1], c[2], c[3]), uv: (0.0, 0.0), data: (0.0, 0.0, 0.0, 0.0) }
}

fn badge_mesh<F: Fn([f32; 3], [f32; 4]) -> draw::Vertex>(built: label::Builder<draw::Vertex, F>) -> Result<draw::Handle, draw::DrawError> {
    draw::create_geometry(&draw::GeometrySpec::Triangles(draw::Mesh { vertices: built.vertices, indices: built.indices }))
}

fn mesh(mesh: shapes::Mesh) -> Result<draw::Handle, draw::DrawError> {
    draw::create_geometry(&draw::GeometrySpec::Triangles(draw::Mesh {
        vertices: mesh
            .vertices
            .into_iter()
            .map(|(p, c)| draw::Vertex {
                position: (p[0], p[1], p[2]),
                color: (c[0], c[1], c[2], c[3]),
                uv: (0.0, 0.0),
                data: (0.0, 0.0, 0.0, 0.0),
            })
            .collect(),
        indices: mesh.indices,
    }))
}

fn distance_sq(a: [i32; 3], b: [i32; 3]) -> i64 {
    (0..3).map(|i| i64::from(a[i] - b[i]).pow(2)).sum()
}

impl State {
    fn ctx(&self) -> Context {
        Context {
            world: self.world.clone(),
            dim: self.dimension,
            feet: self.feet,
            clock: self.clock,
            revision: self.revision,
        }
    }

    fn frame(&mut self) {
        self.frames = self.frames.wrapping_add(1);
        if let Some(loader) = &mut self.loader {
            if loader.step(&mut self.settings) {
                self.loader = None;
                self.settings.sanitize();
                self.revision += 1;
                // The menu goes out next frame: this one read part of the file.
                self.panel_pending = true;
            }
            return;
        }
        self.busy = false;
        self.read_world();
        // A frame that handled menu clicks or keys leaves the rest for the next one.
        self.busy |= self.read_controls();
        // After reading: whether the menu is open decides which keys are claimed.
        self.reserve_keys();
        if self.world.is_none() {
            if self.panel_pending && (self.panel_open || !self.panel_sent) {
                self.publish_panel();
            } else if self.save_pending {
                self.save();
            }
            return;
        }
        self.reached();
        if self.shapes.is_none() && !self.shapes_failed {
            self.upload_shapes();
            return;
        }
        // The "look at the game chat" note goes away after a few seconds.
        if self.panel.kofi_until > 0.0 && self.clock >= self.panel.kofi_until {
            self.panel.kofi_until = 0.0;
            self.panel_pending = true;
        }
        // The open menu's distances follow you every few blocks.
        if self.panel_open && distance_sq(self.feet, self.panel_feet) >= 16 {
            self.panel_pending = true;
        }
        // A closed menu is not rebuilt: nobody sees it until it opens.
        // Values go only to this mod's own open menu (`panel_open` is about whichever menu is
        // shown).
        let panel_due = self.panel_pending && self.panel_open && CORE.with_borrow(|link| link.has_panel());
        // At most one heavy step per frame, and a frame after one places the drawings first:
        // both together would not fit in a frame's fuel.
        let rest = std::mem::take(&mut self.rest);
        let heavy = if self.busy {
            true
        } else if rest {
            false
        } else if panel_due {
            // Sorting the list, when needed, takes a frame of its own; the values go next.
            let ctx = self.ctx();
            if !(self.panel_full_sent && self.panel.prepare(&self.settings, &ctx)) {
                self.publish_panel();
            }
            true
        } else if self.save_pending {
            self.save();
            true
        } else if self.refresh_near() {
            true
        } else {
            self.make_meshes()
        };
        if heavy {
            self.rest = true;
        } else {
            self.place();
        }
        if let Some(shapes) = &self.shapes {
            let mut calls = if heavy { DRAW_CALLS_BUSY } else { DRAW_CALLS };
            let mut error = None;
            if self.pool.sync(shapes, &mut calls, &mut error) && !self.no_through_walls {
                self.no_through_walls = true;
                self.placed = None;
            }
            if error.is_some() {
                self.draw_error = error;
            }
            // Meshes no drawing uses any more are released, a few per frame.
            // Each check looks through every drawing, so only a few are checked a frame.
            // They get their own calls: walking moves every badge and can use all of `calls`.
            let (mut index, mut checks, mut removed) = (0, 0, 0);
            while index < self.retired.len() && checks < 8 && removed < RETIRE_CALLS {
                let key = self.retired[index];
                checks += 1;
                if self.pool.uses(key) {
                    index += 1;
                } else {
                    let _ = draw::remove(key.handle());
                    self.retired.swap_remove(index);
                    removed += 1;
                }
            }
            // The ones still in use wait at the back.
            if index > 0 && index < self.retired.len() {
                self.retired.rotate_left(index);
            }
        }
    }

    /// Keeps the new-waypoint key away from the game while it is bound.
    /// Claims the hotkey; while the menu is open, also every key used to type a name, so
    /// other mods (loaded after this one) do not act on them while you write.
    fn reserve_keys(&mut self) {
        let want = (self.panel_open, self.settings.add_key.clone());
        if self.reserved.as_ref() == Some(&want) {
            return;
        }
        let mut keys: Vec<String> = Vec::with_capacity(TYPING_KEYS.len() + 1);
        keys.push(want.1.clone());
        if want.0 {
            keys.extend(TYPING_KEYS.iter().map(|k| (*k).to_string()));
        }
        if input::reserve_keys(&keys).is_ok() {
            self.reserved = Some(want);
        }
    }

    /// Saving takes two frames: the text is written in one and handed to the host in the next.
    fn save(&mut self) {
        let Some(text) = self.save_text.take() else {
            self.save_text = Some(self.settings.save_text());
            return;
        };
        self.save_pending = false;
        if let Err(error) = settings::save(&text) {
            if !self.save_error {
                self.save_error = true;
                let clean: String = error.chars().take(100).collect();
                let _ = host_hud::local_message(&format!("Waypoints: could not save ({clean})."));
            }
        }
    }

    fn publish_panel(&mut self) {
        self.panel_pending = false;
        self.panel_sent = true;
        self.panel_feet = self.feet;
        let ctx = self.ctx();
        // Only values: the whole menu is set when it opens (Mods Core) and dropped when it
        // closes.
        self.panel_full_sent = true;
        self.panel.rebuild = false;
        let result = host_panel::update_content(&panel::patch(&self.settings, &mut self.panel, &ctx));
        if let Err(error) = result {
            let clean: String = error.chars().take(100).collect();
            let _ = host_hud::local_message(&format!("Waypoints menu: {clean}"));
        }
    }

    fn read_world(&mut self) {
        let Ok(Some(world)) = player_state::read_world() else {
            if self.world.take().is_some() {
                self.revision += 1;
            }
            return;
        };
        let (x, y, z) = world.position;
        if !(x.is_finite() && y.is_finite() && z.is_finite()) {
            return;
        }
        let id = if world.world.is_empty() { "default".to_string() } else { world.world.clone() };
        if self.world.as_deref() != Some(&id) || self.session != world.session || self.dimension != world.dimension {
            self.revision += 1;
            self.alive = world.alive;
            self.panel_pending = true;
        }
        self.world = Some(id);
        self.session = world.session;
        self.dimension = world.dimension;
        self.position = [f64::from(x), f64::from(y), f64::from(z)];
        self.feet = [x.floor() as i32, y.floor() as i32, z.floor() as i32];
        // Dying leaves a death point where you fell.
        if self.alive && !world.alive && self.settings.death_point {
            let world_id = self.world.clone().unwrap_or_default();
            match self.settings.set_death(&world_id, self.dimension, self.feet) {
                Ok(_) => {
                    let [x, y, z] = self.feet;
                    let _ = host_hud::local_message(&format!(
                        "Waypoints: death point saved at {x} {y} {z} ({}).",
                        crate::config::dimension_name(self.dimension)
                    ));
                    self.apply(Effect { save: true, republish: true, changed: true, message: None });
                    self.busy = true;
                }
                Err(why) => {
                    let _ = host_hud::local_message(&format!("Waypoints: death point not saved, {why}."));
                }
            }
        }
        self.alive = world.alive;
    }

    /// Handles menu events, typing and the hotkey; true when there were any.
    fn read_controls(&mut self) -> bool {
        let capturing = self.panel.is_capturing();
        let typing = self.panel.typing;
        let Ok(controls) = input::read_selected_controls(&input::Selection {
            // While typing or rebinding every key counts; otherwise only the hotkey.
            keys_pressed: if capturing {
                None
            } else if crate::panel::has_own_key(&self.settings.menu_key) {
                Some(vec![self.settings.add_key.clone(), self.settings.menu_key.clone()])
            } else {
                Some(vec![self.settings.add_key.clone()])
            },
            keys_held: Some(if typing { vec!["ShiftLeft".into(), "ShiftRight".into()] } else { Vec::new() }),
            events: true,
            keys_admitted: Some(Vec::new()),
        }) else {
            return false;
        };
        // Touch controls: a "+ WP" button that adds a waypoint here, as the hotkey does.
        if !self.touch_set {
            let button = touch::Button {
                id: TOUCH_ADD.into(),
                label: "+ WP".into(),
                anchor: touch::Anchor::Right,
                offset: touch::Point { x: 6.0, y: -8.0 },
                size: touch::Point { x: 40.0, y: 22.0 },
            };
            self.touch_set = touch::set_buttons(&[button]).is_ok();
        }
        let tapped = touch::read_presses().is_ok_and(|taps| taps.iter().any(|id| id == TOUCH_ADD));
        // Mods Core: a click on this mod in its list sets and opens the menu; the player's own
        // key does the same through Mods Core's hand-over.
        if !capturing && crate::panel::has_own_key(&self.settings.menu_key) && controls.focused
            && controls.keys_pressed.iter().any(|key| *key == self.settings.menu_key)
        {
            core_toggle(controls.panel_open);
        }
        let ctx = self.ctx();
        let whole = || panel::full(&self.settings, &mut self.panel, &ctx);
        if core_frame(controls.panel_open, whole) == crate::mods_core::Event::Opened {
            // The values follow in a patch.
            self.panel_pending = true;
        }
        // `panel_open` is about whichever menu is shown: this one only while it has its panel.
        let own_open = controls.panel_open && CORE.with_borrow(|link| link.has_panel());
        let opened = own_open && !self.panel_open;
        if opened {
            // Just opened: bring it up to date.
            self.panel_pending = true;
        }
        if !own_open && self.panel.typing {
            // Closing the menu ends typing.
            let ctx = self.ctx();
            let effect = self.panel.type_keys(&mut self.settings, &ctx, &["Enter".into()], false);
            self.apply(effect);
        }
        self.panel_open = own_open;
        let seconds = f64::from(controls.seconds);
        self.clock += if seconds.is_finite() { seconds.clamp(0.0, 1.0) } else { 0.0 };
        let busy = !controls.events.is_empty() || !controls.keys_pressed.is_empty() || tapped;
        if !busy {
            return false;
        }
        let ctx = self.ctx();
        let mut effect = Effect::default();
        for event in &controls.events {
            effect = effect.merge(self.panel.event(&mut self.settings, &ctx, &event.id, event.value));
        }
        if typing && self.panel.typing {
            let shift = !controls.keys_held.is_empty();
            effect = effect.merge(self.panel.type_keys(&mut self.settings, &ctx, &controls.keys_pressed, shift));
        } else if capturing && !typing {
            effect = effect.merge(self.panel.capture(&mut self.settings, &controls.keys_pressed));
        } else if !capturing && controls.focused && !controls.panel_open && self.world.is_some() {
            // The client only marks "gameplay" for mods with terrain or player access, so the
            // hotkey goes by focus and an open world instead.
            if tapped || controls.keys_pressed.iter().any(|key| *key == self.settings.add_key) {
                let (added, id) = panel::add_here(&mut self.settings, &ctx);
                if id.is_some() {
                    self.panel.selected = id;
                }
                effect = effect.merge(added);
            }
        }
        self.apply(effect);
        true
    }

    fn apply(&mut self, effect: Effect) {
        // Saving and the menu wait for their own frames (see `frame`).
        if effect.save && self.loader.is_none() {
            self.save_pending = true;
            self.save_text = None;
        }
        if effect.changed || effect.save {
            self.revision += 1;
        }
        if effect.republish {
            self.panel_pending = true;
        }
        if let Some(message) = &effect.message {
            let _ = host_hud::local_message(message);
        }
    }

    /// Reaching the death point removes it (when that is on).
    fn reached(&mut self) {
        let Some(world_id) = self.world.clone() else { return };
        let Some(world) = self.settings.worlds.iter_mut().find(|w| w.id == world_id) else { return };
        let (feet, dim) = (self.feet, self.dimension);
        let clear = self.settings.death_clear;
        let mut gone = false;
        world.points.retain_mut(|point| {
            if point.dim != dim || point.kind != Kind::Death {
                return true;
            }
            let flat = (point.pos[0] - feet[0]).abs().max((point.pos[2] - feet[2]).abs());
            let up = (point.pos[1] - feet[1]).abs();
            if flat > ARM_DISTANCE || up > ARM_DISTANCE {
                point.armed = true;
            } else if clear && point.armed && flat <= REACH_DISTANCE && up <= REACH_DISTANCE + 1 {
                gone = true;
                return false;
            }
            true
        });
        if gone {
            let _ = host_hud::local_message("Waypoints: death point reached, removed.");
            self.apply(Effect { save: true, republish: true, changed: true, message: None });
            self.busy = true;
        }
    }

    fn upload_shapes(&mut self) {
        let uploaded = (|| {
            Ok::<_, draw::DrawError>(Shapes {
                beam: mesh(shapes::beam(0.32, BEAM_HEIGHT, 0.55, 0.0))?,
                core: mesh(shapes::beam(0.09, BEAM_HEIGHT, 1.0, 0.15))?,
                gem: mesh(shapes::gem(GEM_HEIGHT, 0.3))?,
                floor: mesh(shapes::floor_mark())?,
            })
        })();
        match uploaded {
            Ok(shapes) => self.shapes = Some(shapes),
            Err(error) => {
                self.draw_error = Some(error.detail);
                self.shapes_failed = true;
                let _ = host_hud::local_message(
                    "Waypoints: allow \"World drawing\" for this mod to see waypoints in the world.",
                );
            }
        }
    }

    fn current_world(&self) -> Option<&crate::config::World> {
        self.world.as_deref().and_then(|w| self.settings.world(w))
    }

    /// Finds the nearest shown waypoints again; true when it did (every 8 blocks or on change).
    fn refresh_near(&mut self) -> bool {
        let coarse = self.feet.map(|v| v.div_euclid(8));
        let key = Some((self.revision, self.dimension, coarse, self.settings.show));
        if self.near_key == key {
            return false;
        }
        self.near_key = key;
        let (dim, feet) = (self.dimension, self.feet);
        let mut found: Vec<(i64, usize)> = match (self.settings.show, self.current_world()) {
            (true, Some(w)) => w
                .points
                .iter()
                .enumerate()
                .filter(|(_, p)| p.dim == dim && !p.hidden)
                .map(|(i, p)| (distance_sq(p.pos, feet), i))
                .collect(),
            _ => Vec::new(),
        };
        if found.len() > MAX_MARKS {
            found.select_nth_unstable(MAX_MARKS);
            found.truncate(MAX_MARKS);
        }
        found.sort_unstable();
        self.near = found.into_iter().map(|(_, i)| i).collect();
        // Keep each waypoint's meshes; drop those of waypoints no longer drawn.
        let ids: Vec<u32> = self.near_points().iter().map(|p| p.id).collect();
        let mut old = std::mem::take(&mut self.labels);
        for id in ids {
            let label = match old.iter().position(|l| l.id == id) {
                Some(i) => old.swap_remove(i),
                None => Label { id, badge_for: None, badge: None, name_for: None, name: Vec::new(), distance_for: None, distance: None },
            };
            self.labels.push(label);
        }
        for label in old {
            self.retired.extend(label.badge);
            self.retired.extend(label.name);
            self.retired.extend(label.distance);
        }
        self.placed = None;
        true
    }

    fn near_points(&self) -> Vec<&Waypoint> {
        match self.current_world() {
            Some(w) => self.near.iter().filter_map(|i| w.points.get(*i)).collect(),
            None => Vec::new(),
        }
    }

    /// The text under a badge: the distance, rounded more the farther it is.
    fn distance_text(&self, point: &Waypoint) -> String {
        let d = self.distance_to(point).round() as u32;
        let shown = match d {
            0..=999 => d,
            1000..=9999 => d / 10 * 10,
            _ => d / 100 * 100,
        };
        let mut text = String::with_capacity(10);
        crate::hud::distance_text(&mut text, shown);
        text
    }

    fn eye(&self) -> [f64; 3] {
        [self.position[0], self.position[1] + EYE_HEIGHT, self.position[2]]
    }

    fn badge_point(point: &Waypoint) -> [f64; 3] {
        [f64::from(point.pos[0]) + 0.5, f64::from(point.pos[1]) + BADGE_HEIGHT, f64::from(point.pos[2]) + 0.5]
    }

    fn distance_to(&self, point: &Waypoint) -> f64 {
        let p = [f64::from(point.pos[0]) + 0.5, f64::from(point.pos[1]), f64::from(point.pos[2]) + 0.5];
        let e = self.position;
        ((p[0] - e[0]).powi(2) + (p[1] - e[1]).powi(2) + (p[2] - e[2]).powi(2)).sqrt()
    }

    /// Makes the box, name and distance meshes that changed, at most one a frame (each costs a
    /// good part of a frame's fuel). True when it made one.
    fn make_meshes(&mut self) -> bool {
        if self.shapes.is_none() {
            return false;
        }
        let names = self.settings.show_names;
        let world = match self.current_world() {
            Some(w) => w,
            None => return false,
        };
        for (slot, &index) in self.near.iter().enumerate() {
            let Some(point) = world.points.get(index) else { continue };
            let Some(label) = self.labels.get(slot) else { continue };
            if label.id != point.id {
                continue;
            }
            let rgba = COLORS[point.color()];
            let boxed = point.boxed();
            let letters = if boxed { point.glyph() } else { "" };
            let changed_badge = match &label.badge_for {
                Some((l, i, c)) => l != letters || *i != point.icon || *c != point.color,
                None => true,
            };
            enum Part {
                Badge((String, u8, u8)),
                /// The next chunk of the name; true when the old name's meshes go first.
                Name((usize, usize), bool),
                NoName,
                Distance((String, u8, bool)),
            }
            let part = if changed_badge {
                Part::Badge((letters.to_string(), point.icon, point.color))
            } else if names && label.name_for.as_deref() != Some(point.name.as_str()) {
                Part::Name(label::name_chunks(&point.name)[0], true)
            } else if names && label.name.len() < label::name_chunks(&point.name).len() {
                Part::Name(label::name_chunks(&point.name)[label.name.len()], false)
            } else if !names && (!label.name.is_empty() || label.name_for.is_some()) {
                Part::NoName
            } else {
                let want = (self.distance_text(point), point.color, names);
                if label.distance_for.as_ref() == Some(&want) {
                    continue;
                }
                Part::Distance(want)
            };
            let built = match &part {
                Part::Badge((letters, icon, _)) => {
                    let mark = if boxed { label::Mark::Letters(letters) } else { label::Mark::Icon(usize::from(*icon) - 1) };
                    Some(badge_mesh(label::badge(mark, rgba, BADGE_UNIT, vertex)))
                }
                Part::Name(chunk, _) => Some(badge_mesh(label::name(&point.name, *chunk, BADGE_UNIT, vertex))),
                Part::NoName => None,
                Part::Distance((text, _, names)) => {
                    Some(badge_mesh(label::distance(text, rgba, label::distance_top(*names), BADGE_UNIT, vertex)))
                }
            };
            let name = point.name.clone();
            let label = &mut self.labels[slot];
            match (part, built) {
                (_, Some(Err(error))) => self.draw_error = Some(error.detail),
                (Part::Badge(key), Some(Ok(h))) => {
                    self.retired.extend(label.badge.replace(Key::of(&h)));
                    label.badge_for = Some(key);
                }
                (Part::Name(_, reset), Some(Ok(h))) => {
                    if reset {
                        self.retired.append(&mut label.name);
                        label.name_for = Some(name);
                    }
                    label.name.push(Key::of(&h));
                }
                (Part::Distance(key), Some(Ok(h))) => {
                    self.retired.extend(label.distance.replace(Key::of(&h)));
                    label.distance_for = Some(key);
                }
                _ => {
                    self.retired.append(&mut label.name);
                    label.name_for = None;
                }
            }
            self.meshes_made += 1;
            self.placed = None;
            return true;
        }
        false
    }

    /// Places every drawing: beacons at their blocks, badges along your line of sight. Done
    /// again when you move a little or something changed; unchanged drawings cost no calls.
    fn place(&mut self) {
        let eye = self.eye();
        if let Some((at, revision, made)) = self.placed {
            let moved = (0..3).map(|i| (at[i] - eye[i]).powi(2)).sum::<f64>();
            if moved < 0.2 * 0.2 && revision == self.revision && made == self.meshes_made {
                return;
            }
        }
        self.placed = Some((eye, self.revision, self.meshes_made));
        let dim = self.dimension;
        let always = !self.no_through_walls;
        let mut wanted = Vec::new();
        let beams = self.settings.beams;
        let beam_walls = self.settings.beams_through_walls && always;
        let k = (self.settings.beam_width * 0.5).clamp(0.25, 4.0) as f32;
        let size = self.settings.icon_size / 1.5;
        for point in self.near_points() {
            let [r, g, b, _] = COLORS[point.color()];
            let block = [f64::from(point.pos[0]), f64::from(point.pos[1]), f64::from(point.pos[2])];
            // Far beacons are switched off to save drawing work; the badge still shows.
            if beams && self.distance_to(point) <= BEACON_RANGE {
                let floor = Want::at(Geo::Floor, dim, block, [r, g, b, 1.0], beam_walls);
                // The glow parts add light, so their tint is how bright they are.
                let soft = [r * 0.55, g * 0.55, b * 0.55, 1.0];
                let bright = [0.5 + r * 0.5, 0.5 + g * 0.5, 0.5 + b * 0.5, 1.0];
                // The beam's size setting widens it about the block's centre.
                let widen = |want: Want| Want {
                    scale: [k, 1.0, k],
                    translation: [0.5 - 0.5 * k, 0.0, 0.5 - 0.5 * k],
                    ..want
                };
                wanted.push(floor);
                wanted.push(widen(Want::at(Geo::Beam, dim, block, soft, beam_walls)));
                wanted.push(widen(Want::at(Geo::Core, dim, block, bright, beam_walls)));
                wanted.push(Want { geo: Geo::Gem, ..floor });
            }
            let Some(label) = self.labels.iter().find(|l| l.id == point.id) else { continue };
            // The badge: pulled in to `LABEL_RANGE`, facing you, scaled to keep its size.
            let target = Self::badge_point(point);
            let to = [target[0] - eye[0], target[1] - eye[1], target[2] - eye[2]];
            let d = (to[0] * to[0] + to[1] * to[1] + to[2] * to[2]).sqrt().max(0.01);
            let placed = d.min(LABEL_RANGE);
            let at = [eye[0] + to[0] / d * placed, eye[1] + to[1] / d * placed, eye[2] + to[2] / d * placed];
            // Rounded a little so tiny moves do not resend every badge.
            let at = at.map(|v| (v * 16.0).round() / 16.0);
            let rotation = label::facing([eye[0] - at[0], eye[1] - at[1], eye[2] - at[2]]);
            let s = (placed.max(3.0) * LABEL_SCALE * size) as f32 / BADGE_UNIT;
            for key in [label.badge, label.distance].into_iter().flatten().chain(label.name.iter().copied()) {
                wanted.push(Want {
                    rotation,
                    scale: [s, s, s],
                    ..Want::at(Geo::Mesh(key), dim, at, [1.0; 4], always)
                });
            }
        }
        self.pool.want(wanted);
    }
}

mod_api::bindings::export!(Mod with_types_in mod_api::bindings);

/// Announces this mod to Mods Core, and sets and opens the menu when asked (or drops it when it
/// closes, so only one menu exists at a time; see `mods_core.rs`).
fn core_frame(panel_open: bool, whole_panel: impl FnOnce() -> String) -> crate::mods_core::Event {
    CORE.with_borrow_mut(|link| link.frame(panel_open, whole_panel))
}

/// The player's own menu key: opens the menu through Mods Core's hand-over, or closes it.
fn core_toggle(panel_open: bool) {
    CORE.with_borrow_mut(|link| link.toggle(panel_open));
}
