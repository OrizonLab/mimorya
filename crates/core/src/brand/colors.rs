//! Colours as the installation writes them (`#rrggbb`), and their contrast
//! as WCAG 2 measures it.

use std::fmt;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Color {
    pub red: u8,
    pub green: u8,
    pub blue: u8,
}

impl Color {
    pub const fn rgb(red: u8, green: u8, blue: u8) -> Self {
        Self { red, green, blue }
    }

    /// `#rrggbb`, in upper or lower case.
    pub fn parse(text: &str) -> Option<Self> {
        let hex = text.trim().strip_prefix('#')?;
        if hex.len() != 6 || !hex.bytes().all(|byte| byte.is_ascii_hexdigit()) {
            return None;
        }
        let channel = |at: usize| u8::from_str_radix(&hex[at..at + 2], 16).ok();
        Some(Self::rgb(channel(0)?, channel(2)?, channel(4)?))
    }

    /// The relative luminance of WCAG 2.2 (« relative luminance »).
    pub fn luminance(self) -> f64 {
        let linear = |channel: u8| {
            let value = f64::from(channel) / 255.0;
            if value <= 0.04045 {
                value / 12.92
            } else {
                ((value + 0.055) / 1.055).powf(2.4)
            }
        };
        0.2126 * linear(self.red) + 0.7152 * linear(self.green) + 0.0722 * linear(self.blue)
    }

    /// The contrast ratio of two colours, from 1 to 21.
    pub fn contrast(self, other: Self) -> f64 {
        let (one, two) = (self.luminance(), other.luminance());
        let (lighter, darker) = if one > two { (one, two) } else { (two, one) };
        (lighter + 0.05) / (darker + 0.05)
    }

    /// This colour laid over `under` with the opacity `share` (0 to 1).
    pub fn over(self, under: Self, share: f64) -> Self {
        let mix = |top: u8, bottom: u8| {
            let (top, bottom) = (f64::from(top), f64::from(bottom));
            // Between two bytes, rounded: the cast cannot truncate.
            (bottom + (top - bottom) * share).round().clamp(0.0, 255.0) as u8
        };
        Self::rgb(
            mix(self.red, under.red),
            mix(self.green, under.green),
            mix(self.blue, under.blue),
        )
    }
}

impl fmt::Display for Color {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "#{:02x}{:02x}{:02x}", self.red, self.green, self.blue)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn colours_are_read_as_written() {
        assert_eq!(Color::parse("#1F5f8b"), Some(Color::rgb(0x1f, 0x5f, 0x8b)));
        assert_eq!(Color::parse(" #ffffff "), Some(Color::rgb(255, 255, 255)));
        for refused in [
            "1f5f8b",
            "#fff",
            "#1f5f8",
            "#1f5f8bb",
            "#gg0000",
            "",
            "#",
            "rgb(0,0,0)",
        ] {
            assert_eq!(Color::parse(refused), None, "{refused}");
        }
        assert_eq!(Color::rgb(0x1f, 0x5f, 0x8b).to_string(), "#1f5f8b");
    }

    #[test]
    fn contrast_is_measured_as_wcag_does() {
        let (black, white) = (Color::rgb(0, 0, 0), Color::rgb(255, 255, 255));
        assert!((black.contrast(white) - 21.0).abs() < 1e-9);
        assert!((white.contrast(white) - 1.0).abs() < 1e-9);
        // #767676 on white is the classic 4.54:1, just above AA.
        let grey = Color::parse("#767676").unwrap();
        assert!((grey.contrast(white) - 4.54).abs() < 0.01);
        assert_eq!(grey.contrast(white), white.contrast(grey));
    }

    #[test]
    fn a_colour_laid_over_another_mixes_them() {
        let (black, white) = (Color::rgb(0, 0, 0), Color::rgb(255, 255, 255));
        assert_eq!(black.over(white, 0.0), white);
        assert_eq!(black.over(white, 1.0), black);
        assert_eq!(black.over(white, 0.5), Color::rgb(128, 128, 128));
    }
}
