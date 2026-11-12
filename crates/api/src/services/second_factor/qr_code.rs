//! The QR code an authenticator app reads at an enrolment (ADR-203): the
//! `otpauth://` address drawn as SVG on the server, given to the browser as
//! a `data:` image (the page's policy allows them, `img-src`). The page
//! loads no QR library and nothing about the secret leaves the response.

use base64::Engine;
use base64::engine::general_purpose::STANDARD;
use qrcode::render::svg;
use qrcode::{EcLevel, QrCode};

/// Squares of quiet margin and pixels per module of the drawing.
const MODULE_PIXELS: u32 = 6;

/// The QR code of `uri`, as a `data:image/svg+xml` address; `None` if the
/// address does not fit one (it always does: an `otpauth://` address is a
/// few hundred characters at most).
pub fn qr_code(uri: &str) -> Option<String> {
    let code = QrCode::with_error_correction_level(uri.as_bytes(), EcLevel::M).ok()?;
    let drawing = code
        .render::<svg::Color<'_>>()
        .module_dimensions(MODULE_PIXELS, MODULE_PIXELS)
        .dark_color(svg::Color("#000000"))
        .light_color(svg::Color("#ffffff"))
        .quiet_zone(true)
        .build();
    Some(format!(
        "data:image/svg+xml;base64,{}",
        STANDARD.encode(drawing)
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn draws_the_address_as_an_svg_image() {
        let uri = "otpauth://totp/Mimorya:paul.renaud%40neo-maintenance.example\
                   ?secret=JBSWY3DPEHPK3PXPJBSWY3DPEHPK3PXP&issuer=Mimorya\
                   &algorithm=SHA1&digits=6&period=30";
        let image = qr_code(uri).unwrap();
        let encoded = image.strip_prefix("data:image/svg+xml;base64,").unwrap();
        let svg = String::from_utf8(STANDARD.decode(encoded).unwrap()).unwrap();
        assert!(svg.contains("<svg") && svg.contains("</svg>"));
        // Black on white, whatever the page's theme.
        assert!(svg.contains("#000000") && svg.contains("#ffffff"));
        // Small enough for a response.
        assert!(image.len() < 64 * 1024, "{}", image.len());
    }

    #[test]
    fn the_same_address_draws_the_same_code() {
        assert_eq!(qr_code("otpauth://totp/a"), qr_code("otpauth://totp/a"));
        assert_ne!(qr_code("otpauth://totp/a"), qr_code("otpauth://totp/b"));
    }
}
