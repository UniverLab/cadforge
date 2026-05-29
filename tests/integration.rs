//! Integration tests — full pipeline from .cf files to DXF output.

use cadforge::compiler::compile_project;
use std::fs;
use std::path::Path;

#[test]
fn compile_example_project_produces_valid_dxf() {
    let project_dir = Path::new("examples/vivienda");
    let output = project_dir.join("output.dxf");

    // Remove previous output if exists
    let _ = fs::remove_file(&output);

    compile_project(project_dir, None).unwrap();

    assert!(output.exists(), "output.dxf should be created");

    let content = fs::read_to_string(&output).unwrap();

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
    let result = compile_project(Path::new("/tmp/nonexistent_cadforge_dir"), None);
    assert!(result.is_err());
}

#[test]
fn check_project_validates_without_generating_dxf() {
    use cadforge::compiler::check_project;

    let project_dir = Path::new("examples/vivienda");
    let output = project_dir.join("check_should_not_exist.dxf");
    let _ = std::fs::remove_file(&output);

    let count = check_project(project_dir).unwrap();
    assert_eq!(count, 20);
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

    let cf: cadforge::model::CfFile = toml::from_str(toml).unwrap();
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

    let cf: cadforge::model::CfFile = toml::from_str(toml).unwrap();
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

    let cf: cadforge::model::CfFile = toml::from_str(toml).unwrap();
    assert_eq!(cf.hatches.len(), 1);
    assert_eq!(cf.hatches[0].boundary, "pl-room");

    // Compile it to verify no panic
    use cadforge::dxf_writer::DxfWriter;
    let mut writer = DxfWriter::new();
    writer.add_layer("test", 7);
    cadforge::compiler::compile_cf_public(&mut writer, &cf, "test");
}
