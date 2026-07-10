//! Minimal glTF 2.0 exporter.
//!
//! Turns the scene's solid meshes (from [`crate::render3d::scene_meshes`]) into a
//! single self-contained `.gltf`: JSON with the binary buffer base64-embedded as
//! a data URI. Pure Rust, deterministic, no extra dependencies.
//!
//! cadspec is Z-up (height is +Z); glTF is Y-up and right-handed, so every
//! position and normal is remapped `(x, y, z) → (x, z, -y)` (a proper rotation).
//! Triangles are flat-shaded (a per-face normal on each of its three vertices)
//! and materials are double-sided, since boolean output can have mixed winding.

use crate::mesh::Mesh;

type V3 = [f64; 3];

fn sub(a: V3, b: V3) -> V3 {
    [a[0] - b[0], a[1] - b[1], a[2] - b[2]]
}
fn cross(a: V3, b: V3) -> V3 {
    [
        a[1] * b[2] - a[2] * b[1],
        a[2] * b[0] - a[0] * b[2],
        a[0] * b[1] - a[1] * b[0],
    ]
}
fn norm(a: V3) -> V3 {
    let l = (a[0] * a[0] + a[1] * a[1] + a[2] * a[2]).sqrt();
    if l < 1e-12 {
        [0.0, 0.0, 1.0]
    } else {
        [a[0] / l, a[1] / l, a[2] / l]
    }
}
/// Z-up (cadspec) → Y-up (glTF).
fn to_gltf(p: V3) -> [f32; 3] {
    [p[0] as f32, p[2] as f32, -p[1] as f32]
}

/// Parse `#rgb` / `#rrggbb` into linear-ish `[r, g, b, 1]` floats (0..1).
fn color_rgba(hex: &str) -> [f32; 4] {
    let h = hex.trim().trim_start_matches('#');
    let parse = |s: &str| u8::from_str_radix(s, 16).unwrap_or(160) as f32 / 255.0;
    match h.len() {
        3 => {
            let r = parse(&h[0..1].repeat(2));
            let g = parse(&h[1..2].repeat(2));
            let b = parse(&h[2..3].repeat(2));
            [r, g, b, 1.0]
        }
        6 => [parse(&h[0..2]), parse(&h[2..4]), parse(&h[4..6]), 1.0],
        _ => [0.63, 0.63, 0.63, 1.0],
    }
}

const B64: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";

fn base64(data: &[u8]) -> String {
    let mut out = String::with_capacity(data.len().div_ceil(3) * 4);
    for chunk in data.chunks(3) {
        let b = [
            chunk[0],
            *chunk.get(1).unwrap_or(&0),
            *chunk.get(2).unwrap_or(&0),
        ];
        let n = ((b[0] as u32) << 16) | ((b[1] as u32) << 8) | (b[2] as u32);
        out.push(B64[((n >> 18) & 63) as usize] as char);
        out.push(B64[((n >> 12) & 63) as usize] as char);
        out.push(if chunk.len() > 1 {
            B64[((n >> 6) & 63) as usize] as char
        } else {
            '='
        });
        out.push(if chunk.len() > 2 {
            B64[(n & 63) as usize] as char
        } else {
            '='
        });
    }
    out
}

/// Serialize the scene meshes to a self-contained glTF 2.0 document.
pub fn scene_to_gltf(meshes: &[(Mesh, String)]) -> String {
    // Binary buffer: per mesh, all vertex positions then all normals (FLOAT VEC3).
    let mut buf: Vec<u8> = Vec::new();
    // Per-mesh accessor metadata gathered while writing the buffer.
    struct Acc {
        pos_off: usize,
        nrm_off: usize,
        count: usize,
        min: [f32; 3],
        max: [f32; 3],
        color: [f32; 4],
    }
    let mut accs: Vec<Acc> = Vec::new();

    for (mesh, color) in meshes {
        if mesh.tris.is_empty() {
            continue;
        }
        let pos_off = buf.len();
        let mut min = [f32::MAX; 3];
        let mut max = [f32::MIN; 3];
        // Positions.
        for t in &mesh.tris {
            for v in t {
                let p = to_gltf(*v);
                for k in 0..3 {
                    min[k] = min[k].min(p[k]);
                    max[k] = max[k].max(p[k]);
                    buf.extend_from_slice(&p[k].to_le_bytes());
                }
            }
        }
        let nrm_off = buf.len();
        // Flat normals (same for the three vertices of a triangle).
        for t in &mesh.tris {
            let n = norm(cross(sub(t[1], t[0]), sub(t[2], t[0])));
            let ng = to_gltf(n);
            for _ in 0..3 {
                for c in ng {
                    buf.extend_from_slice(&c.to_le_bytes());
                }
            }
        }
        accs.push(Acc {
            pos_off,
            nrm_off,
            count: mesh.tris.len() * 3,
            min,
            max,
            color: color_rgba(color),
        });
    }

    // Assemble the JSON.
    let mut buffer_views = String::new();
    let mut accessors = String::new();
    let mut materials = String::new();
    let mut gltf_meshes = String::new();
    let mut nodes = String::new();
    let mut node_idx = String::new();

    for (i, a) in accs.iter().enumerate() {
        let bv_pos = i * 2;
        let bv_nrm = i * 2 + 1;
        let acc_pos = i * 2;
        let acc_nrm = i * 2 + 1;
        let bytes = a.count * 12;
        if i > 0 {
            buffer_views.push(',');
            accessors.push(',');
            materials.push(',');
            gltf_meshes.push(',');
            nodes.push(',');
            node_idx.push(',');
        }
        buffer_views.push_str(&format!(
            r#"{{"buffer":0,"byteOffset":{},"byteLength":{}}},{{"buffer":0,"byteOffset":{},"byteLength":{}}}"#,
            a.pos_off, bytes, a.nrm_off, bytes
        ));
        accessors.push_str(&format!(
            r#"{{"bufferView":{bv_pos},"componentType":5126,"count":{c},"type":"VEC3","min":[{mn0},{mn1},{mn2}],"max":[{mx0},{mx1},{mx2}]}},{{"bufferView":{bv_nrm},"componentType":5126,"count":{c},"type":"VEC3"}}"#,
            c = a.count,
            mn0 = a.min[0], mn1 = a.min[1], mn2 = a.min[2],
            mx0 = a.max[0], mx1 = a.max[1], mx2 = a.max[2],
        ));
        materials.push_str(&format!(
            r#"{{"pbrMetallicRoughness":{{"baseColorFactor":[{r},{g},{b},{al}],"metallicFactor":0.0,"roughnessFactor":0.85}},"doubleSided":true}}"#,
            r = a.color[0], g = a.color[1], b = a.color[2], al = a.color[3],
        ));
        gltf_meshes.push_str(&format!(
            r#"{{"primitives":[{{"attributes":{{"POSITION":{acc_pos},"NORMAL":{acc_nrm}}},"material":{i}}}]}}"#,
        ));
        nodes.push_str(&format!(r#"{{"mesh":{i}}}"#));
        node_idx.push_str(&i.to_string());
    }

    let b64 = base64(&buf);
    format!(
        r#"{{"asset":{{"version":"2.0","generator":"cadspec"}},"scene":0,"scenes":[{{"nodes":[{node_idx}]}}],"nodes":[{nodes}],"meshes":[{gltf_meshes}],"materials":[{materials}],"accessors":[{accessors}],"bufferViews":[{buffer_views}],"buffers":[{{"byteLength":{len},"uri":"data:application/octet-stream;base64,{b64}"}}]}}"#,
        len = buf.len(),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn base64_known_vectors() {
        assert_eq!(base64(b"Man"), "TWFu");
        assert_eq!(base64(b"Ma"), "TWE=");
        assert_eq!(base64(b"M"), "TQ==");
    }

    #[test]
    fn color_parses_hex() {
        assert_eq!(color_rgba("#ff0000"), [1.0, 0.0, 0.0, 1.0]);
        assert_eq!(color_rgba("#000"), [0.0, 0.0, 0.0, 1.0]);
    }

    #[test]
    fn single_triangle_exports_valid_gltf() {
        let mesh = Mesh {
            tris: vec![[[0.0, 0.0, 0.0], [1.0, 0.0, 0.0], [0.0, 1.0, 0.0]]],
        };
        let doc = scene_to_gltf(&[(mesh, "#6ec6e6".to_string())]);
        // One triangle = 3 vertices; positions + normals = 3 * 2 * 12 = 72 bytes.
        assert!(doc.contains(r#""byteLength":72"#));
        assert!(doc.contains(r#""version":"2.0""#));
        assert!(doc.contains(r#""count":3"#));
        assert!(doc.contains(r#""POSITION""#) && doc.contains(r#""NORMAL""#));
        // Z-up→Y-up: cad y=1 becomes gltf z=-1, so the POSITION min z is -1.
        assert!(doc.contains(r#""min":[0,0,-1]"#));
    }

    #[test]
    fn empty_scene_is_still_valid_json() {
        let doc = scene_to_gltf(&[]);
        assert!(doc.contains(r#""byteLength":0"#));
        assert!(doc.contains(r#""meshes":[]"#));
    }
}
