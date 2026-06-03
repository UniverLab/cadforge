//! Color and weight conversion utilities for DXF output.

/// ACI color index from hex string (best-effort mapping to standard palette).
pub fn hex_to_aci(hex: &str) -> u8 {
    match hex.to_uppercase().trim_start_matches('#') {
        "FF0000" => 1, // red
        "FFFF00" => 2, // yellow
        "00FF00" => 3, // green
        "00FFFF" => 4, // cyan
        "0000FF" => 5, // blue
        "FF00FF" => 6, // magenta
        "FFFFFF" => 7, // white
        "808080" => 8, // dark grey
        "C0C0C0" => 9, // light grey
        _ => 7,        // default white
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
        assert_eq!(hex_to_aci("#123456"), 7); // unknown → white
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
