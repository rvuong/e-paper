//! The phase 2 pictograms: 1-bit 96 px images embedded in the binary.
//!
//! Made once from Tabler Icons (MIT licence, `assets/icons/`), as drawn in
//! the mockups (`tools/export_mockups.py`).

use crate::frame::decode_png;

/// Width and height of a pictogram, the height of a departure time.
pub const ICON_SIZE: u32 = 96;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Icon {
    /// The direction's service is over, or not started yet.
    Moon,
    /// The bus data is unavailable.
    Wrench,
}

impl Icon {
    /// The pixels, row by row, `true` for black.
    pub fn pixels(self) -> Vec<bool> {
        let png: &[u8] = match self {
            Icon::Moon => include_bytes!("../assets/icons/moon.png"),
            Icon::Wrench => include_bytes!("../assets/icons/wrench.png"),
        };
        let (_, _, pixels) = decode_png(png).expect("embedded icon is a valid PNG");
        pixels
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // D. Pictograms

    #[test]
    fn icons_are_96_px_and_have_black_pixels() {
        for (icon, png) in [
            (Icon::Moon, &include_bytes!("../assets/icons/moon.png")[..]),
            (
                Icon::Wrench,
                &include_bytes!("../assets/icons/wrench.png")[..],
            ),
        ] {
            let (width, height, _) = decode_png(png).unwrap();
            assert_eq!((width, height), (ICON_SIZE, ICON_SIZE), "{icon:?}");
            let black = icon.pixels().iter().filter(|&&black| black).count();
            assert!(black > 1000, "{icon:?}: {black} black pixels");
        }
    }
}
