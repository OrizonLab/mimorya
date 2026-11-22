//! The brand's images, laid in its folder under fixed names (`logo.svg`,
//! `favicon.ico`…): which names are known, what each may weigh, and whether
//! a file is the image its name says, read from its content, never trusted
//! from its extension.

use super::svg;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ImageRole {
    /// Beside the product's name: the sign-in, the sidebar.
    Logo,
    /// The browser tab's icon.
    Favicon,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ImageKind {
    Svg,
    Png,
    Webp,
    Ico,
}

impl ImageRole {
    pub const ALL: [Self; 2] = [Self::Logo, Self::Favicon];

    fn stem(self) -> &'static str {
        match self {
            Self::Logo => "logo",
            Self::Favicon => "favicon",
        }
    }

    /// The kinds the role takes, in the order its file names are listed.
    pub fn kinds(self) -> &'static [ImageKind] {
        match self {
            Self::Logo => &[ImageKind::Svg, ImageKind::Png, ImageKind::Webp],
            Self::Favicon => &[ImageKind::Svg, ImageKind::Png, ImageKind::Ico],
        }
    }

    /// The most a file of the role may weigh: an image of a few hundred
    /// pixels, never a photograph.
    pub fn max_bytes(self) -> usize {
        match self {
            Self::Logo => 512 * 1024,
            Self::Favicon => 128 * 1024,
        }
    }

    /// The role's file names, as the folder must hold them.
    pub fn file_names(self) -> Vec<String> {
        self.kinds()
            .iter()
            .map(|kind| format!("{}.{}", self.stem(), kind.extension()))
            .collect()
    }

    pub fn label(self) -> &'static str {
        match self {
            Self::Logo => "le logo",
            Self::Favicon => "le favicon",
        }
    }
}

impl ImageKind {
    pub fn extension(self) -> &'static str {
        match self {
            Self::Svg => "svg",
            Self::Png => "png",
            Self::Webp => "webp",
            Self::Ico => "ico",
        }
    }

    pub fn media_type(self) -> &'static str {
        match self {
            Self::Svg => "image/svg+xml",
            Self::Png => "image/png",
            Self::Webp => "image/webp",
            Self::Ico => "image/x-icon",
        }
    }

    fn name(self) -> &'static str {
        match self {
            Self::Svg => "SVG",
            Self::Png => "PNG",
            Self::Webp => "WebP",
            Self::Ico => "ICO",
        }
    }
}

/// The role and kind a file name stands for, if it is one of the brand's.
pub fn named(file_name: &str) -> Option<(ImageRole, ImageKind)> {
    ImageRole::ALL.into_iter().find_map(|role| {
        role.kinds()
            .iter()
            .find(|kind| file_name == format!("{}.{}", role.stem(), kind.extension()))
            .map(|kind| (role, *kind))
    })
}

/// Whether `bytes` may be served as the role's image of that kind, or why
/// not: a sentence that follows « logo.png : ».
pub fn refusal(role: ImageRole, kind: ImageKind, bytes: &[u8]) -> Option<String> {
    if bytes.is_empty() {
        return Some("fichier vide".to_string());
    }
    if bytes.len() > role.max_bytes() {
        return Some(format!(
            "{} dépasse {} Kio",
            role.label(),
            role.max_bytes() / 1024
        ));
    }
    let not_the_kind = || Some(format!("ce n’est pas une image {}", kind.name()));
    match kind {
        ImageKind::Png if !bytes.starts_with(b"\x89PNG\r\n\x1a\n") => not_the_kind(),
        ImageKind::Webp
            if !(bytes.len() > 12 && &bytes[..4] == b"RIFF" && &bytes[8..12] == b"WEBP") =>
        {
            not_the_kind()
        }
        // An icon directory: reserved zero, type 1, at least one image.
        ImageKind::Ico
            if !(bytes.len() > 6 && bytes[..4] == [0, 0, 1, 0] && bytes[4..6] != [0, 0]) =>
        {
            not_the_kind()
        }
        ImageKind::Svg => {
            match std::str::from_utf8(bytes.strip_prefix(b"\xef\xbb\xbf").unwrap_or(bytes)) {
                Err(_) => Some("ce n’est pas une image SVG (texte UTF-8 attendu)".to_string()),
                Ok(text) => svg::refusal(text).map(|why| format!("SVG refusé : {why}")),
            }
        }
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const PNG: &[u8] = b"\x89PNG\r\n\x1a\n\0\0\0\rIHDR";
    const ICO: &[u8] = &[0, 0, 1, 0, 1, 0, 16, 16];
    const WEBP: &[u8] = b"RIFF\x1a\0\0\0WEBPVP8 ";

    #[test]
    fn the_brand_s_files_have_fixed_names() {
        assert_eq!(named("logo.svg"), Some((ImageRole::Logo, ImageKind::Svg)));
        assert_eq!(
            named("favicon.ico"),
            Some((ImageRole::Favicon, ImageKind::Ico))
        );
        assert_eq!(named("logo.ico"), None);
        assert_eq!(named("Logo.png"), None);
        assert_eq!(named("logo.png.exe"), None);
        assert_eq!(
            ImageRole::Favicon.file_names(),
            ["favicon.svg", "favicon.png", "favicon.ico"]
        );
    }

    #[test]
    fn an_image_is_read_from_its_content() {
        assert_eq!(refusal(ImageRole::Logo, ImageKind::Png, PNG), None);
        assert_eq!(refusal(ImageRole::Logo, ImageKind::Webp, WEBP), None);
        assert_eq!(refusal(ImageRole::Favicon, ImageKind::Ico, ICO), None);
        let svg = b"\xef\xbb\xbf<svg xmlns=\"http://www.w3.org/2000/svg\"/>";
        assert_eq!(refusal(ImageRole::Favicon, ImageKind::Svg, svg), None);
        assert_eq!(
            refusal(ImageRole::Logo, ImageKind::Png, ICO).unwrap(),
            "ce n’est pas une image PNG"
        );
        assert_eq!(
            refusal(ImageRole::Favicon, ImageKind::Ico, PNG).unwrap(),
            "ce n’est pas une image ICO"
        );
        assert_eq!(
            refusal(ImageRole::Logo, ImageKind::Webp, b"RIFF\0\0\0\0WAVEfmt ").unwrap(),
            "ce n’est pas une image WebP"
        );
        assert_eq!(
            refusal(ImageRole::Logo, ImageKind::Svg, PNG).unwrap(),
            "ce n’est pas une image SVG (texte UTF-8 attendu)"
        );
        assert_eq!(
            refusal(ImageRole::Logo, ImageKind::Png, b"").unwrap(),
            "fichier vide"
        );
    }

    #[test]
    fn an_image_too_heavy_is_refused() {
        let heavy = [PNG, &vec![0; ImageRole::Favicon.max_bytes()]].concat();
        assert_eq!(
            refusal(ImageRole::Favicon, ImageKind::Png, &heavy).unwrap(),
            "le favicon dépasse 128 Kio"
        );
        assert_eq!(refusal(ImageRole::Logo, ImageKind::Png, &heavy), None);
    }

    #[test]
    fn an_svg_that_runs_something_is_refused_and_said_so() {
        let svg = br#"<svg xmlns="http://www.w3.org/2000/svg"><script>alert(1)</script></svg>"#;
        let refused = refusal(ImageRole::Logo, ImageKind::Svg, svg).unwrap();
        assert!(
            refused.starts_with("SVG refusé : élément <script> refusé"),
            "{refused}"
        );
    }
}
