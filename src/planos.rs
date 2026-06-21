//! Planos (drawing sheets) — render a named view of the model onto a sheet
//! with a title block (rótulo).
//!
//! A "plano" is the CAD notion of a sheet/viewport: it reuses the one model and
//! presents it through a chosen view (plan, iso, the four elevations, or a
//! section cut), framed at a paper size with a title block. The title block is
//! either auto-generated from the project metadata or drawn by the user in a
//! `.cf` file referenced via `rotulo` — same primitives as everything else.

use crate::parser::{parse_cf, Plano};
use crate::render3d::{render_view, Camera, Cut};
use crate::svg::{load_project_layers, render_scene_from};
use anyhow::Result;
use std::fmt::Write as _;
use std::path::Path;

const INNER_W: u32 = 1400;
const BG: &str = "#0d0d0d";
const FRAME: &str = "#9a9a9a";
const INK: &str = "#e0e0e0";
const MUTED: &str = "#9a9a9a";

/// A rendered sheet SVG plus its pixel size.
pub struct Sheet {
    pub svg: String,
    pub width_px: f64,
    pub height_px: f64,
}

/// Render a single plano to a framed sheet SVG.
pub fn render_plano(project_dir: &Path, plano: &Plano, target_width: u32) -> Result<Sheet> {
    let (project, layers) = load_project_layers(project_dir, None)?;

    // 1. Render the model through the requested view.
    let (inner_svg, iw, ih) = match plano.view.as_str() {
        "plan" => {
            let s = render_scene_from(
                &project.project.name,
                &project.project.units,
                &layers,
                INNER_W,
                &[],
            );
            (s.svg, s.width_px, s.height_px)
        }
        "section" => {
            let axis = match plano.cut_axis.as_deref() {
                Some("x") => 0,
                Some("y") => 1,
                _ => 2, // default: horizontal cut (z)
            };
            let cam = match axis {
                0 => Camera::right_side(),
                1 => Camera::front(),
                _ => Camera::top(),
            };
            let cut = Cut {
                axis,
                at: plano.cut_at.unwrap_or(0.0),
                keep_max: plano.keep.as_deref() == Some("max"),
            };
            let r = render_view(&layers, INNER_W, &cam, Some(&cut));
            (r.svg, r.width_px, r.height_px)
        }
        other => {
            let cam = match other {
                "front" => Camera::front(),
                "back" => Camera::back(),
                "left" => Camera::left_side(),
                "right" => Camera::right_side(),
                "top" => Camera::top(),
                _ => Camera::iso(),
            };
            let r = render_view(&layers, INNER_W, &cam, None);
            (r.svg, r.width_px, r.height_px)
        }
    };

    // 2. Sheet geometry (paper size in mm → pixels).
    let [mm_w, mm_h] = plano.size.unwrap_or([420.0, 297.0]); // A3 landscape
    let sheet_w = target_width as f64;
    let sheet_h = sheet_w * (mm_h / mm_w).max(0.1);
    let margin = sheet_w * 0.02;

    // Title block: bottom-right box.
    let tb_w = (sheet_w * 0.34).min(sheet_w - 2.0 * margin);
    let tb_h = (sheet_h * 0.16).min(sheet_h - 2.0 * margin);
    let tb_x = sheet_w - margin - tb_w;
    let tb_y = sheet_h - margin - tb_h;

    // Drawing area: inside the frame, above the title block.
    let draw_x = margin + margin;
    let draw_y = margin + margin;
    let draw_w = sheet_w - 2.0 * draw_x;
    let draw_h = (tb_y - draw_y - margin).max(1.0);

    // 3. Compose the sheet SVG.
    let mut out = String::with_capacity(inner_svg.len() + 4096);
    let _ = write!(
        out,
        r#"<svg xmlns="http://www.w3.org/2000/svg" width="{w:.0}" height="{h:.0}" viewBox="0 0 {w:.0} {h:.0}" data-view="plano">"#,
        w = sheet_w,
        h = sheet_h,
    );
    let _ = write!(out, r#"<rect width="100%" height="100%" fill="{BG}"/>"#);
    // Outer frame border.
    let _ = write!(
        out,
        r#"<rect x="{x:.1}" y="{x:.1}" width="{w:.1}" height="{h:.1}" fill="none" stroke="{FRAME}" stroke-width="2"/>"#,
        x = margin,
        w = sheet_w - 2.0 * margin,
        h = sheet_h - 2.0 * margin,
    );

    // The model view, fitted into the drawing area (nested SVG keeps aspect).
    let _ = write!(
        out,
        r#"<svg x="{dx:.1}" y="{dy:.1}" width="{dw:.1}" height="{dh:.1}" viewBox="0 0 {iw:.0} {ih:.0}" preserveAspectRatio="xMidYMid meet">{body}</svg>"#,
        dx = draw_x,
        dy = draw_y,
        dw = draw_w,
        dh = draw_h,
        body = svg_inner(&inner_svg),
    );

    // 4. Title block (rótulo): custom .cf or generated default.
    out.push_str(&title_block(
        project_dir,
        plano,
        &project.project.name,
        &project.project.units,
        project.project.scale.as_str(),
        tb_x,
        tb_y,
        tb_w,
        tb_h,
    ));

    out.push_str("</svg>");
    Ok(Sheet {
        svg: out,
        width_px: sheet_w,
        height_px: sheet_h,
    })
}

/// The content between an SVG's outer `<svg …>` and `</svg>` tags.
fn svg_inner(svg: &str) -> &str {
    let start = svg.find('>').map(|i| i + 1).unwrap_or(0);
    let end = svg.rfind("</svg>").unwrap_or(svg.len());
    if start <= end {
        &svg[start..end]
    } else {
        ""
    }
}

#[allow(clippy::too_many_arguments)]
fn title_block(
    project_dir: &Path,
    plano: &Plano,
    project_name: &str,
    units: &str,
    project_scale: &str,
    x: f64,
    y: f64,
    w: f64,
    h: f64,
) -> String {
    // Custom rótulo: render the referenced .cf, fitted into the title block.
    if let Some(rotulo) = &plano.rotulo {
        if let Ok(cf) = parse_cf(&project_dir.join(rotulo)) {
            let scene = render_scene_from(
                project_name,
                units,
                &[("rotulo".to_string(), cf)],
                INNER_W,
                &[],
            );
            return format!(
                r#"<rect x="{x:.1}" y="{y:.1}" width="{w:.1}" height="{h:.1}" fill="{BG}" stroke="{FRAME}" stroke-width="2"/>"#,
            ) + &format!(
                r#"<svg x="{x:.1}" y="{y:.1}" width="{w:.1}" height="{h:.1}" viewBox="0 0 {iw:.0} {ih:.0}" preserveAspectRatio="xMidYMid meet">{body}</svg>"#,
                iw = scene.width_px,
                ih = scene.height_px,
                body = svg_inner(&scene.svg),
            );
        }
    }

    // Default rótulo.
    let title = plano.title.as_deref().unwrap_or(&plano.name);
    let scale = plano.scale.as_deref().unwrap_or(project_scale);
    let pad = w * 0.04;
    let row = h / 4.0;
    let mut s = String::new();
    let _ = write!(
        s,
        r#"<rect x="{x:.1}" y="{y:.1}" width="{w:.1}" height="{h:.1}" fill="{BG}" stroke="{FRAME}" stroke-width="2"/>"#,
    );
    // Divider under the title.
    let _ = write!(
        s,
        r#"<line x1="{x:.1}" y1="{ly:.1}" x2="{x2:.1}" y2="{ly:.1}" stroke="{FRAME}" stroke-width="1"/>"#,
        ly = y + row * 1.6,
        x2 = x + w,
    );
    let fs_title = (row * 0.62).max(8.0);
    let fs = (row * 0.40).max(7.0);
    let _ = write!(
        s,
        r#"<text x="{tx:.1}" y="{ty:.1}" font-family="monospace" font-size="{fs_title:.1}" fill="{INK}">{title}</text>"#,
        tx = x + pad,
        ty = y + row * 1.05,
        title = xml_escape(title),
    );
    let _ = write!(
        s,
        r#"<text x="{tx:.1}" y="{ty:.1}" font-family="monospace" font-size="{fs:.1}" fill="{MUTED}">{n}</text>"#,
        tx = x + pad,
        ty = y + row * 2.35,
        n = xml_escape(project_name),
    );
    let _ = write!(
        s,
        r#"<text x="{tx:.1}" y="{ty:.1}" font-family="monospace" font-size="{fs:.1}" fill="{MUTED}">Escala {scale} · {units} · {view}</text>"#,
        tx = x + pad,
        ty = y + row * 3.25,
        view = xml_escape(&plano.view),
        scale = xml_escape(scale),
        units = xml_escape(units),
    );
    let _ = write!(
        s,
        r#"<text x="{tx:.1}" y="{ty:.1}" font-family="monospace" font-size="{fs:.1}" fill="{MUTED}" text-anchor="end">cadspec</text>"#,
        tx = x + w - pad,
        ty = y + h - row * 0.35,
    );
    s
}

fn xml_escape(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::parser::Plano;

    fn plano(view: &str) -> Plano {
        Plano {
            name: "P-01".into(),
            view: view.into(),
            size: Some([420.0, 297.0]),
            scale: Some("1:50".into()),
            title: Some("Planta baja".into()),
            rotulo: None,
            cut_axis: None,
            cut_at: None,
            keep: None,
        }
    }

    #[test]
    fn renders_plan_sheet_with_title_block() {
        let dir = Path::new("examples/vivienda");
        let s = render_plano(dir, &plano("plan"), 1200).unwrap();
        assert!(s.svg.contains(r#"data-view="plano""#));
        assert!(s.svg.contains("Planta baja"));
        assert!(s.svg.contains("Escala 1:50"));
        // The model view is nested inside the sheet.
        assert!(s.svg.matches("<svg").count() >= 2);
        assert!((s.width_px - 1200.0).abs() < 1.0 && s.height_px > 0.0);
    }

    #[test]
    fn renders_section_sheet() {
        let dir = Path::new("examples/vivienda");
        let mut p = plano("section");
        p.cut_axis = Some("z".into());
        p.cut_at = Some(1.0);
        let s = render_plano(dir, &p, 1000).unwrap();
        assert!(s.svg.contains(r#"data-view="plano""#));
    }
}
