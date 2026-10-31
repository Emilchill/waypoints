//! Filled shapes uploaded once and placed with world anchors: a beacon-like beam, a gem and a
//! mark on the floor. Every shape stays within 64 blocks of its origin (the block's corner).

pub type Point = [f32; 3];

/// Triangles with a premultiplied colour per vertex (the drawing's tint multiplies it).
#[derive(Default, Debug)]
pub struct Mesh {
    pub vertices: Vec<(Point, [f32; 4])>,
    pub indices: Vec<u32>,
}

impl Mesh {
    /// A quad seen from its front only (+Z, the side badges turn towards you).
    pub fn front_quad(&mut self, corners: [Point; 4], color: [f32; 4]) {
        let base = self.vertices.len() as u32;
        for corner in corners {
            self.vertices.push((corner, color));
        }
        self.indices.extend([base, base + 1, base + 2, base, base + 2, base + 3]);
    }

    pub fn colored_quad(&mut self, corners: [Point; 4], color: [f32; 4]) {
        let base = self.vertices.len() as u32;
        for corner in corners {
            self.vertices.push((corner, color));
        }
        // Both sides, so it shows whatever the renderer culls.
        self.indices.extend([base, base + 1, base + 2, base, base + 2, base + 3]);
        self.indices.extend([base, base + 2, base + 1, base, base + 3, base + 2]);
    }

    fn quad(&mut self, corners: [Point; 4], alpha: [f32; 4]) {
        let base = self.vertices.len() as u32;
        for (corner, a) in corners.into_iter().zip(alpha) {
            self.vertices.push((corner, [a; 4]));
        }
        self.indices.extend([base, base + 1, base + 2, base, base + 2, base + 3]);
        self.indices.extend([base, base + 2, base + 1, base, base + 3, base + 2]);
    }

    fn triangle(&mut self, corners: [Point; 3], alpha: f32) {
        let base = self.vertices.len() as u32;
        for corner in corners {
            self.vertices.push((corner, [alpha; 4]));
        }
        self.indices.extend([base, base + 1, base + 2, base, base + 2, base + 1]);
    }
}

/// A beacon beam: two crossed planes through the block's centre, fading out towards the top.
pub fn beam(half_width: f32, height: f32, bottom: f32, top: f32) -> Mesh {
    let mut mesh = Mesh::default();
    let (c, w) = (0.5, half_width);
    mesh.quad(
        [[c - w, 0.0, c], [c + w, 0.0, c], [c + w, height, c], [c - w, height, c]],
        [bottom, bottom, top, top],
    );
    mesh.quad(
        [[c, 0.0, c - w], [c, 0.0, c + w], [c, height, c + w], [c, height, c - w]],
        [bottom, bottom, top, top],
    );
    mesh
}

/// A gem (octahedron) floating `y` blocks above the block, `r` blocks from centre to tip.
pub fn gem(y: f32, r: f32) -> Mesh {
    let mut mesh = Mesh::default();
    let c = [0.5, y, 0.5];
    let top = [c[0], c[1] + r * 1.4, c[2]];
    let bottom = [c[0], c[1] - r * 1.4, c[2]];
    let ring = [[c[0] + r, c[1], c[2]], [c[0], c[1], c[2] + r], [c[0] - r, c[1], c[2]], [c[0], c[1], c[2] - r]];
    for i in 0..4 {
        let (a, b) = (ring[i], ring[(i + 1) % 4]);
        // Lighter upper half, darker lower half: reads as a lit gem.
        mesh.triangle([top, a, b], 1.0);
        mesh.triangle([bottom, b, a], 0.6);
    }
    mesh
}

/// A soft square on the floor of the block, with a brighter frame.
pub fn floor_mark() -> Mesh {
    let mut mesh = Mesh::default();
    let y = 0.03;
    mesh.quad([[0.0, y, 0.0], [1.0, y, 0.0], [1.0, y, 1.0], [0.0, y, 1.0]], [0.35; 4]);
    let t = 0.1;
    for (x0, z0, x1, z1) in [(0.0, 0.0, 1.0, t), (0.0, 1.0 - t, 1.0, 1.0), (0.0, t, t, 1.0 - t), (1.0 - t, t, 1.0, 1.0 - t)] {
        mesh.quad([[x0, y + 0.005, z0], [x1, y + 0.005, z0], [x1, y + 0.005, z1], [x0, y + 0.005, z1]], [0.9; 4]);
    }
    mesh
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn shapes_stay_near_their_origin_and_index_their_vertices() {
        for mesh in [beam(0.3, 60.0, 0.6, 0.0), gem(3.0, 0.35), floor_mark()] {
            assert!(mesh.vertices.iter().all(|(p, _)| (p[0] * p[0] + p[1] * p[1] + p[2] * p[2]).sqrt() < 64.0));
            assert!(mesh.indices.iter().all(|i| (*i as usize) < mesh.vertices.len()));
            assert_eq!(mesh.indices.len() % 3, 0);
        }
    }
}
