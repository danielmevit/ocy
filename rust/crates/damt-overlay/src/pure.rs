//! Pure overlay maths: colours, premultiplied pixels and border rectangles.
//!
//! Everything here is plain arithmetic, so it is compiled and tested on every target.

use std::fmt;

/// Straight (non-premultiplied) 8-bit RGBA.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub struct Rgba {
    /// Red channel.
    pub r: u8,
    /// Green channel.
    pub g: u8,
    /// Blue channel.
    pub b: u8,
    /// Alpha channel.
    pub a: u8,
}

impl Rgba {
    /// Fully transparent black.
    pub const TRANSPARENT: Self = Self::new(0, 0, 0, 0);

    /// Opaque black.
    pub const BLACK: Self = Self::new(0, 0, 0, 255);

    /// Build a colour from its channels.
    pub const fn new(r: u8, g: u8, b: u8, a: u8) -> Self {
        Self { r, g, b, a }
    }

    /// Build an opaque colour.
    pub const fn rgb(r: u8, g: u8, b: u8) -> Self {
        Self::new(r, g, b, 255)
    }

    /// Same colour with a different alpha.
    pub const fn with_alpha(self, a: u8) -> Self {
        Self { a, ..self }
    }

    /// Parse `#RRGGBB` and apply `alpha`.
    ///
    /// The leading `#` is required. Each channel must be two hexadecimal digits.
    pub fn from_hex_rgb(rgb: &str, alpha: u8) -> Result<Self, ColorParseError> {
        let bytes = rgb.as_bytes();
        if bytes.len() != 7 || bytes[0] != b'#' {
            return Err(ColorParseError::ExpectedHex);
        }

        let r = parse_pair(bytes[1], bytes[2])?;
        let g = parse_pair(bytes[3], bytes[4])?;
        let b = parse_pair(bytes[5], bytes[6])?;
        Ok(Self::new(r, g, b, alpha))
    }

    /// Multiplied channels with the alpha kept. Uses truncating integer maths, like the R0 spike.
    pub const fn premultiplied(self) -> Self {
        Self {
            r: premultiply(self.r, self.a),
            g: premultiply(self.g, self.a),
            b: premultiply(self.b, self.a),
            a: self.a,
        }
    }

    /// The premultiplied pixel word for a 32-bit top-down DIB (`0xAARRGGBB`).
    ///
    /// On a little-endian machine this stores the bytes in `B`, `G`, `R`, `A` order, which is what
    /// `CreateDIBSection` with `BI_RGB` and 32 bits expects.
    pub const fn premultiplied_argb(self) -> u32 {
        let p = self.premultiplied();
        (p.a as u32) << 24 | (p.r as u32) << 16 | (p.g as u32) << 8 | p.b as u32
    }
}

fn parse_pair(high: u8, low: u8) -> Result<u8, ColorParseError> {
    let high = hex_digit(high)?;
    let low = hex_digit(low)?;
    Ok(high << 4 | low)
}

const fn premultiply(channel: u8, alpha: u8) -> u8 {
    (channel as u32 * alpha as u32 / 255) as u8
}

const fn hex_digit(byte: u8) -> Result<u8, ColorParseError> {
    match byte {
        b'0'..=b'9' => Ok(byte - b'0'),
        b'a'..=b'f' => Ok(byte - b'a' + 10),
        b'A'..=b'F' => Ok(byte - b'A' + 10),
        _ => Err(ColorParseError::InvalidDigit),
    }
}

/// Why a `#RRGGBB` colour could not be parsed.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ColorParseError {
    /// The input was not exactly seven bytes starting with `#`.
    ExpectedHex,
    /// A channel byte was not a hexadecimal digit.
    InvalidDigit,
}

impl fmt::Display for ColorParseError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::ExpectedHex => f.write_str("expected a #RRGGBB colour"),
            Self::InvalidDigit => f.write_str("expected hexadecimal colour digits"),
        }
    }
}

impl std::error::Error for ColorParseError {}

/// Pre-scale a DIP length by a monitor's effective DPI.
///
/// A missing or zero DPI is treated as 96. The result is rounded to the nearest physical pixel.
pub fn scaled_length(dip: u32, dpi: u32) -> i32 {
    let dpi = if dpi == 0 { 96 } else { dpi };
    let scaled = (u64::from(dip) * u64::from(dpi) + 48) / 96;
    i32::try_from(scaled).unwrap_or(i32::MAX)
}

/// A rectangle in physical pixels.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub struct PixelRect {
    /// Left edge.
    pub x: i32,
    /// Top edge.
    pub y: i32,
    /// Width; never negative for rectangles produced by this crate.
    pub width: i32,
    /// Height; never negative for rectangles produced by this crate.
    pub height: i32,
}

impl PixelRect {
    /// An empty rectangle at the origin.
    pub const EMPTY: Self = Self {
        x: 0,
        y: 0,
        width: 0,
        height: 0,
    };

    /// Whether the rectangle has no area.
    pub const fn is_empty(self) -> bool {
        self.width <= 0 || self.height <= 0
    }
}

/// The four border strips of a `width` × `height` rectangle with `thickness` pixels.
///
/// Strips overlap in the corners; painting all four with the same colour is equivalent to the R0
/// `x < t || y < t || x >= w - t || y >= h - t` test. A zero or oversized thickness is clamped so no
/// rectangle leaves the surface.
pub fn border_rects(width: i32, height: i32, thickness: i32) -> [PixelRect; 4] {
    let w = width.max(0);
    let h = height.max(0);
    if w == 0 || h == 0 {
        return [PixelRect::EMPTY; 4];
    }

    let t = thickness.max(0).min(w).min(h);
    if t == 0 {
        return [PixelRect::EMPTY; 4];
    }

    [
        PixelRect {
            x: 0,
            y: 0,
            width: w,
            height: t,
        },
        PixelRect {
            x: 0,
            y: h - t,
            width: w,
            height: t,
        },
        PixelRect {
            x: 0,
            y: 0,
            width: t,
            height: h,
        },
        PixelRect {
            x: w - t,
            y: 0,
            width: t,
            height: h,
        },
    ]
}

/// The premultiplied ARGB word for `color`.
pub const fn premultiplied_argb(color: Rgba) -> u32 {
    color.premultiplied_argb()
}

/// Fill every pixel in `pixels` with `word`.
pub fn fill_solid(pixels: &mut [u32], word: u32) {
    pixels.fill(word);
}

/// Paint the border strips of a `width` × `height` surface with `word`.
///
/// `pixels` is row-major with `width` pixels per row. Out-of-range pixels are skipped rather than panicking.
pub fn fill_border(pixels: &mut [u32], width: i32, height: i32, thickness: i32, word: u32) {
    for rect in border_rects(width, height, thickness) {
        fill_rect(pixels, width, rect, word);
    }
}

fn fill_rect(pixels: &mut [u32], width: i32, rect: PixelRect, word: u32) {
    if width <= 0 || rect.is_empty() {
        return;
    }

    let row_width = width as usize;
    let x0 = rect.x.max(0) as usize;
    let y0 = rect.y.max(0) as usize;
    let x1 = rect.x.saturating_add(rect.width).max(0) as usize;
    let y1 = rect.y.saturating_add(rect.height).max(0) as usize;

    for y in y0..y1 {
        let row = y.saturating_mul(row_width);
        for x in x0..x1 {
            if let Some(pixel) = pixels.get_mut(row.saturating_add(x)) {
                *pixel = word;
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn from_hex_rgb_parses_channels_and_alpha() {
        assert_eq!(
            Rgba::from_hex_rgb("#2F7D62", 140),
            Ok(Rgba::new(0x2F, 0x7D, 0x62, 140))
        );
        assert_eq!(Rgba::from_hex_rgb("#000000", 0), Ok(Rgba::new(0, 0, 0, 0)));
        assert_eq!(
            Rgba::from_hex_rgb("#FFFFFF", 255),
            Ok(Rgba::new(0xFF, 0xFF, 0xFF, 255))
        );
    }

    #[test]
    fn from_hex_rgb_rejects_bad_input() {
        for input in ["2F7D62", "#2F7D6", "#2F7D622", ""] {
            assert_eq!(
                Rgba::from_hex_rgb(input, 255),
                Err(ColorParseError::ExpectedHex)
            );
        }

        for input in ["#GGGGGG", "#2F7D6Z"] {
            assert_eq!(
                Rgba::from_hex_rgb(input, 255),
                Err(ColorParseError::InvalidDigit)
            );
        }
    }

    #[test]
    fn premultiply_is_truncating() {
        assert_eq!(
            Rgba::new(255, 255, 255, 0).premultiplied(),
            Rgba::new(0, 0, 0, 0)
        );
        assert_eq!(
            Rgba::new(255, 128, 1, 255).premultiplied(),
            Rgba::new(255, 128, 1, 255)
        );
        assert_eq!(
            Rgba::new(47, 125, 98, 140).premultiplied(),
            Rgba::new(25, 68, 53, 140)
        );
    }

    #[test]
    fn premultiplied_argb_word_is_aarrggbb_and_bgra_on_disk() {
        let color = Rgba::new(0x2F, 0x7D, 0x62, 140);
        let word = premultiplied_argb(color);
        assert_eq!(word, 0x8C19_4435);
        assert_eq!(word.to_le_bytes(), [0x35, 0x44, 0x19, 0x8C]);

        let opaque = Rgba::new(0x2F, 0x7D, 0x62, 255);
        assert_eq!(opaque.premultiplied_argb(), 0xFF2F_7D62);
        assert_eq!(
            opaque.premultiplied_argb().to_le_bytes(),
            [0x62, 0x7D, 0x2F, 0xFF]
        );

        assert_eq!(Rgba::BLACK.premultiplied_argb(), 0xFF00_0000);
        assert_eq!(Rgba::TRANSPARENT.premultiplied_argb(), 0);
    }

    #[test]
    fn scaled_length_uses_per_monitor_dpi() {
        assert_eq!(scaled_length(3, 96), 3);
        assert_eq!(scaled_length(3, 120), 4);
        assert_eq!(scaled_length(3, 144), 5);
        assert_eq!(scaled_length(3, 192), 6);
        assert_eq!(scaled_length(3, 0), 3);
        assert_eq!(scaled_length(0, 300), 0);
    }

    #[test]
    fn border_rects_cover_the_edges() {
        let rects = border_rects(10, 10, 2);
        assert_eq!(
            rects,
            [
                PixelRect {
                    x: 0,
                    y: 0,
                    width: 10,
                    height: 2
                },
                PixelRect {
                    x: 0,
                    y: 8,
                    width: 10,
                    height: 2
                },
                PixelRect {
                    x: 0,
                    y: 0,
                    width: 2,
                    height: 10
                },
                PixelRect {
                    x: 8,
                    y: 0,
                    width: 2,
                    height: 10
                },
            ]
        );
    }

    #[test]
    fn border_rects_clamp_thickness() {
        assert_eq!(border_rects(10, 10, 0), [PixelRect::EMPTY; 4]);
        assert_eq!(border_rects(0, 10, 2), [PixelRect::EMPTY; 4]);
        assert_eq!(border_rects(10, 0, 2), [PixelRect::EMPTY; 4]);

        let rects = border_rects(4, 4, 99);
        assert_eq!(
            rects[0],
            PixelRect {
                x: 0,
                y: 0,
                width: 4,
                height: 4
            }
        );
        assert_eq!(
            rects[1],
            PixelRect {
                x: 0,
                y: 0,
                width: 4,
                height: 4
            }
        );
    }

    #[test]
    fn fill_border_paints_only_the_edges() {
        let (width, height) = (10, 10);
        let mut pixels = vec![0u32; width * height];
        fill_border(&mut pixels, width as i32, height as i32, 2, 0xAB);

        let at = |x: usize, y: usize| pixels[y * width + x];
        assert_eq!(at(0, 0), 0xAB);
        assert_eq!(at(5, 0), 0xAB);
        assert_eq!(at(5, 1), 0xAB);
        assert_eq!(at(0, 5), 0xAB);
        assert_eq!(at(1, 5), 0xAB);
        assert_eq!(at(9, 5), 0xAB);
        assert_eq!(at(8, 5), 0xAB);
        assert_eq!(at(5, 9), 0xAB);
        assert_eq!(at(5, 8), 0xAB);
        assert_eq!(at(2, 5), 0);
        assert_eq!(at(5, 2), 0);
        assert_eq!(at(5, 5), 0);
        assert_eq!(at(7, 7), 0);
    }

    #[test]
    fn fill_border_with_zero_thickness_does_nothing() {
        let mut pixels = vec![7u32; 100];
        fill_border(&mut pixels, 10, 10, 0, 0xAB);
        assert!(pixels.iter().all(|&pixel| pixel == 7));
    }

    #[test]
    fn fill_border_skips_out_of_range_surfaces() {
        let mut pixels = vec![0u32; 9];
        fill_border(&mut pixels, 10, 10, 2, 0xAB);
        assert_eq!(pixels.len(), 9);
    }

    #[test]
    fn fill_solid_paints_every_pixel() {
        let mut pixels = vec![0u32; 4];
        fill_solid(&mut pixels, 0xDEAD_BEEF);
        assert_eq!(pixels, [0xDEAD_BEEF; 4]);
    }
}
