//! Integration tests — full pipeline from .cf files to DXF output.

use cadforge::compiler::compile_project;
use cadforge::importer::import_dxf;
use std::fs;
use std::path::Path;

#[test]
fn compile_example_project_produces_valid_dxf() {
    let project_dir = Path::new("examples/vivienda");
    let output = project_dir.join("output.dxf");

    // Remove previous output if exists
    let _ = fs::remove_file(&output);

    compile_project(project_dir, None, None).unwrap();

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
    let result = compile_project(Path::new("/tmp/nonexistent_cadforge_dir"), None, None);
    assert!(result.is_err());
}

#[test]
fn check_project_validates_without_generating_dxf() {
    use cadforge::compiler::check_project;

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

#[test]
fn solid_fill_generates_triangles() {
    let toml = r##"
[[fill]]
id = "fl-room"
points = [[0.0, 0.0], [4.0, 0.0], [4.0, 3.0], [0.0, 3.0]]
color = "#808080"
"##;

    let cf: cadforge::model::CfFile = toml::from_str(toml).unwrap();
    assert_eq!(cf.fills.len(), 1);

    // Compile and verify it produces a DXF with SOLID entities
    use cadforge::dxf_writer::DxfWriter;
    let mut writer = DxfWriter::new();
    writer.add_layer("test", 7);
    cadforge::compiler::compile_cf_public(&mut writer, &cf, "test");

    let path = std::path::PathBuf::from("/tmp/cadforge_test_fill.dxf");
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
    let dir = Path::new("/tmp/cadforge_constraints_non_strict");
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
    let dir = Path::new("/tmp/cadforge_constraints_strict");
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
fn import_generated_dxf_creates_cadforge_project() {
    let source = Path::new("examples/vivienda");
    let source_output = source.join("output.dxf");
    let _ = fs::remove_file(&source_output);
    compile_project(source, None, Some(&source_output)).unwrap();

    let imported = Path::new("/tmp/cadforge_import_test");
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
