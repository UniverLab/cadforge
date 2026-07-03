//! Color and weight conversion utilities for DXF output.

/// Standard ACI palette (indices 1-9) as RGB triples.
const ACI_PALETTE: [(u8, (u8, u8, u8)); 9] = [
    (1, (0xFF, 0x00, 0x00)), // red
    (2, (0xFF, 0xFF, 0x00)), // yellow
    (3, (0x00, 0xFF, 0x00)), // green
    (4, (0x00, 0xFF, 0xFF)), // cyan
    (5, (0x00, 0x00, 0xFF)), // blue
    (6, (0xFF, 0x00, 0xFF)), // magenta
    (7, (0xFF, 0xFF, 0xFF)), // white
    (8, (0x80, 0x80, 0x80)), // dark grey
    (9, (0xC0, 0xC0, 0xC0)), // light grey
];

/// ACI color index from hex string: nearest color in the standard palette
/// (unparseable input falls back to 7, white).
pub fn hex_to_aci(hex: &str) -> u8 {
    let hex = hex.trim_start_matches('#');
    let Ok(rgb) = u32::from_str_radix(hex, 16) else {
        return 7;
    };
    if hex.len() != 6 {
        return 7;
    }
    let (r, g, b) = (
        (rgb >> 16) as i32,
        ((rgb >> 8) & 0xFF) as i32,
        (rgb & 0xFF) as i32,
    );
    ACI_PALETTE
        .iter()
        .min_by_key(|(_, (pr, pg, pb))| {
            let (dr, dg, db) = (r - *pr as i32, g - *pg as i32, b - *pb as i32);
            dr * dr + dg * dg + db * db
        })
        .map(|(index, _)| *index)
        .unwrap_or(7)
}

/// Hex color from an ACI color index (inverse of `hex_to_aci`).
pub fn aci_to_hex(index: u8) -> &'static str {
    match index {
        1 => "#FF0000", // red
        2 => "#FFFF00", // yellow
        3 => "#00FF00", // green
        4 => "#00FFFF", // cyan
        5 => "#0000FF", // blue
        6 => "#FF00FF", // magenta
        7 => "#FFFFFF", // white
        8 => "#808080", // dark grey
        9 => "#C0C0C0", // light grey
        _ => "#FFFFFF", // default white
    }
}

/// Convert hex color string to 24-bit integer for DXF true color.
pub fn hex_to_24bit(hex: &str) -> i32 {
    let hex = hex.trim_start_matches('#');
    i32::from_str_radix(hex, 16).unwrap_or(0x00FF_FFFF)
}

/// Lineweight in mm → DXF lineweight enum value (hundredths of mm).
pub fn weight_to_dxf(mm: f64) -> i16 {
    (mm * 100.0) as i16
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hex_to_aci_maps_standard_colors() {
        assert_eq!(hex_to_aci("#FF0000"), 1);
        assert_eq!(hex_to_aci("#00FF00"), 3);
        assert_eq!(hex_to_aci("#FFFFFF"), 7);
    }

    #[test]
    fn hex_to_aci_maps_arbitrary_colors_to_nearest() {
        assert_eq!(hex_to_aci("#FF4444"), 1); // reddish → red
        assert_eq!(hex_to_aci("#00CC44"), 3); // greenish → green
        assert_eq!(hex_to_aci("#2244CC"), 5); // bluish → blue
        assert_eq!(hex_to_aci("#123456"), 8); // dark muted → dark grey
        assert_eq!(hex_to_aci("#invalid"), 7); // unparseable → white
        assert_eq!(hex_to_aci("#FFF"), 7); // wrong length → white
    }

    #[test]
    fn aci_roundtrips_standard_palette() {
        for index in 1..=9u8 {
            assert_eq!(hex_to_aci(aci_to_hex(index)), index);
        }
        assert_eq!(aci_to_hex(42), "#FFFFFF"); // unknown → white
    }

    #[test]
    fn hex_to_24bit_parses_correctly() {
        assert_eq!(hex_to_24bit("#FF0000"), 0xFF0000);
        assert_eq!(hex_to_24bit("00FF00"), 0x00FF00);
        assert_eq!(hex_to_24bit("#invalid"), 0x00FF_FFFF);
    }

    #[test]
    fn weight_converts_mm_to_hundredths() {
        assert_eq!(weight_to_dxf(0.35), 35);
        assert_eq!(weight_to_dxf(0.50), 50);
        assert_eq!(weight_to_dxf(1.0), 100);
    }
}
