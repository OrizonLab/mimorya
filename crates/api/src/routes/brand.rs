//! The brand (ADR-222), read without a session: the sign-in page shows it.
//! Its images are served with their own policy: never a script, never
//! anything fetched, whatever an SVG holds (it was checked at the start);
//! kept by browsers as long as their address, which changes with them.

use axum::extract::State;
use axum::http::HeaderValue;
use axum::http::header::{CACHE_CONTROL, CONTENT_SECURITY_POLICY, CONTENT_TYPE};
use axum::response::{IntoResponse, Response};
use axum::routing::get;
use axum::{Json, Router};
use mimorya_core::DomainError;
use mimorya_core::brand::{ImageRole, Palette, file_prefix};

use crate::app::AppState;
use crate::contract::{BrandImageView, BrandPalette, BrandView};
use crate::http::{ApiError, ApiResult};
use crate::services::brand::BrandImage;

/// What an image of the brand may do once a browser opens it: draw itself.
const IMAGE_POLICY: &str =
    "default-src 'none'; style-src 'unsafe-inline'; img-src data:; sandbox; frame-ancestors 'none'";

pub fn routes() -> Router<AppState> {
    Router::new()
        .route("/brand", get(brand))
        .route("/brand/logo", get(logo))
        .route("/brand/favicon", get(favicon))
}

async fn brand(State(state): State<AppState>) -> Json<BrandView> {
    let brand = &state.brand;
    let image = |image: &Option<BrandImage>| {
        image.as_ref().map(|image| BrandImageView {
            version: image.version.clone(),
            media_type: image.kind.media_type().to_string(),
        })
    };
    let palette = |palette: Option<Palette>| {
        palette.map(|palette| BrandPalette {
            accent: palette.accent.to_string(),
            accent_fg: palette.accent_fg.to_string(),
            accent_soft: palette.accent_soft.to_string(),
        })
    };
    Json(BrandView {
        product_name: brand.names.product.clone(),
        assistant_name: brand.names.assistant.clone(),
        file_prefix: file_prefix(&brand.names.product),
        logo: image(&brand.logo),
        favicon: image(&brand.favicon),
        light: palette(brand.accent.light),
        dark: palette(brand.accent.dark),
    })
}

async fn logo(State(state): State<AppState>) -> ApiResult<Response> {
    served(state.brand.image(ImageRole::Logo))
}

async fn favicon(State(state): State<AppState>) -> ApiResult<Response> {
    served(state.brand.image(ImageRole::Favicon))
}

fn served(image: Option<&BrandImage>) -> ApiResult<Response> {
    let image = image.ok_or_else(|| ApiError::from(DomainError::not_found("Aucune image")))?;
    let headers = [
        (
            CONTENT_TYPE,
            HeaderValue::from_static(image.kind.content_type()),
        ),
        (
            CONTENT_SECURITY_POLICY,
            HeaderValue::from_static(IMAGE_POLICY),
        ),
        // Its address changes with it (`?v=` its version).
        (
            CACHE_CONTROL,
            HeaderValue::from_static("public, max-age=31536000, immutable"),
        ),
    ];
    Ok((headers, image.bytes.clone()).into_response())
}
