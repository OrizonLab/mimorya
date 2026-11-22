//! What the configuration itself holds: the master key, the brand.

use super::{Finding, State};
use crate::config::AppConfig;
use crate::services::brand::{Brand, BrandImage};

pub(super) fn master_key(config: &AppConfig) -> Finding {
    const TOPIC: &str = "Clé maître";
    match config.secrets {
        Some(_) => Finding::new(
            TOPIC,
            State::Ready,
            "présente ; gardez-en une copie hors du serveur : perdue, les secrets scellés \
             sont illisibles",
        ),
        None => Finding::new(
            TOPIC,
            State::Failed,
            "absente (MIMORYA_MASTER_KEY) : aucune entreprise ne peut garder sa clé de modèle \
             ni relier un compte",
        ),
    }
}

/// The brand as the API serves it, its images read from their folder.
pub(super) fn brand(config: &AppConfig) -> Finding {
    const TOPIC: &str = "Marque";
    let brand = match Brand::load(&config.brand, true) {
        Ok(brand) => brand,
        Err(error) => return Finding::new(TOPIC, State::Failed, format!("{error:#}")),
    };
    let names = &brand.names;
    let named = if names.assistant == names.product {
        format!("« {} »", names.product)
    } else {
        format!("« {} », assistant « {} »", names.product, names.assistant)
    };
    let accent = |palette: Option<mimorya_core::brand::Palette>| {
        palette.map_or("celui de l’interface".to_string(), |p| {
            p.accent.to_string()
        })
    };
    let image = |image: &Option<BrandImage>| {
        image.as_ref().map_or("aucun".to_string(), |image| {
            format!("{} ({} octets)", image.kind.media_type(), image.bytes.len())
        })
    };
    Finding::new(
        TOPIC,
        State::Ready,
        format!(
            "{named} ; accent clair {}, sombre {} ; logo : {} ; favicon : {}",
            accent(brand.accent.light),
            accent(brand.accent.dark),
            image(&brand.logo),
            image(&brand.favicon)
        ),
    )
}
