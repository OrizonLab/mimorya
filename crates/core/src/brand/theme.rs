//! The accent of the interface, in its light and dark schemes: the colour of
//! its links, buttons and selections, with the two the interface draws from
//! it (the text on a button, the pale ground of a selection). An accent is
//! taken only if people can read it on every ground the interface draws it
//! on, as WCAG 2 AA asks of text (4.5:1).

use super::Color;

/// The contrast WCAG 2 AA asks of text (« 1.4.3 Contrast (Minimum) »).
pub const MIN_CONTRAST: f64 = 4.5;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Scheme {
    Light,
    Dark,
}

/// The colours drawn from an accent, as the interface's tokens name them
/// (`--accent`, `--accent-fg`, `--accent-soft`, apps/web/app/globals.css).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Palette {
    pub accent: Color,
    /// The text on the accent: a button's, a message's.
    pub accent_fg: Color,
    /// The pale ground of what is selected, its text in the accent.
    pub accent_soft: Color,
}

/// What the installation sets of each scheme; the interface's own accent
/// for the scheme it leaves.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Accent {
    pub light: Option<Palette>,
    pub dark: Option<Palette>,
}

/// The grounds of a scheme, as `globals.css` sets them.
pub(super) struct Grounds {
    pub(super) bg: Color,
    pub(super) surface: Color,
    pub(super) surface_2: Color,
    pub(super) accent_fg: Color,
    /// The accent's share in its pale ground, over the surface.
    soft_share: f64,
}

pub(super) const LIGHT: Grounds = Grounds {
    bg: Color::rgb(0xf5, 0xf6, 0xf8),
    surface: Color::rgb(0xff, 0xff, 0xff),
    surface_2: Color::rgb(0xf0, 0xf2, 0xf5),
    accent_fg: Color::rgb(0xff, 0xff, 0xff),
    soft_share: 0.12,
};

pub(super) const DARK: Grounds = Grounds {
    bg: Color::rgb(0x0f, 0x15, 0x1c),
    surface: Color::rgb(0x15, 0x1d, 0x26),
    surface_2: Color::rgb(0x1b, 0x25, 0x30),
    accent_fg: Color::rgb(0x0b, 0x12, 0x19),
    soft_share: 0.2,
};

impl Scheme {
    pub(super) fn grounds(self) -> &'static Grounds {
        match self {
            Self::Light => &LIGHT,
            Self::Dark => &DARK,
        }
    }

    fn name(self) -> &'static str {
        match self {
            Self::Light => "clair",
            Self::Dark => "sombre",
        }
    }

    /// Which way an accent too close to its grounds must go.
    fn remedy(self) -> &'static str {
        match self {
            Self::Light => "plus foncée",
            Self::Dark => "plus claire",
        }
    }
}

/// The palette of an accent written `#rrggbb`, or why it is refused.
pub fn palette(scheme: Scheme, written: &str) -> Result<Palette, String> {
    let accent = Color::parse(written).ok_or_else(|| {
        format!(
            "couleur au format #rrggbb attendue, pas « {} »",
            written.trim()
        )
    })?;
    let grounds = scheme.grounds();
    let accent_soft = accent.over(grounds.surface, grounds.soft_share);
    let pairs = [
        (grounds.bg, "le fond de la page"),
        (grounds.surface, "le fond des cartes"),
        (grounds.surface_2, "le fond des zones secondaires"),
        (accent_soft, "le fond pâle d’une sélection"),
        (grounds.accent_fg, "le texte d’un bouton"),
    ];
    for (ground, what) in pairs {
        let ratio = accent.contrast(ground);
        if ratio < MIN_CONTRAST {
            return Err(format!(
                "{accent} n’a qu’un contraste de {}:1 avec {what} ({ground}) en thème {} ; \
                 WCAG AA en demande 4,5:1 : choisissez une couleur {}",
                shown(ratio),
                scheme.name(),
                scheme.remedy()
            ));
        }
    }
    Ok(Palette {
        accent,
        accent_fg: grounds.accent_fg,
        accent_soft,
    })
}

/// A ratio as people read it, cut (not rounded) to two decimals: 4.499
/// shows 4,49, never the 4,5 it fails to reach.
fn shown(ratio: f64) -> String {
    format!("{:.2}", (ratio * 100.0).floor() / 100.0).replace('.', ",")
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The interface's own accents and text, from `globals.css`.
    const LIGHT_ACCENT: &str = "#1f5f8b";
    const DARK_ACCENT: &str = "#5ea7d8";
    const LIGHT_TEXT: Color = Color::rgb(0x16, 0x20, 0x2c);
    const DARK_TEXT: Color = Color::rgb(0xe6, 0xeb, 0xf0);

    #[test]
    fn the_interface_s_own_accents_are_taken() {
        let light = palette(Scheme::Light, LIGHT_ACCENT).unwrap();
        assert_eq!(light.accent_fg, Color::rgb(255, 255, 255));
        assert_eq!(light.accent_soft.to_string(), "#e4ecf1");
        let dark = palette(Scheme::Dark, DARK_ACCENT).unwrap();
        assert_eq!(dark.accent_fg, Color::rgb(0x0b, 0x12, 0x19));
    }

    #[test]
    fn an_accent_people_could_not_read_is_refused_with_its_remedy() {
        let pale = palette(Scheme::Light, "#7fb3d5").unwrap_err();
        assert!(
            pale.starts_with("#7fb3d5 n’a qu’un contraste de 2,"),
            "{pale}"
        );
        assert!(
            pale.contains("le fond de la page (#f5f6f8) en thème clair"),
            "{pale}"
        );
        assert!(
            pale.ends_with("choisissez une couleur plus foncée"),
            "{pale}"
        );
        let deep = palette(Scheme::Dark, "#1f5f8b").unwrap_err();
        assert!(
            deep.ends_with("choisissez une couleur plus claire"),
            "{deep}"
        );
        assert!(deep.contains("en thème sombre"), "{deep}");
    }

    #[test]
    fn the_ratio_shown_never_rounds_up_to_the_threshold() {
        assert_eq!(shown(4.499), "4,49");
        assert_eq!(shown(2.0), "2,00");
        // Just under 4.5:1 on white (4.48:1), refused there too.
        let grey = Color::parse("#777777").unwrap();
        assert_eq!(shown(grey.contrast(LIGHT.surface)), "4,47");
        assert!(palette(Scheme::Light, "#777777").is_err());
    }

    #[test]
    fn a_colour_not_written_as_rrggbb_is_refused() {
        for written in ["bleu", "#fff", "1f5f8b"] {
            let refused = palette(Scheme::Light, written).unwrap_err();
            assert!(
                refused.starts_with("couleur au format #rrggbb attendue"),
                "{refused}"
            );
        }
    }

    #[test]
    fn the_text_on_a_pale_selection_stays_readable_whatever_the_accent() {
        // The pale ground is a tint of the accent: the scheme's text stays
        // readable on it, even from the extreme accents.
        for (scheme, text, accent) in [
            (Scheme::Light, LIGHT_TEXT, Color::rgb(0, 0, 0)),
            (Scheme::Light, LIGHT_TEXT, Color::rgb(0x76, 0x76, 0x76)),
            (Scheme::Dark, DARK_TEXT, Color::rgb(255, 255, 255)),
            (Scheme::Dark, DARK_TEXT, Color::rgb(0x8a, 0x8a, 0x8a)),
        ] {
            let grounds = scheme.grounds();
            let soft = accent.over(grounds.surface, grounds.soft_share);
            assert!(text.contrast(soft) >= MIN_CONTRAST, "{accent} {scheme:?}");
        }
    }

    #[test]
    fn the_grounds_are_the_interface_s() {
        let css = include_str!("../../../../apps/web/app/globals.css");
        let (light, dark) = css.split_once("prefers-color-scheme: dark").unwrap();
        for (block, grounds, accent, text) in [
            (light, &LIGHT, LIGHT_ACCENT, LIGHT_TEXT),
            (dark, &DARK, DARK_ACCENT, DARK_TEXT),
        ] {
            for (token, colour) in [
                ("--bg", grounds.bg.to_string()),
                ("--surface", grounds.surface.to_string()),
                ("--surface-2", grounds.surface_2.to_string()),
                ("--fg", text.to_string()),
                ("--accent-fg", grounds.accent_fg.to_string()),
                ("--accent", accent.to_string()),
            ] {
                assert!(
                    block.contains(&format!("{token}: {colour};")),
                    "{token}: {colour}"
                );
            }
        }
    }
}
