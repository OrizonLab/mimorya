//! The brand the interface shows (ADR-222), read before any session: the
//! sign-in page carries it.

use serde::Serialize;
use ts_rs::TS;

/// `GET /brand`.
#[derive(Debug, Serialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct BrandView {
    /// The tab, the sign-in, the sidebar.
    pub product_name: String,
    /// The conversations.
    pub assistant_name: String,
    /// The start of the names of the files people download (`mimorya`).
    pub file_prefix: String,
    /// Served at `/brand/logo`, if the installation gives one.
    pub logo: Option<BrandImageView>,
    /// Served at `/brand/favicon`.
    pub favicon: Option<BrandImageView>,
    /// The accent of each scheme the installation sets; the interface's own
    /// for the other.
    pub light: Option<BrandPalette>,
    pub dark: Option<BrandPalette>,
}

#[derive(Debug, Serialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct BrandImageView {
    /// Changes with the image: its address carries it (`?v=`).
    pub version: String,
    pub media_type: String,
}

/// Colours as `#rrggbb`, named as the interface's tokens.
#[derive(Debug, Serialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct BrandPalette {
    pub accent: String,
    pub accent_fg: String,
    pub accent_soft: String,
}
