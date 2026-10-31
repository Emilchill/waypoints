//! The HUD: a floating icon over each waypoint, with its name and distance.

use crate::config::{Labels, Settings, TEXT_JSON};
use crate::json::{num, push_u};

/// Labels further than this from your eyes are pulled in along the same line of sight: the
/// screen position is the same, and the camera's far plane would hide them otherwise.
pub const LABEL_RANGE: f32 = 48.0;
/// Waypoints shown at once, nearest first (two labels each; the host allows 64 labels).
pub const MAX_SHOWN: usize = 16;

/// One waypoint as the HUD shows it.
#[derive(Clone, Debug, PartialEq)]
pub struct Mark<'a> {
    /// Where the label is drawn (already pulled in to `LABEL_RANGE`).
    pub label: [f32; 3],
    pub glyph: &'a str,
    /// The glyph is two letters in a box.
    pub boxed: bool,
    pub name: &'a str,
    pub distance: u32,
    pub color: usize,
}

/// Where a label goes for a waypoint at `target` seen from `eye`.
pub fn label_position(eye: [f32; 3], target: [f32; 3]) -> [f32; 3] {
    let v = [target[0] - eye[0], target[1] - eye[1], target[2] - eye[2]];
    let d = (v[0] * v[0] + v[1] * v[1] + v[2] * v[2]).sqrt();
    if d <= LABEL_RANGE || !d.is_finite() {
        return target;
    }
    let k = LABEL_RANGE / d;
    [eye[0] + v[0] * k, eye[1] + v[1] * k, eye[2] + v[2] * k]
}

pub fn distance_text(out: &mut String, blocks: u32) {
    if blocks < 1000 {
        push_u(out, u64::from(blocks));
        out.push_str(" m");
    } else {
        let tenths = (blocks + 50) / 100;
        push_u(out, u64::from(tenths / 10));
        out.push('.');
        push_u(out, u64::from(tenths % 10));
        out.push_str(" km");
    }
}

/// A coordinate to a tenth of a block, which is plenty for a label.
fn tenths(out: &mut String, value: f32) {
    let v = (value * 10.0).round() as i32;
    let abs = v.unsigned_abs();
    crate::json::push_u32(out, abs / 10, v < 0);
    if abs % 10 != 0 {
        out.push('.');
        out.push((b'0' + (abs % 10) as u8) as char);
    }
}

fn position(out: &mut String, p: [f32; 3]) {
    out.push('[');
    tenths(out, p[0]);
    out.push(',');
    tenths(out, p[1]);
    out.push(',');
    tenths(out, p[2]);
    out.push(']');
}

/// Blank lines that push the name below the icon, which sits on the same point: half the icon's
/// height, `(10 + 2 * padding) * scale / 2`, plus a gap.
fn spacer_lines(icon_size: f64, padding: f64) -> usize {
    (((5.0 + padding) * icon_size + 11.0) / 5.0).ceil() as usize
}

pub fn build(marks: &[Mark], settings: &Settings) -> String {
    let shown = &marks[..marks.len().min(MAX_SHOWN)];
    let mut out = String::with_capacity(64 + shown.len() * 320);
    out.push_str("{\"cards\":[],\"world_labels\":[");
    // The parts every label shares are written once.
    let mut icon_tail = String::with_capacity(80);
    icon_tail.push_str(",\"scale\":");
    num(&mut icon_tail, settings.icon_size);
    icon_tail.push_str(",\"padding\":1,\"background_opacity\":0,\"shadow\":true}");
    // Boxed: the letters in a dark box with a coloured border.
    let mut box_tail = String::with_capacity(96);
    box_tail.push_str(",\"scale\":");
    num(&mut box_tail, settings.icon_size);
    box_tail.push_str(",\"padding\":2,\"border_width\":2,\"background_opacity\":0.7,\"shadow\":true}");
    let mut text_tail = String::with_capacity(80);
    text_tail.push_str(",\"padding\":2,\"background_opacity\":0,\"shadow\":true");
    if settings.labels == Labels::Looking {
        text_tail.push_str(",\"near_angle\":");
        num(&mut text_tail, settings.look_angle);
    }
    text_tail.push('}');
    let spacer = "\\n".repeat(spacer_lines(settings.icon_size, 1.0));
    let box_spacer = "\\n".repeat(spacer_lines(settings.icon_size, 2.0));
    let mut at = String::with_capacity(32);
    for (index, mark) in shown.iter().enumerate() {
        if index > 0 {
            out.push(',');
        }
        let color = TEXT_JSON[mark.color % TEXT_JSON.len()];
        at.clear();
        position(&mut at, mark.label);
        out.push_str("{\"position\":");
        out.push_str(&at);
        out.push_str(",\"text\":\"");
        crate::json::escape_into(&mut out, mark.glyph);
        out.push_str("\",\"color\":");
        out.push_str(color);
        out.push_str(if mark.boxed { &box_tail } else { &icon_tail });
        if settings.labels == Labels::Never {
            continue;
        }
        out.push_str(",{\"position\":");
        out.push_str(&at);
        out.push_str(",\"text\":\"");
        out.push_str(if mark.boxed { &box_spacer } else { &spacer });
        crate::json::escape_into(&mut out, mark.name);
        out.push_str("\\n");
        distance_text(&mut out, mark.distance);
        out.push_str("\",\"color\":");
        out.push_str(color);
        out.push_str(&text_tail);
    }
    out.push_str("]}");
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::Value;

    fn marks() -> Vec<Mark<'static>> {
        vec![
            Mark { label: [10.5, 65.5, -3.5], glyph: "★", boxed: false, name: "Home \"1\"", distance: 12, color: 3 },
            Mark { label: [40.0, 70.0, 0.0], glyph: "De", boxed: true, name: "Death", distance: 1234, color: 0 },
        ]
    }

    #[test]
    fn labels_are_valid_json() {
        let settings = Settings::default();
        let text = build(&marks(), &settings);
        let json: Value = serde_json::from_str(&text).unwrap();
        let labels = json["world_labels"].as_array().unwrap();
        assert_eq!(labels.len(), 4);
        assert_eq!(labels[0]["text"], "★");
        let detail = labels[1]["text"].as_str().unwrap();
        assert!(detail.ends_with("Home \"1\"\n12 m"), "{detail}");
        assert!(detail.starts_with("\n\n\n\n"));
        assert_eq!(labels[1]["near_angle"], 8.0);
        assert!(labels[3]["text"].as_str().unwrap().ends_with("Death\n1.2 km"));
        assert_eq!((labels[2]["text"].as_str().unwrap(), labels[2]["border_width"].as_f64()), ("De", Some(2.0)));
        assert_eq!(labels[2]["color"][0], 0.6);
        let never = Settings { labels: Labels::Never, ..Settings::default() };
        let json: Value = serde_json::from_str(&build(&marks(), &never)).unwrap();
        assert_eq!(json["world_labels"].as_array().unwrap().len(), 2);
        assert!(json["cards"].as_array().unwrap().is_empty());
        if std::env::var_os("WP_DUMP").is_some() {
            std::fs::write("/home/claude/wp-hud.json", &text).unwrap();
            let many: Vec<Mark> = (0..30).map(|n| Mark { label: [n as f32, 70.0, -40.5], glyph: crate::config::ICONS[n % 20], boxed: n % 3 == 0, name: "Ancient City 12", distance: 40_000, color: n % 16 }).collect();
            let big = Settings { icon_size: 3.0, look_angle: 30.0, ..Settings::default() };
            std::fs::write("/home/claude/wp-hud-many.json", build(&many, &big)).unwrap();
        }
    }

    #[test]
    fn far_labels_are_pulled_in() {
        let p = label_position([0.0, 64.0, 0.0], [1000.0, 64.0, 0.0]);
        assert!((p[0] - LABEL_RANGE).abs() < 0.01 && (p[1] - 64.0).abs() < 0.01);
        assert_eq!(label_position([0.0, 0.0, 0.0], [3.0, 4.0, 0.0]), [3.0, 4.0, 0.0]);
        let mut s = String::new();
        distance_text(&mut s, 999);
        s.push(' ');
        distance_text(&mut s, 15_960);
        assert_eq!(s, "999 m 16.0 km");
    }
}
