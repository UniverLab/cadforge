//! Integration tests — full pipeline from .cf files to DXF output.

use cadspec::compiler::compile_project;
use cadspec::importer::import_dxf;
use std::fs;
use std::path::Path;

#[test]
fn compile_example_project_produces_valid_dxf() {
    let project_dir = Path::new("examples/vivienda");
    let output = Path::new("/tmp/cadspec_compile_test_output.dxf");

    // Remove previous output if exists
    let _ = fs::remove_file(output);

    compile_project(project_dir, None, Some(output)).unwrap();

    assert!(output.exists(), "output.dxf should be created");

    let content = fs::read_to_string(output).unwrap();

    // Verify DXF structure
    assert!(content.contains("SECTION"));
    assert!(content.contains("HEADER"));
    assert!(content.contains("ENTITIES"));
    assert!(content.contains("EOF"));

    // Verify version
    assert!(content.contains("AC1018"));

    // Verify layers are present
    assert!(content.contains("muros"));
    assert!(content.contains("puertas"));
    assert!(content.contains("mobiliario"));
    assert!(content.contains("cotas"));
    assert!(content.contains("achurados"));

    // Verify entity types exist
    assert!(content.contains("LWPOLYLINE"));
    assert!(content.contains("LINE"));
    assert!(content.contains("ARC"));
    assert!(content.contains("CIRCLE"));
    assert!(content.contains("TEXT"));

    // Verify line types are registered
    assert!(content.contains("DASHED"));
    assert!(content.contains("DASHDOT"));
}

#[test]
fn compile_project_fails_on_missing_project_toml() {
    let result = compile_project(Path::new("/tmp/nonexistent_cadspec_dir"), None, None);
    assert!(result.is_err());
}

#[test]
fn check_project_validates_without_generating_dxf() {
    use cadspec::compiler::check_project;

    let project_dir = Path::new("examples/vivienda");
    let output = project_dir.join("check_should_not_exist.dxf");
    let _ = std::fs::remove_file(&output);

    let count = check_project(project_dir).unwrap();
    assert!(count > 0, "project should have entities");
    assert!(!output.exists());
}

#[test]
fn parser_handles_all_primitives() {
    let toml = r##"
[layer]
name = "test"

[[line]]
from = [0.0, 0.0]
to = [1.0, 1.0]

[[polyline]]
points = [[0.0, 0.0], [1.0, 0.0], [1.0, 1.0]]
closed = true
weight = 0.35

[[rect]]
origin = [0.0, 0.0]
width = 2.0
height = 3.0

[[circle]]
center = [5.0, 5.0]
radius = 1.0

[[arc]]
center = [0.0, 0.0]
radius = 2.0
from_angle = 0.0
to_angle = 180.0

[[text]]
position = [1.0, 1.0]
content = "Hello"

[[point]]
position = [3.0, 3.0]

[[dim]]
type = "linear"
from = [0.0, 0.0]
to = [8.5, 0.0]
offset = 0.5
"##;

    let cf: cadspec::model::CfFile = toml::from_str(toml).unwrap();
    assert_eq!(cf.lines.len(), 1);
    assert_eq!(cf.polylines.len(), 1);
    assert!(cf.polylines[0].common.weight.is_some());
    assert_eq!(cf.rects.len(), 1);
    assert_eq!(cf.circles.len(), 1);
    assert_eq!(cf.arcs.len(), 1);
    assert_eq!(cf.texts.len(), 1);
    assert_eq!(cf.points.len(), 1);
    assert_eq!(cf.dims.len(), 1);
}

#[test]
fn line_styles_parsed_and_compiled() {
    let toml = r##"
[[line]]
from = [0.0, 0.0]
to = [10.0, 0.0]
style = "dashed"

[[line]]
from = [0.0, 1.0]
to = [10.0, 1.0]
style = "dotted"

[[line]]
from = [0.0, 2.0]
to = [10.0, 2.0]
style = "dashdot"
"##;

    let cf: cadspec::model::CfFile = toml::from_str(toml).unwrap();
    assert_eq!(cf.lines.len(), 3);
    assert!(cf.lines[0].common.style.is_some());
}

#[test]
fn hatch_resolves_boundary_from_polyline() {
    let toml = r##"
[[polyline]]
id = "pl-room"
points = [[0.0, 0.0], [4.0, 0.0], [4.0, 3.0], [0.0, 3.0]]
closed = true

[[hatch]]
boundary = "pl-room"
pattern = "ansi31"
scale = 1.0
angle = 45.0
"##;

    let cf: cadspec::model::CfFile = toml::from_str(toml).unwrap();
    assert_eq!(cf.hatches.len(), 1);
    assert_eq!(cf.hatches[0].boundary.as_deref(), Some("pl-room"));

    // Compile it to verify no panic
    use cadspec::dxf_writer::DxfWriter;
    let mut writer = DxfWriter::new();
    writer.add_layer("test", 7);
    cadspec::compiler::compile_cf_public(&mut writer, &cf, "test");
}

#[test]
fn solid_fill_generates_triangles() {
    let toml = r##"
[[fill]]
id = "fl-room"
points = [[0.0, 0.0], [4.0, 0.0], [4.0, 3.0], [0.0, 3.0]]
color = "#808080"
"##;

    let cf: cadspec::model::CfFile = toml::from_str(toml).unwrap();
    assert_eq!(cf.fills.len(), 1);

    // Compile and verify it produces a DXF with SOLID entities
    use cadspec::dxf_writer::DxfWriter;
    let mut writer = DxfWriter::new();
    writer.add_layer("test", 7);
    cadspec::compiler::compile_cf_public(&mut writer, &cf, "test");

    let path = std::path::PathBuf::from("/tmp/cadspec_test_fill.dxf");
    writer.save(&path).unwrap();
    let content = fs::read_to_string(&path).unwrap();
    assert!(content.contains("SOLID"));
}

fn write_constraints_fixture(base: &Path, strict: bool) {
    fs::create_dir_all(base).unwrap();
    fs::write(
        base.join("project.toml"),
        format!(
            r#"[project]
name = "constraints-fixture"
scale = "1:100"
units = "m"
strict = {strict}

[layers]
parent = {{ file = "parent.cf", locked = false }}
child = {{ file = "child.cf", locked = false }}

[constraints]
child.parent = "parent"
child.belongs_to = "parent"
"#
        ),
    )
    .unwrap();

    fs::write(
        base.join("parent.cf"),
        r#"[layer]
name = "parent"

[[rect]]
id = "room-1"
origin = [0.0, 0.0]
width = 2.0
height = 2.0
"#,
    )
    .unwrap();

    fs::write(
        base.join("child.cf"),
        r#"[layer]
name = "child"

[[rect]]
id = "furn-1"
origin = [3.0, 3.0]
width = 1.0
height = 1.0
belongs_to = "room-1"
"#,
    )
    .unwrap();
}

#[test]
fn compile_allows_constraint_warnings_when_not_strict() {
    let dir = Path::new("/tmp/cadspec_constraints_non_strict");
    let _ = fs::remove_dir_all(dir);
    write_constraints_fixture(dir, false);

    let output = dir.join("output.dxf");
    let _ = fs::remove_file(&output);

    compile_project(dir, None, None).unwrap();
    assert!(
        output.exists(),
        "output.dxf should be created in non-strict mode"
    );

    let _ = fs::remove_dir_all(dir);
}

#[test]
fn compile_fails_on_constraint_violation_when_strict() {
    let dir = Path::new("/tmp/cadspec_constraints_strict");
    let _ = fs::remove_dir_all(dir);
    write_constraints_fixture(dir, true);

    let result = compile_project(dir, None, None);
    assert!(
        result.is_err(),
        "strict mode should fail on constraint violation"
    );

    let _ = fs::remove_dir_all(dir);
}

#[test]
fn render_svg_on_example_project() {
    let svg = cadspec::svg::render_svg(Path::new("examples/vivienda"), None, 1600).unwrap();
    assert!(svg.starts_with("<svg"));
    assert!(svg.ends_with("</svg>"));
    // Layer groups for every project layer
    for layer in ["muros", "puertas", "mobiliario", "cotas", "achurados"] {
        assert!(
            svg.contains(&format!(r#"data-layer="{}""#, layer)),
            "missing layer group {}",
            layer
        );
    }
    // Dimensions are labeled with the measured value in project units
    assert!(svg.contains(" m</text>"), "dim labels should include units");
}

#[test]
fn project_report_is_serializable_and_complete() {
    let report = cadspec::compiler::project_report(Path::new("examples/vivienda")).unwrap();
    assert!(report.total_entities > 0);
    assert_eq!(report.layers.len(), 5);
    assert!(report.layers.iter().all(|l| !l.missing));

    let json = serde_json::to_string(&report).unwrap();
    assert!(json.contains("\"total_entities\""));
    assert!(json.contains("\"issues\""));
}

#[test]
fn taller_example_expands_arrays_and_mirrors() {
    let dir = Path::new("examples/taller");

    // Expanded entity counts: 16 treads + 16 teeth + mirrored geometry
    let report = cadspec::compiler::project_report(dir).unwrap();
    assert_eq!(report.total_entities, 49);

    let svg = cadspec::svg::render_svg(dir, None, 1200).unwrap();
    // 15 generated tread copies with derived ids
    let tread_copies = svg.matches(r#"data-id="pl-huella@"#).count();
    assert_eq!(tread_copies, 15);
    // Mirrored door arc exists
    assert!(svg.contains(r#"data-id="ar-puerta@m""#));
    // Styled dims: 1 decimal with units, 3 decimals without units
    assert!(svg.contains("3.2 m"));
    assert!(svg.contains(">2.900<"));

    // The DXF compiles with the expanded geometry
    let out = Path::new("/tmp/cadspec_taller.dxf");
    let _ = fs::remove_file(out);
    compile_project(dir, None, Some(out)).unwrap();
    assert!(out.exists());
    let _ = fs::remove_file(out);
}

#[test]
fn preview_renders_faithful_png_with_metadata_and_highlights() {
    let dir = Path::new("/tmp/cadspec_preview_test");
    let _ = fs::remove_dir_all(dir);
    fs::create_dir_all(dir).unwrap();
    fs::write(
        dir.join("project.toml"),
        r#"[project]
name = "preview-fixture"
units = "m"

[layers]
plano = { file = "plano.cf", locked = false }
"#,
    )
    .unwrap();
    fs::write(
        dir.join("plano.cf"),
        r##"[[rect]]
id = "rc-room"
origin = [0.0, 0.0]
width = 6.0
height = 4.0

[[text]]
id = "tx-label"
position = [3.0, 2.0]
content = "SALA"
size = 0.4
align = "center"

[[dim]]
id = "dm-width"
from = [0.0, 0.0]
to = [6.0, 0.0]
offset = -0.6
"##,
    )
    .unwrap();

    cadspec::preview::generate_preview(
        dir,
        800,
        800,
        None,
        &["rc-room".to_string()],
        cadspec::preview::PreviewOutputs {
            png: true,
            svg: true,
        },
        cadspec::preview::PreviewView::Plan,
    )
    .unwrap();

    assert!(dir.join("preview.png").exists());
    assert!(dir.join("preview.svg").exists());
    let meta = fs::read_to_string(dir.join("preview.meta.json")).unwrap();
    assert!(meta.contains(r#""content": "SALA""#));
    assert!(meta.contains(r#""entity_type": "dim""#));
    assert!(meta.contains(r#""highlighted""#));
    assert!(meta.contains("rc-room"));

    // The PNG must fit within the requested box and be non-trivial
    let png = fs::read(dir.join("preview.png")).unwrap();
    assert!(png.len() > 1000, "PNG should contain rendered content");

    let _ = fs::remove_dir_all(dir);
}

#[test]
fn import_generated_dxf_creates_cadspec_project() {
    let source = Path::new("examples/vivienda");
    let source_output = source.join("output.dxf");
    let _ = fs::remove_file(&source_output);
    compile_project(source, None, Some(&source_output)).unwrap();

    let imported = Path::new("/tmp/cadspec_import_test");
    let _ = fs::remove_dir_all(imported);
    import_dxf(&source_output, imported, None).unwrap();

    assert!(imported.join("project.toml").exists());
    let project_toml = fs::read_to_string(imported.join("project.toml")).unwrap();
    assert!(project_toml.contains("[layers]"));
    assert!(project_toml.contains("muros"));

    compile_project(imported, None, None).unwrap();
    assert!(imported.join("output.dxf").exists());

    let _ = fs::remove_dir_all(imported);
}

#[test]
fn import_roundtrip_recovers_dims_styles_and_colors() {
    let dir = Path::new("/tmp/cadspec_roundtrip_fidelity");
    let _ = fs::remove_dir_all(dir);
    fs::create_dir_all(dir).unwrap();
    fs::write(
        dir.join("project.toml"),
        "[project]\nname = \"rt\"\nunits = \"m\"\n\n[layers]\nplano = { file = \"plano.cf\", locked = false }\n",
    )
    .unwrap();
    fs::write(
        dir.join("plano.cf"),
        r##"[layer]
name = "plano"
color = "#FF4444"

[[line]]
id = "ln-base"
from = [0.0, 0.0]
to = [8.0, 0.0]
color = "#FF5050"
weight = 0.5
style = "dashed"

[[text]]
id = "tx-sala"
position = [4.0, 3.0]
content = "SALA"
size = 0.25

[[dim]]
id = "dm-h"
from = [0.0, 0.0]
to = [8.0, 0.0]
offset = -0.8

[[dim]]
id = "dm-v"
from = [0.0, 0.0]
to = [0.0, 6.0]
offset = -1.2
"##,
    )
    .unwrap();

    let dxf_path = dir.join("output.dxf");
    compile_project(dir, None, Some(&dxf_path)).unwrap();

    // Each layer appears exactly once in the DXF LAYER table ('0' + 'plano'),
    // and 'plano' carries its ACI color instead of the default white.
    let dxf_text = fs::read_to_string(&dxf_path).unwrap();
    assert_eq!(
        dxf_text.matches("AcDbLayerTableRecord").count(),
        2,
        "duplicate LAYER table records"
    );

    let imported = Path::new("/tmp/cadspec_roundtrip_fidelity_out");
    let _ = fs::remove_dir_all(imported);
    import_dxf(&dxf_path, imported, None).unwrap();
    let cf = fs::read_to_string(imported.join("plano.cf")).unwrap();

    // Layer color survives via the DXF layer table (ACI): #FF4444 is not
    // itself an ACI color, so it comes back as the nearest ACI palette
    // entry (#FF3F00, ACI 20).
    assert!(
        cf.contains("color = \"#FF3F00\""),
        "layer color lost:\n{cf}"
    );
    // Entity style survives via true color, lineweight and line type
    assert!(
        cf.contains("color = \"#FF5050\""),
        "entity color lost:\n{cf}"
    );
    assert!(cf.contains("weight = 0.5"), "entity weight lost:\n{cf}");
    assert!(cf.contains("style = \"dashed\""), "line style lost:\n{cf}");
    // Both dims come back as dims with their perpendicular offsets intact
    assert_eq!(cf.matches("[[dim]]").count(), 2, "dims lost:\n{cf}");
    assert!(cf.contains("offset = -0.8000"), "horizontal offset:\n{cf}");
    assert!(cf.contains("offset = -1.2000"), "vertical offset:\n{cf}");
    // Companion graphics (3 lines + 1 label per dim) are deduplicated
    assert_eq!(
        cf.matches("[[line]]").count(),
        1,
        "dim companion lines not deduped:\n{cf}"
    );
    assert_eq!(
        cf.matches("[[text]]").count(),
        1,
        "dim label texts not deduped:\n{cf}"
    );

    // The reimported project must still compile
    compile_project(imported, None, None).unwrap();

    let _ = fs::remove_dir_all(dir);
    let _ = fs::remove_dir_all(imported);
}

#[test]
fn import_refuses_hatch_lines_into_single_hatch() {
    // A hatch expands into many DXF pattern lines. On import they must collapse
    // back into one `[[hatch]]` (with its region + pattern/angle/scale), while a
    // genuine standalone line on the same layer survives as a `[[line]]`.
    let dir = Path::new("/tmp/cadspec_hatch_roundtrip");
    let _ = fs::remove_dir_all(dir);
    fs::create_dir_all(dir).unwrap();
    fs::write(
        dir.join("project.toml"),
        "[project]\nname = \"ht\"\nunits = \"m\"\n\n[layers]\nzona = { file = \"zona.cf\", locked = false }\n",
    )
    .unwrap();
    fs::write(
        dir.join("zona.cf"),
        r##"[layer]
name = "zona"
color = "#C0C0C0"

[[polyline]]
id = "pl-room"
points = [[0.0, 0.0], [4.0, 0.0], [4.0, 3.0], [0.0, 3.0]]
closed = true

[[hatch]]
id = "ht-room"
boundary = "pl-room"
pattern = "ansi31"
scale = 2.0
angle = 45.0

[[line]]
id = "ln-real"
from = [0.0, 0.0]
to = [4.0, 3.0]
"##,
    )
    .unwrap();

    let dxf_path = dir.join("output.dxf");
    compile_project(dir, None, Some(&dxf_path)).unwrap();
    // The hatch really did expand into several tagged pattern lines in the DXF.
    let dxf_text = fs::read_to_string(&dxf_path).unwrap();
    assert!(
        dxf_text.matches("CADSPEC_HATCH").count() > 3,
        "expected the hatch to expand into multiple tagged pattern lines"
    );

    let imported = Path::new("/tmp/cadspec_hatch_roundtrip_out");
    let _ = fs::remove_dir_all(imported);
    import_dxf(&dxf_path, imported, None).unwrap();
    let cf = fs::read_to_string(imported.join("zona.cf")).unwrap();

    // Exactly one hatch comes back, carrying its region and source parameters.
    assert_eq!(
        cf.matches("[[hatch]]").count(),
        1,
        "hatch not re-fused:\n{cf}"
    );
    assert!(cf.contains("pattern = \"ansi31\""), "pattern lost:\n{cf}");
    assert!(cf.contains("angle = 45.0000"), "angle lost:\n{cf}");
    assert!(cf.contains("scale = 2.0000"), "scale lost:\n{cf}");
    // The genuine line survives; hatch pattern lines are not emitted as lines.
    assert_eq!(
        cf.matches("[[line]]").count(),
        1,
        "hatch pattern lines leaked as lines:\n{cf}"
    );

    // The reimported hatch (now using inline points) still compiles and expands
    // back into the same tagged pattern lines — a stable round-trip.
    let dxf2 = imported.join("output2.dxf");
    compile_project(imported, None, Some(&dxf2)).unwrap();
    let dxf2_text = fs::read_to_string(&dxf2).unwrap();
    assert_eq!(
        dxf_text.matches("CADSPEC_HATCH").count(),
        dxf2_text.matches("CADSPEC_HATCH").count(),
        "hatch pattern-line count drifted across a round-trip"
    );

    let _ = fs::remove_dir_all(dir);
    let _ = fs::remove_dir_all(imported);
}

#[test]
fn preview_warns_on_unresolved_hatch_and_fill_boundary() {
    let dir = Path::new("/tmp/cadspec_preview_unresolved_boundary");
    let _ = fs::remove_dir_all(dir);
    fs::create_dir_all(dir).unwrap();

    fs::write(
        dir.join("project.toml"),
        r#"[project]
name = "boundary-fixture"
scale = "1:100"
units = "m"

[layers]
main = { file = "main.cf" }
"#,
    )
    .unwrap();

    fs::write(
        dir.join("main.cf"),
        r##"[layer]
name = "main"

[[hatch]]
id = "ht-missing"
boundary = "does-not-exist"
pattern = "ansi31"
scale = 1.0
angle = 45.0

[[fill]]
id = "fl-missing"
boundary = "also-missing"
"##,
    )
    .unwrap();

    let output = std::process::Command::new(env!("CARGO_BIN_EXE_cadspec"))
        .args(["preview", "-p"])
        .arg(dir)
        .args(["--format", "svg"])
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "preview should not fail on an unresolved boundary, it should warn and skip the region"
    );

    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("hatch 'ht-missing'") && stderr.contains("does-not-exist"),
        "expected a warning for the unresolved hatch boundary, got:\n{stderr}"
    );
    assert!(
        stderr.contains("fill 'fl-missing'") && stderr.contains("also-missing"),
        "expected a warning for the unresolved fill boundary, got:\n{stderr}"
    );

    let _ = fs::remove_dir_all(dir);
}

/// Copy a project's source files (project.toml + *.cf) into `dest`, skipping
/// generated/gitignored artifacts like output.dxf or preview.*.
fn copy_project_sources(src: &Path, dest: &Path) {
    fs::create_dir_all(dest).unwrap();
    for entry in fs::read_dir(src).unwrap() {
        let entry = entry.unwrap();
        let path = entry.path();
        let name = entry.file_name();
        let is_source = name == "project.toml" || path.extension().is_some_and(|e| e == "cf");
        if path.is_file() && is_source {
            fs::copy(&path, dest.join(&name)).unwrap();
        }
    }
}

#[test]
fn fmt_is_idempotent_on_a_real_project() {
    use cadspec::fmt::format_project;

    let dir = Path::new("/tmp/cadspec_fmt_real_project");
    let _ = fs::remove_dir_all(dir);
    copy_project_sources(Path::new("examples/vivienda"), dir);

    // First pass may or may not need changes; either way it must succeed.
    format_project(dir, false).unwrap();
    let after_first: Vec<(std::path::PathBuf, String)> = fs::read_dir(dir)
        .unwrap()
        .map(|e| e.unwrap().path())
        .filter(|p| p.extension().is_some_and(|e| e == "cf") || p.ends_with("project.toml"))
        .map(|p| {
            let content = fs::read_to_string(&p).unwrap();
            (p, content)
        })
        .collect();

    // A formatted project must still compile to a valid DXF.
    let dxf_path = dir.join("output.dxf");
    compile_project(dir, None, Some(&dxf_path)).unwrap();
    assert!(dxf_path.exists());

    // `fmt --check` on an already-formatted project reports nothing to do.
    format_project(dir, true).unwrap();

    // A second `fmt` pass must be a no-op (idempotent) on every file.
    format_project(dir, false).unwrap();
    for (path, before) in after_first {
        let after = fs::read_to_string(&path).unwrap();
        assert_eq!(
            before,
            after,
            "fmt was not idempotent on {}",
            path.display()
        );
    }

    let _ = fs::remove_dir_all(dir);
}

#[test]
fn generate_plano_renders_a_declared_sheet_end_to_end() {
    use cadspec::preview::{generate_plano, PreviewOutputs};

    let dir = Path::new("/tmp/cadspec_generate_plano_e2e");
    let _ = fs::remove_dir_all(dir);
    copy_project_sources(Path::new("examples/vivienda"), dir);

    // examples/vivienda has no [[plano]] declared; append one so generate_plano
    // (the CLI-facing `preview --plano` path) has a sheet to look up by name.
    let mut project_toml = fs::read_to_string(dir.join("project.toml")).unwrap();
    project_toml.push_str(
        r#"
[[plano]]
name = "P-01"
view = "plan"
size = [420.0, 297.0]
scale = "1:50"
title = "Planta baja"
"#,
    );
    fs::write(dir.join("project.toml"), project_toml).unwrap();

    generate_plano(
        dir,
        "P-01",
        1200,
        900,
        PreviewOutputs {
            png: true,
            svg: true,
        },
    )
    .unwrap();

    assert!(dir.join("preview.png").exists());
    let svg = fs::read_to_string(dir.join("preview.svg")).unwrap();
    assert!(svg.contains(r#"data-view="plano""#));
    assert!(svg.contains("Planta baja"));

    // An unknown plano name is a clear error, not a panic.
    let err = generate_plano(
        dir,
        "does-not-exist",
        1200,
        900,
        PreviewOutputs {
            png: true,
            svg: true,
        },
    )
    .unwrap_err();
    assert!(
        err.to_string().contains("P-01"),
        "error should list the available plano names: {err}"
    );

    let _ = fs::remove_dir_all(dir);
}

/// An OS-assigned free port: bind to port 0, read back what the OS picked,
/// then drop the listener so `serve_project` can bind it itself.
fn free_local_port() -> u16 {
    std::net::TcpListener::bind(("127.0.0.1", 0))
        .unwrap()
        .local_addr()
        .unwrap()
        .port()
}

fn wait_for_port(port: u16, timeout: std::time::Duration) -> bool {
    let addr = std::net::SocketAddr::from(([127, 0, 0, 1], port));
    let deadline = std::time::Instant::now() + timeout;
    while std::time::Instant::now() < deadline {
        if std::net::TcpStream::connect_timeout(&addr, std::time::Duration::from_millis(200))
            .is_ok()
        {
            return true;
        }
        std::thread::sleep(std::time::Duration::from_millis(50));
    }
    false
}

/// GET `path` from the local `serve` instance and return (status, body).
/// Sends `Connection: close` so the server closes the socket once done and we
/// can just read to EOF instead of tracking Content-Length ourselves.
fn http_get(port: u16, path: &str) -> (u16, String) {
    use std::io::{Read, Write};
    let mut stream = std::net::TcpStream::connect(("127.0.0.1", port)).unwrap();
    stream
        .write_all(
            format!("GET {path} HTTP/1.1\r\nHost: localhost\r\nConnection: close\r\n\r\n")
                .as_bytes(),
        )
        .unwrap();
    stream.shutdown(std::net::Shutdown::Write).ok();
    let mut raw = String::new();
    stream.read_to_string(&mut raw).unwrap();
    let (head, body) = raw.split_once("\r\n\r\n").unwrap_or((raw.as_str(), ""));
    let status = head
        .lines()
        .next()
        .and_then(|l| l.split_whitespace().nth(1))
        .and_then(|s| s.parse().ok())
        .unwrap_or(0);
    (status, body.to_string())
}

fn state_version(port: u16) -> u64 {
    let (status, body) = http_get(port, "/state");
    assert_eq!(status, 200, "GET /state failed: {body}");
    let json: serde_json::Value = serde_json::from_str(&body).unwrap();
    json["version"].as_u64().unwrap()
}

#[test]
fn serve_project_serves_state_and_rebuilds_on_file_change_e2e() {
    let dir = Path::new("/tmp/cadspec_serve_e2e");
    let _ = fs::remove_dir_all(dir);
    copy_project_sources(Path::new("examples/vivienda"), dir);

    let port = free_local_port();
    let serve_dir = dir.to_path_buf();
    std::thread::spawn(move || {
        // Leaked on purpose: serve_project blocks forever in listener.incoming()
        // (no shutdown hook exists); the thread dies with the test process.
        let _ = cadspec::serve::serve_project(&serve_dir, port, false);
    });
    assert!(
        wait_for_port(port, std::time::Duration::from_secs(5)),
        "serve did not start listening on 127.0.0.1:{port} in time"
    );

    let (status, body) = http_get(port, "/state");
    assert_eq!(status, 200);
    assert!(body.contains("Vivienda Unifamiliar Lote 12"));
    let initial_version = state_version(port);

    let (svg_status, svg_body) = http_get(port, "/preview.svg");
    assert_eq!(svg_status, 200);
    assert!(svg_body.contains("<svg"));

    let (favicon_status, favicon_body) = http_get(port, "/favicon.svg");
    assert_eq!(favicon_status, 200);
    assert!(favicon_body.contains("<svg"));

    let (missing_status, _) = http_get(port, "/does-not-exist");
    assert_eq!(missing_status, 404);

    // Editing a watched .cf file must trigger a rebuild: bump the state version.
    let muros = dir.join("muros.cf");
    let mut contents = fs::read_to_string(&muros).unwrap();
    contents
        .push_str("\n# touched by serve_project_serves_state_and_rebuilds_on_file_change_e2e\n");
    fs::write(&muros, contents).unwrap();

    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(5);
    let mut rebuilt_version = initial_version;
    while std::time::Instant::now() < deadline && rebuilt_version <= initial_version {
        std::thread::sleep(std::time::Duration::from_millis(100));
        rebuilt_version = state_version(port);
    }
    assert!(
        rebuilt_version > initial_version,
        "expected a rebuild (version > {initial_version}) after editing muros.cf, got {rebuilt_version}"
    );

    let _ = fs::remove_dir_all(dir);
}

#[test]
fn watch_project_rebuilds_dxf_on_file_change_e2e() {
    let dir = Path::new("/tmp/cadspec_watch_e2e");
    let _ = fs::remove_dir_all(dir);
    copy_project_sources(Path::new("examples/vivienda"), dir);

    let output = dir.join("output.dxf");
    assert!(!output.exists());

    let watch_dir = dir.to_path_buf();
    std::thread::spawn(move || {
        // Leaked on purpose: watch_project blocks forever on rx.recv() (no
        // shutdown hook exists); the thread dies with the test process.
        let _ = cadspec::watch::watch_project(&watch_dir);
    });

    // Give the watcher time to register and clear its own startup debounce
    // window (a change within DEBOUNCE=300ms of start is silently dropped,
    // since `last_build` is initialized before the watch loop even starts).
    std::thread::sleep(std::time::Duration::from_millis(500));

    let muros = dir.join("muros.cf");
    let mut contents = fs::read_to_string(&muros).unwrap();
    contents.push_str("\n# touched by watch_project_rebuilds_dxf_on_file_change_e2e\n");
    fs::write(&muros, contents).unwrap();

    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(5);
    while std::time::Instant::now() < deadline && !output.exists() {
        std::thread::sleep(std::time::Duration::from_millis(100));
    }
    assert!(
        output.exists(),
        "watch did not rebuild output.dxf after editing muros.cf"
    );
    let dxf = fs::read_to_string(&output).unwrap();
    assert!(
        dxf.contains("SECTION"),
        "output.dxf does not look like a DXF file"
    );

    let _ = fs::remove_dir_all(dir);
}
