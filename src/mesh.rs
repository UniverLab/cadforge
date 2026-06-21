//! Triangle meshes + CSG booleans for the solid 3D view.
//!
//! Solids are triangle meshes. Booleans (`union`/`difference`/`intersection`)
//! use a BSP-tree algorithm (the classic csg.js approach) — no dependencies,
//! deterministic. Primitives (`box`, `cylinder`, extruded `prism`) tessellate
//! to meshes; [`render3d`](crate::render3d) projects and shades the result.

/// A 3D point / vector.
pub type V3 = [f64; 3];

fn add(a: V3, b: V3) -> V3 {
    [a[0] + b[0], a[1] + b[1], a[2] + b[2]]
}
fn sub(a: V3, b: V3) -> V3 {
    [a[0] - b[0], a[1] - b[1], a[2] - b[2]]
}
fn scale(a: V3, s: f64) -> V3 {
    [a[0] * s, a[1] * s, a[2] * s]
}
fn dot(a: V3, b: V3) -> f64 {
    a[0] * b[0] + a[1] * b[1] + a[2] * b[2]
}
fn cross(a: V3, b: V3) -> V3 {
    [
        a[1] * b[2] - a[2] * b[1],
        a[2] * b[0] - a[0] * b[2],
        a[0] * b[1] - a[1] * b[0],
    ]
}
fn normalize(a: V3) -> V3 {
    let l = dot(a, a).sqrt();
    if l < 1e-12 {
        a
    } else {
        scale(a, 1.0 / l)
    }
}
fn lerp(a: V3, b: V3, t: f64) -> V3 {
    add(a, scale(sub(b, a), t))
}

/// A triangle mesh (the only solid representation we keep).
#[derive(Clone, Default)]
pub struct Mesh {
    pub tris: Vec<[V3; 3]>,
}

impl Mesh {
    fn from_polys(polys: &[Poly]) -> Mesh {
        let mut tris = Vec::new();
        for p in polys {
            // Fan-triangulate (BSP output polygons are convex).
            for i in 1..p.verts.len().saturating_sub(1) {
                tris.push([p.verts[0], p.verts[i], p.verts[i + 1]]);
            }
        }
        Mesh { tris }
    }

    fn to_polys(&self) -> Vec<Poly> {
        self.tris.iter().map(|t| Poly::new(t.to_vec())).collect()
    }

    /// Boolean union (`self` ∪ `other`).
    pub fn union(&self, other: &Mesh) -> Mesh {
        Mesh::from_polys(&csg_union(self.to_polys(), other.to_polys()))
    }

    /// Boolean difference (`self` − `other`).
    pub fn difference(&self, other: &Mesh) -> Mesh {
        Mesh::from_polys(&csg_difference(self.to_polys(), other.to_polys()))
    }

    /// Boolean intersection (`self` ∩ `other`).
    pub fn intersection(&self, other: &Mesh) -> Mesh {
        Mesh::from_polys(&csg_intersection(self.to_polys(), other.to_polys()))
    }
}

// ── Primitive builders ──────────────────────────────────────────────────

/// Axis-aligned box with its minimum corner at `at` and the given `size`.
pub fn box_solid(at: V3, size: V3) -> Mesh {
    let [x, y, z] = at;
    let [sx, sy, sz] = size;
    let c = [
        [x, y, z],                // 0
        [x + sx, y, z],           // 1
        [x + sx, y + sy, z],      // 2
        [x, y + sy, z],           // 3
        [x, y, z + sz],           // 4
        [x + sx, y, z + sz],      // 5
        [x + sx, y + sy, z + sz], // 6
        [x, y + sy, z + sz],      // 7
    ];
    // Each face as two triangles, wound CCW as seen from outside (consistent
    // outward normals — the CSG BSP relies on it).
    let faces = [
        ([0, 3, 2], [0, 2, 1]), // bottom -Z
        ([4, 5, 6], [4, 6, 7]), // top +Z
        ([0, 1, 5], [0, 5, 4]), // -Y
        ([1, 2, 6], [1, 6, 5]), // +X
        ([2, 3, 7], [2, 7, 6]), // +Y
        ([3, 0, 4], [3, 4, 7]), // -X
    ];
    let mut tris = Vec::with_capacity(12);
    for (t1, t2) in faces {
        tris.push([c[t1[0]], c[t1[1]], c[t1[2]]]);
        tris.push([c[t2[0]], c[t2[1]], c[t2[2]]]);
    }
    Mesh { tris }
}

/// Vertical cylinder: base circle centered at `at`, radius `r`, height `h`.
pub fn cylinder_solid(at: V3, r: f64, h: f64, segments: usize) -> Mesh {
    let segs = segments.max(3);
    let [cx, cy, cz] = at;
    let top_c = [cx, cy, cz + h];
    let bot_c = [cx, cy, cz];
    let ring = |z: f64| -> Vec<V3> {
        (0..segs)
            .map(|i| {
                let a = (i as f64 / segs as f64) * std::f64::consts::TAU;
                [cx + r * a.cos(), cy + r * a.sin(), z]
            })
            .collect()
    };
    let bot = ring(cz);
    let top = ring(cz + h);
    let mut tris = Vec::with_capacity(segs * 4);
    for i in 0..segs {
        let j = (i + 1) % segs;
        // Side (two triangles), outward.
        tris.push([bot[i], bot[j], top[j]]);
        tris.push([bot[i], top[j], top[i]]);
        // Bottom fan (facing -Z) and top fan (facing +Z).
        tris.push([bot_c, bot[j], bot[i]]);
        tris.push([top_c, top[i], top[j]]);
    }
    Mesh { tris }
}

/// Extrude a 2D footprint from `base` to `base + height`. `closed` adds caps
/// (a solid prism); otherwise only side walls are produced (an open wall).
pub fn prism(footprint: &[[f64; 2]], base: f64, height: f64, closed: bool) -> Mesh {
    let n = footprint.len();
    if n < 2 {
        return Mesh::default();
    }
    let top = base + height;
    let mut tris = Vec::new();
    let edges = if closed { n } else { n - 1 };
    for i in 0..edges {
        let a = footprint[i];
        let b = footprint[(i + 1) % n];
        let (a0, a1) = ([a[0], a[1], base], [a[0], a[1], top]);
        let (b0, b1) = ([b[0], b[1], base], [b[0], b[1], top]);
        tris.push([a0, b0, b1]);
        tris.push([a0, b1, a1]);
    }
    if closed && n >= 3 {
        // Caps (fan). Bottom faces -Z, top faces +Z.
        for i in 1..n - 1 {
            let t0 = [footprint[0][0], footprint[0][1], top];
            let ti = [footprint[i][0], footprint[i][1], top];
            let tj = [footprint[i + 1][0], footprint[i + 1][1], top];
            tris.push([t0, ti, tj]);
            let b0 = [footprint[0][0], footprint[0][1], base];
            let bi = [footprint[i][0], footprint[i][1], base];
            let bj = [footprint[i + 1][0], footprint[i + 1][1], base];
            tris.push([b0, bj, bi]);
        }
    }
    Mesh { tris }
}

// ── BSP CSG (port of csg.js) ──────────────────────────────────────────────

const EPS: f64 = 1e-5;

#[derive(Clone)]
struct Plane {
    normal: V3,
    w: f64,
}

impl Plane {
    fn from_points(a: V3, b: V3, c: V3) -> Plane {
        let normal = normalize(cross(sub(b, a), sub(c, a)));
        Plane {
            w: dot(normal, a),
            normal,
        }
    }
    fn flip(&mut self) {
        self.normal = scale(self.normal, -1.0);
        self.w = -self.w;
    }
}

#[derive(Clone)]
struct Poly {
    verts: Vec<V3>,
    plane: Plane,
}

impl Poly {
    fn new(verts: Vec<V3>) -> Poly {
        let plane = Plane::from_points(verts[0], verts[1], verts[2]);
        Poly { verts, plane }
    }
    fn flip(&mut self) {
        self.verts.reverse();
        self.plane.flip();
    }
}

const COPLANAR: u8 = 0;
const FRONT: u8 = 1;
const BACK: u8 = 2;
const SPANNING: u8 = 3;

/// Split `poly` by `plane`, appending the pieces to the appropriate buckets.
fn split_polygon(
    plane: &Plane,
    poly: &Poly,
    coplanar_front: &mut Vec<Poly>,
    coplanar_back: &mut Vec<Poly>,
    front: &mut Vec<Poly>,
    back: &mut Vec<Poly>,
) {
    let mut poly_type = 0u8;
    let mut types = Vec::with_capacity(poly.verts.len());
    for v in &poly.verts {
        let t = dot(plane.normal, *v) - plane.w;
        let ty = if t < -EPS {
            BACK
        } else if t > EPS {
            FRONT
        } else {
            COPLANAR
        };
        poly_type |= ty;
        types.push(ty);
    }
    match poly_type {
        COPLANAR => {
            if dot(plane.normal, poly.plane.normal) > 0.0 {
                coplanar_front.push(poly.clone());
            } else {
                coplanar_back.push(poly.clone());
            }
        }
        FRONT => front.push(poly.clone()),
        BACK => back.push(poly.clone()),
        _ => {
            let mut fv = Vec::new();
            let mut bv = Vec::new();
            let n = poly.verts.len();
            for i in 0..n {
                let j = (i + 1) % n;
                let (ti, tj) = (types[i], types[j]);
                let (vi, vj) = (poly.verts[i], poly.verts[j]);
                if ti != BACK {
                    fv.push(vi);
                }
                if ti != FRONT {
                    bv.push(vi);
                }
                if (ti | tj) == SPANNING {
                    let denom = dot(plane.normal, sub(vj, vi));
                    let t = if denom.abs() < 1e-12 {
                        0.0
                    } else {
                        (plane.w - dot(plane.normal, vi)) / denom
                    };
                    let v = lerp(vi, vj, t);
                    fv.push(v);
                    bv.push(v);
                }
            }
            if fv.len() >= 3 {
                front.push(Poly::new(fv));
            }
            if bv.len() >= 3 {
                back.push(Poly::new(bv));
            }
        }
    }
}

#[derive(Default)]
struct Node {
    plane: Option<Plane>,
    front: Option<Box<Node>>,
    back: Option<Box<Node>>,
    polygons: Vec<Poly>,
}

impl Node {
    fn build(&mut self, polygons: Vec<Poly>) {
        if polygons.is_empty() {
            return;
        }
        if self.plane.is_none() {
            self.plane = Some(polygons[0].plane.clone());
        }
        let plane = self.plane.clone().unwrap();
        let mut front = Vec::new();
        let mut back = Vec::new();
        let mut cf = Vec::new();
        let mut cb = Vec::new();
        for p in &polygons {
            split_polygon(&plane, p, &mut cf, &mut cb, &mut front, &mut back);
        }
        // Both coplanar buckets stay on this node (csg.js Node.build).
        self.polygons.extend(cf);
        self.polygons.extend(cb);
        if !front.is_empty() {
            self.front.get_or_insert_with(Default::default).build(front);
        }
        if !back.is_empty() {
            self.back.get_or_insert_with(Default::default).build(back);
        }
    }

    fn invert(&mut self) {
        for p in &mut self.polygons {
            p.flip();
        }
        if let Some(pl) = &mut self.plane {
            pl.flip();
        }
        if let Some(f) = &mut self.front {
            f.invert();
        }
        if let Some(b) = &mut self.back {
            b.invert();
        }
        std::mem::swap(&mut self.front, &mut self.back);
    }

    fn clip_polygons(&self, polygons: Vec<Poly>) -> Vec<Poly> {
        let Some(plane) = &self.plane else {
            return polygons;
        };
        let mut front = Vec::new();
        let mut back = Vec::new();
        let mut cf = Vec::new();
        let mut cb = Vec::new();
        for p in &polygons {
            split_polygon(plane, p, &mut cf, &mut cb, &mut front, &mut back);
        }
        // Coplanar pieces follow csg.js: front-facing with front, back with back.
        front.extend(cf);
        back.extend(cb);
        let mut front = match &self.front {
            Some(n) => n.clip_polygons(front),
            None => front,
        };
        if let Some(n) = &self.back {
            front.extend(n.clip_polygons(back));
        }
        front
    }

    fn clip_to(&mut self, other: &Node) {
        self.polygons = other.clip_polygons(std::mem::take(&mut self.polygons));
        if let Some(f) = &mut self.front {
            f.clip_to(other);
        }
        if let Some(b) = &mut self.back {
            b.clip_to(other);
        }
    }

    fn all_polygons(&self) -> Vec<Poly> {
        let mut out = self.polygons.clone();
        if let Some(f) = &self.front {
            out.extend(f.all_polygons());
        }
        if let Some(b) = &self.back {
            out.extend(b.all_polygons());
        }
        out
    }
}

fn node_of(polys: Vec<Poly>) -> Node {
    let mut n = Node::default();
    n.build(polys);
    n
}

fn csg_union(a: Vec<Poly>, b: Vec<Poly>) -> Vec<Poly> {
    let mut a = node_of(a);
    let mut b = node_of(b);
    a.clip_to(&b);
    b.clip_to(&a);
    b.invert();
    b.clip_to(&a);
    b.invert();
    a.build(b.all_polygons());
    a.all_polygons()
}

fn csg_difference(a: Vec<Poly>, b: Vec<Poly>) -> Vec<Poly> {
    let mut a = node_of(a);
    let mut b = node_of(b);
    a.invert();
    a.clip_to(&b);
    b.clip_to(&a);
    b.invert();
    b.clip_to(&a);
    b.invert();
    a.build(b.all_polygons());
    a.invert();
    a.all_polygons()
}

fn csg_intersection(a: Vec<Poly>, b: Vec<Poly>) -> Vec<Poly> {
    let mut a = node_of(a);
    let mut b = node_of(b);
    a.invert();
    b.clip_to(&a);
    b.invert();
    a.clip_to(&b);
    b.clip_to(&a);
    a.build(b.all_polygons());
    a.invert();
    a.all_polygons()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn signed_volume(m: &Mesh) -> f64 {
        // Sum of signed tetrahedron volumes (origin apex) — |result| = volume.
        m.tris
            .iter()
            .map(|t| dot(t[0], cross(t[1], t[2])) / 6.0)
            .sum::<f64>()
            .abs()
    }

    #[test]
    fn box_volume_is_correct() {
        let b = box_solid([0.0, 0.0, 0.0], [2.0, 3.0, 4.0]);
        assert_eq!(b.tris.len(), 12);
        assert!((signed_volume(&b) - 24.0).abs() < 1e-6);
    }

    #[test]
    fn difference_removes_material() {
        // 4×4×4 cube minus a 2×2 column straight through it: volume 64 - 16 = 48.
        let cube = box_solid([0.0, 0.0, 0.0], [4.0, 4.0, 4.0]);
        let bore = box_solid([1.0, 1.0, -1.0], [2.0, 2.0, 6.0]);
        let holed = cube.difference(&bore);
        assert!(!holed.tris.is_empty());
        assert!(
            (signed_volume(&holed) - 48.0).abs() < 1e-4,
            "expected 48, got {}",
            signed_volume(&holed)
        );
    }

    #[test]
    fn union_merges_volume() {
        // Two disjoint unit cubes: total volume 2.
        let a = box_solid([0.0, 0.0, 0.0], [1.0, 1.0, 1.0]);
        let b = box_solid([5.0, 0.0, 0.0], [1.0, 1.0, 1.0]);
        assert!((signed_volume(&a.union(&b)) - 2.0).abs() < 1e-4);
    }

    #[test]
    fn cylinder_and_prism_build() {
        assert!(!cylinder_solid([0.0, 0.0, 0.0], 1.0, 2.0, 24)
            .tris
            .is_empty());
        let sq = [[0.0, 0.0], [2.0, 0.0], [2.0, 2.0], [0.0, 2.0]];
        assert!((signed_volume(&prism(&sq, 0.0, 3.0, true)) - 12.0).abs() < 1e-6);
    }
}
