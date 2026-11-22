//! The brand as the installation sets it (ADR-222): the names and accents,
//! checked here with the rest of the configuration, and the folder its
//! images are read from when the API starts.

use std::path::PathBuf;

use mimorya_core::brand::{self, Accent, BrandNames, DEFAULT_PRODUCT_NAME, Scheme};

use super::reader::Reader;

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct BrandSettings {
    pub names: BrandNames,
    pub accent: Accent,
    /// Where the logo and the favicon are (`MIMORYA_BRAND_DIR`); none, no
    /// image.
    pub folder: Option<PathBuf>,
}

impl Reader<'_> {
    pub(super) fn brand(&mut self) -> BrandSettings {
        let product = self
            .name("MIMORYA_PRODUCT_NAME", true)
            .unwrap_or_else(|| DEFAULT_PRODUCT_NAME.to_string());
        let assistant = self
            .name("MIMORYA_ASSISTANT_NAME", false)
            .unwrap_or_else(|| product.clone());
        let accent = Accent {
            light: self.accent("MIMORYA_ACCENT_LIGHT", Scheme::Light),
            dark: self.accent("MIMORYA_ACCENT_DARK", Scheme::Dark),
        };
        BrandSettings {
            names: BrandNames { product, assistant },
            accent,
            folder: self.value("MIMORYA_BRAND_DIR").map(PathBuf::from),
        }
    }

    fn name(&mut self, key: &str, product: bool) -> Option<String> {
        let written = self.value(key)?;
        brand::name(&written, product)
            .map_err(|problem| self.issue(key, &problem))
            .ok()
    }

    fn accent(&mut self, key: &str, scheme: Scheme) -> Option<brand::Palette> {
        let written = self.value(key)?;
        brand::palette(scheme, &written)
            .map_err(|problem| self.issue(key, &problem))
            .ok()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::AppConfig;

    fn config(vars: &[(&str, &str)]) -> anyhow::Result<AppConfig> {
        let mut all = vec![(
            "DATABASE_APP_URL",
            "postgres://mimorya_app:secret@db/mimorya",
        )];
        all.extend_from_slice(vars);
        AppConfig::from_vars(|key| {
            all.iter()
                .find(|(name, _)| *name == key)
                .map(|(_, value)| value.to_string())
        })
    }

    #[test]
    fn nothing_set_nothing_changes() {
        let brand = config(&[]).unwrap().brand;
        assert_eq!(brand, BrandSettings::default());
        assert_eq!(
            (brand.names.product.as_str(), brand.names.assistant.as_str()),
            ("Mimorya", "Mimorya")
        );
        // Empty values, as a stack passes them, are unset.
        let empty = [
            ("MIMORYA_PRODUCT_NAME", ""),
            ("MIMORYA_ASSISTANT_NAME", " "),
            ("MIMORYA_ACCENT_LIGHT", ""),
            ("MIMORYA_BRAND_DIR", ""),
        ];
        assert_eq!(config(&empty).unwrap().brand, BrandSettings::default());
    }

    #[test]
    fn the_assistant_takes_the_product_s_name_unless_named() {
        let named = config(&[("MIMORYA_PRODUCT_NAME", " Néo Assistant ")])
            .unwrap()
            .brand;
        assert_eq!(named.names.product, "Néo Assistant");
        assert_eq!(named.names.assistant, "Néo Assistant");
        let both = config(&[
            ("MIMORYA_PRODUCT_NAME", "Néo Assistant"),
            ("MIMORYA_ASSISTANT_NAME", "Ada"),
            ("MIMORYA_ACCENT_DARK", "#8fd0b0"),
            ("MIMORYA_BRAND_DIR", "/etc/mimorya/brand"),
        ])
        .unwrap()
        .brand;
        assert_eq!(both.names.assistant, "Ada");
        assert_eq!(both.accent.light, None);
        assert_eq!(both.accent.dark.unwrap().accent.to_string(), "#8fd0b0");
        assert_eq!(both.folder.unwrap(), PathBuf::from("/etc/mimorya/brand"));
    }

    #[test]
    fn a_brand_that_breaks_or_cannot_be_read_stops_the_start() {
        let refused = config(&[
            ("MIMORYA_PRODUCT_NAME", "Néo : assistant"),
            ("MIMORYA_ASSISTANT_NAME", "Ada\r\nBcc: x@y.example"),
            ("MIMORYA_ACCENT_LIGHT", "#7fb3d5"),
            ("MIMORYA_ACCENT_DARK", "bleu"),
        ])
        .unwrap_err()
        .to_string();
        for said in [
            "- MIMORYA_PRODUCT_NAME: deux-points refusés",
            "- MIMORYA_ASSISTANT_NAME: caractère de contrôle",
            "- MIMORYA_ACCENT_LIGHT: #7fb3d5 n’a qu’un contraste de 2,",
            "WCAG AA en demande 4,5:1 : choisissez une couleur plus foncée",
            "- MIMORYA_ACCENT_DARK: couleur au format #rrggbb attendue, pas « bleu »",
        ] {
            assert!(refused.contains(said), "{said}\n{refused}");
        }
    }
}
