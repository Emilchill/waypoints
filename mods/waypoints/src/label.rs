//! Waypoint badges drawn in the world: a coloured box with the icon (or the name's first two
//! letters), the name under it and the distance under that. Letters and icons are smooth
//! triangles from `font`, not pixels, so they stay clean at any size. Units are text pixels,
//! +Z is the front; the mod turns the badge to face you and scales it with distance.
//!
//! Three separate parts, so a changing distance does not rebuild the name, and each fits in a
//! frame's fuel.

use crate::font::{BASELINE, GLYPHS, Glyph, HEIGHT, ICON, ICONS, UNIT};

const BOX: f32 = 17.0;
const PAD: f32 = 3.0;
const LINE: f32 = HEIGHT as f32 + 1.0;
/// Extra space between letters, in pixels.
const TRACKING: f32 = 0.4;
const PLATE: [f32; 4] = [0.0, 0.0, 0.0, 0.55];
const EDGE: [f32; 4] = [0.04, 0.04, 0.06, 0.9];
const WHITE: [f32; 4] = [1.0, 1.0, 1.0, 1.0];
const DARK: [f32; 4] = [0.05, 0.05, 0.07, 1.0];

/// A mesh being built: `make` turns a point and a premultiplied colour into the host's vertex,
/// and every point is multiplied by `scale` first.
pub struct Builder<V, F: Fn([f32; 3], [f32; 4]) -> V> {
    pub vertices: Vec<V>,
    pub indices: Vec<u32>,
    scale: f32,
    make: F,
}

impl<V, F: Fn([f32; 3], [f32; 4]) -> V> Builder<V, F> {
    pub fn new(scale: f32, capacity: usize, make: F) -> Self {
        Self { vertices: Vec::with_capacity(capacity), indices: Vec::with_capacity(capacity * 2), scale, make }
    }

    fn rect(&mut self, x0: f32, y0: f32, x1: f32, y1: f32, z: f32, color: [f32; 4]) {
        let k = self.scale;
        let base = self.vertices.len() as u32;
        let (x0, y0, x1, y1, z) = (x0 * k, y0 * k, x1 * k, y1 * k, z * k);
        for (x, y) in [(x0, y0), (x1, y0), (x1, y1), (x0, y1)] {
            self.vertices.push((self.make)([x, y, z], color));
        }
        self.indices.extend_from_slice(&[base, base + 1, base + 2, base, base + 2, base + 3]);
    }

    /// A glyph's triangles with its origin at (x, y) pixels.
    fn shape(&mut self, points: &[(i16, i16)], triangles: &[u8], x: f32, y: f32, z: f32, color: [f32; 4]) {
        let k = self.scale;
        let unit = k / UNIT;
        let (ox, oy, z) = (x * k, y * k, z * k);
        let base = self.vertices.len() as u32;
        for &(px, py) in points {
            self.vertices.push((self.make)([ox + f32::from(px) * unit, oy + f32::from(py) * unit, z], color));
        }
        self.indices.extend(triangles.iter().map(|&i| base + u32::from(i)));
    }

    /// `text` with the bottom-left of its line at (x, y).
    fn text(&mut self, text: &str, x: f32, y: f32, z: f32, color: [f32; 4]) {
        let mut cursor = x;
        for ch in text.chars() {
            let g = glyph(ch);
            self.shape(g.points, g.triangles, cursor, y + BASELINE, z, color);
            cursor += advance(g);
        }
    }
}

fn glyph(ch: char) -> &'static Glyph {
    // Printable ASCII is stored in order from the space, so it is found by index.
    let code = ch as u32;
    if (32..127).contains(&code) {
        return &GLYPHS[(code - 32) as usize];
    }
    GLYPHS.iter().find(|g| g.ch == ch).unwrap_or(&GLYPHS[('?' as u32 - 32) as usize])
}

fn advance(g: &Glyph) -> f32 {
    f32::from(g.advance) / UNIT + TRACKING
}

/// Width of `text` in pixels.
pub fn width(text: &str) -> f32 {
    let w: f32 = text.chars().map(|ch| advance(glyph(ch))).sum();
    (w - TRACKING).max(0.0)
}

/// Points a part will have, roughly: for the vectors' capacity.
fn points(text: &str) -> usize {
    8 + text.chars().map(|ch| glyph(ch).points.len()).sum::<usize>()
}

/// Premultiplied colour, as the drawing's blending expects.
fn premultiply(c: [f32; 4]) -> [f32; 4] {
    [c[0] * c[3], c[1] * c[3], c[2] * c[3], c[3]]
}

/// Light text on dark colours, dark text on light ones.
pub fn ink(color: [f32; 4]) -> [f32; 4] {
    if 0.3 * color[0] + 0.59 * color[1] + 0.11 * color[2] > 0.6 { DARK } else { WHITE }
}

/// What goes in the box.
pub enum Mark<'a> {
    /// Up to two letters of the name.
    Letters(&'a str),
    /// One of the 20 icons.
    Icon(usize),
}

/// The box, centred on the origin.
pub fn badge<V, F: Fn([f32; 3], [f32; 4]) -> V>(mark: Mark, color: [f32; 4], scale: f32, make: F) -> Builder<V, F> {
    let mut out = Builder::new(scale, 160, make);
    let h = BOX / 2.0;
    out.rect(-h - 1.5, -h - 1.5, h + 1.5, h + 1.5, 0.0, premultiply(EDGE));
    out.rect(-h, -h, h, h, 0.01, [color[0], color[1], color[2], 1.0]);
    let ink = ink(color);
    match mark {
        Mark::Letters(letters) => {
            let w = width(letters);
            out.text(letters, -w / 2.0, -f32::from(HEIGHT) / 2.0 + 0.5, 0.02, ink);
        }
        Mark::Icon(index) => {
            let icon = &ICONS[index % ICONS.len()];
            let o = -f32::from(ICON) / 2.0;
            out.shape(icon.points, icon.triangles, o, o, 0.02, ink);
        }
    }
    out
}

/// Points one name mesh may hold: more would not fit in a frame's fuel.
const CHUNK_POINTS: usize = 110;

/// How a name is cut into meshes: character ranges of at most `CHUNK_POINTS` points.
pub fn name_chunks(name: &str) -> Vec<(usize, usize)> {
    let mut out = Vec::with_capacity(4);
    let (mut start, mut points) = (0, 8);
    for (i, ch) in name.chars().enumerate() {
        let n = glyph(ch).points.len();
        if points + n > CHUNK_POINTS && i > start {
            out.push((start, i));
            start = i;
            points = 0;
        }
        points += n;
    }
    out.push((start, name.chars().count()));
    out
}

/// One chunk (`chars` range) of the name line under the box; the first one carries the plate.
pub fn name<V, F: Fn([f32; 3], [f32; 4]) -> V>(name: &str, chars: (usize, usize), scale: f32, make: F) -> Builder<V, F> {
    let mut out = Builder::new(scale, CHUNK_POINTS + 8, make);
    let w = width(name);
    let top = -BOX / 2.0 - 4.0;
    if chars.0 == 0 {
        out.rect(-w / 2.0 - PAD, top - LINE - 2.0, w / 2.0 + PAD, top, 0.0, premultiply(PLATE));
    }
    let x: f32 = -w / 2.0 + name.chars().take(chars.0).map(|ch| advance(glyph(ch))).sum::<f32>();
    let piece: String = name.chars().skip(chars.0).take(chars.1 - chars.0).collect();
    out.text(&piece, x, top - LINE - 0.5, 0.01, WHITE);
    out
}

/// Where the distance line sits: under the name, or right under the box.
pub fn distance_top(with_name: bool) -> f32 {
    let below_box = -BOX / 2.0 - 4.0;
    if with_name { below_box - LINE - 4.0 } else { below_box }
}

/// The distance line, `"120 m"`, centred, in a light shade of the waypoint's colour.
pub fn distance<V, F: Fn([f32; 3], [f32; 4]) -> V>(label: &str, color: [f32; 4], top: f32, scale: f32, make: F) -> Builder<V, F> {
    let mut out = Builder::new(scale, points(label), make);
    let w = width(label);
    out.rect(-w / 2.0 - PAD, top - LINE - 2.0, w / 2.0 + PAD, top, 0.0, premultiply(PLATE));
    let light = [0.55 + color[0] * 0.45, 0.55 + color[1] * 0.45, 0.55 + color[2] * 0.45, 1.0];
    out.text(label, -w / 2.0, top - LINE - 0.5, 0.01, light);
    out
}

/// Billboard rotation (x, y, z, w) that turns the badge's front (+Z) towards `to`.
pub fn facing(to: [f64; 3]) -> [f32; 4] {
    let (x, y, z) = (to[0], to[1], to[2]);
    let flat = (x * x + z * z).sqrt();
    if flat < 1e-6 && y.abs() < 1e-6 {
        return [0.0, 0.0, 0.0, 1.0];
    }
    let yaw = x.atan2(z);
    let pitch = -(y.atan2(flat));
    let (sy, cy) = ((yaw / 2.0).sin(), (yaw / 2.0).cos());
    let (sp, cp) = ((pitch / 2.0).sin(), (pitch / 2.0).cos());
    // Yaw about Y, then pitch about the turned X axis: q = q_yaw * q_pitch.
    let q = [cy * sp, sy * cp, -sy * sp, cy * cp];
    let len = q.iter().map(|v| v * v).sum::<f64>().sqrt();
    q.map(|v| (v / len) as f32)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn rotate(q: [f32; 4], v: [f32; 3]) -> [f32; 3] {
        let [x, y, z, w] = q;
        let u = [x, y, z];
        let dot = |a: [f32; 3], b: [f32; 3]| a[0] * b[0] + a[1] * b[1] + a[2] * b[2];
        let cross = |a: [f32; 3], b: [f32; 3]| [a[1] * b[2] - a[2] * b[1], a[2] * b[0] - a[0] * b[2], a[0] * b[1] - a[1] * b[0]];
        let c = cross(u, v);
        let k = dot(u, v);
        let s = w * w - dot(u, u);
        [0, 1, 2].map(|i| 2.0 * k * u[i] + s * v[i] + 2.0 * w * c[i])
    }

    #[test]
    fn badges_face_the_player() {
        for to in [[5.0, 0.0, 0.0], [0.0, 0.0, -3.0], [3.0, 4.0, 2.0], [-1.0, -2.0, 0.5]] {
            let q = facing(to);
            let front = rotate(q, [0.0, 0.0, 1.0]);
            let len = (to[0] * to[0] + to[1] * to[1] + to[2] * to[2]).sqrt();
            for i in 0..3 {
                assert!((f64::from(front[i]) - to[i] / len).abs() < 1e-4, "{to:?} {front:?}");
            }
            // The text's up stays up-ish (no roll).
            let right = rotate(q, [1.0, 0.0, 0.0]);
            assert!(right[1].abs() < 1e-4, "{right:?}");
        }
    }

    /// Triangles must face +Z (counter-clockwise seen from the front), or they are culled.
    fn front_facing(b: &Builder<([f32; 3], [f32; 4]), impl Fn([f32; 3], [f32; 4]) -> ([f32; 3], [f32; 4])>) -> bool {
        b.indices.chunks(3).all(|t| {
            let [a, b2, c] = [t[0], t[1], t[2]].map(|i| b.vertices[i as usize].0);
            (b2[0] - a[0]) * (c[1] - a[1]) - (b2[1] - a[1]) * (c[0] - a[0]) >= 0.0
        })
    }

    #[test]
    fn parts_are_small_and_face_forward() {
        let make = |p: [f32; 3], c: [f32; 4]| (p, c);
        assert_eq!(glyph('A').ch, 'A');
        assert_eq!(glyph('~').ch, '~');
        assert_eq!(glyph('ñ').ch, 'ñ');
        assert_eq!(glyph('€').ch, '?');
        let b = badge(Mark::Letters("Ho"), [1.0, 0.67, 0.0, 1.0], 0.25, make);
        let long = "Casa de Emiliano 2025 ñ";
        let chunks = name_chunks(long);
        assert!(chunks.len() >= 2 && chunks[0].0 == 0 && chunks.last().unwrap().1 == long.chars().count(), "{chunks:?}");
        for c in &chunks {
            assert!(name(long, *c, 0.25, make).vertices.len() <= CHUNK_POINTS + 8);
        }
        assert_eq!(name_chunks("Home"), vec![(0, 4)]);
        let n = name(long, chunks[0], 0.25, make);
        let d = distance("12.3 km", [0.3, 0.3, 1.0, 1.0], distance_top(true), 0.25, make);
        assert!(b.vertices.len() < 120 && n.vertices.len() < 600 && d.vertices.len() < 200, "{} {} {}", b.vertices.len(), n.vertices.len(), d.vertices.len());
        for part in [&b, &n, &d] {
            assert!(front_facing(part));
            assert!(part.indices.iter().all(|i| (*i as usize) < part.vertices.len()));
            assert!(part.vertices.iter().all(|(p, _)| p[0].abs() < 40.0 && p[1].abs() < 40.0));
        }
        for i in 0..ICONS.len() {
            let icon = badge(Mark::Icon(i), [0.0; 4], 0.25, make);
            assert!(front_facing(&icon) && icon.vertices.len() > 10, "{i}");
        }
        assert!(width("Home") > 20.0);
    }
}

#[cfg(test)]
mod dump {
    use super::*;

    /// `WP_DUMP=path cargo test dump_badges` writes a few badges as triangles for a preview.
    #[test]
    fn dump_badges() {
        let Some(path) = std::env::var_os("WP_DUMP") else { return };
        let make = |p: [f32; 3], c: [f32; 4]| (p, c);
        let mut out = String::new();
        let parts = |out: &mut String, x: f32, b: &Builder<([f32; 3], [f32; 4]), _>| {
            for t in b.indices.chunks(3) {
                for &i in t {
                    let (p, c) = b.vertices[i as usize];
                    out.push_str(&format!("{} {} {} {} {} {} {} ", p[0] + x, p[1], p[2], c[0], c[1], c[2], c[3]));
                }
                out.push('\n');
            }
        };
        let items: [(Mark, [f32; 4], &str, &str); 4] = [
            (Mark::Letters("Ca"), [1.0, 0.67, 0.0, 1.0], "Casa de Emil", "125 m"),
            (Mark::Icon(5), [1.0, 0.33, 0.33, 1.0], "Death", "1.2 km"),
            (Mark::Icon(0), [0.33, 0.33, 1.0, 1.0], "Mina de diamantes ñ", "48 m"),
            (Mark::Icon(17), [0.33, 1.0, 0.33, 1.0], "Portal", "3 m"),
        ];
        for (i, (mark, color, n, d)) in items.into_iter().enumerate() {
            let x = i as f32 * 120.0;
            parts(&mut out, x, &badge(mark, color, 1.0, make));
            for c in name_chunks(n) {
                parts(&mut out, x, &name(n, c, 1.0, make));
            }
            parts(&mut out, x, &distance(d, color, distance_top(true), 1.0, make));
        }
        std::fs::write(path, out).unwrap();
    }
}
