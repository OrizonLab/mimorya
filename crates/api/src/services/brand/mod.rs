//! The brand the platform shows (ADR-222): its names and accents from the
//! configuration, its logo and favicon read once from the brand's folder
//! when the API starts. A file that is not what its name says, too heavy,
//! an SVG that could run anything, a name the folder does not know or two
//! files for one image stop the start, each said in French.

mod folder;

use mimorya_core::brand::{Accent, BrandNames, ImageKind, ImageRole};
use sha2::{Digest, Sha256};

use crate::config::BrandSettings;
pub use folder::read_folder;

/// An image of the brand, as it is served.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BrandImage {
    pub kind: ImageKind,
    pub bytes: Vec<u8>,
    /// The start of its SHA-256: its address changes with it, so that a
    /// browser keeps it as long as it likes.
    pub version: String,
}

impl BrandImage {
    pub fn new(kind: ImageKind, bytes: Vec<u8>) -> Self {
        let digest = Sha256::digest(&bytes);
        let version = digest[..8]
            .iter()
            .map(|byte| format!("{byte:02x}"))
            .collect();
        Self {
            kind,
            bytes,
            version,
        }
    }
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Brand {
    pub names: BrandNames,
    pub accent: Accent,
    pub logo: Option<BrandImage>,
    pub favicon: Option<BrandImage>,
}

impl Brand {
    /// The brand of a process: its images read when it serves the API,
    /// which alone shows them.
    pub fn load(settings: &BrandSettings, serves_http: bool) -> anyhow::Result<Self> {
        let mut images = match (&settings.folder, serves_http) {
            (Some(folder), true) => read_folder(folder)?,
            _ => Vec::new(),
        };
        let mut take = |role: ImageRole| {
            let at = images.iter().position(|(found, _)| *found == role)?;
            Some(images.swap_remove(at).1)
        };
        Ok(Self {
            names: settings.names.clone(),
            accent: settings.accent,
            logo: take(ImageRole::Logo),
            favicon: take(ImageRole::Favicon),
        })
    }

    pub fn image(&self, role: ImageRole) -> Option<&BrandImage> {
        match role {
            ImageRole::Logo => self.logo.as_ref(),
            ImageRole::Favicon => self.favicon.as_ref(),
        }
    }
}
