//! The platform's brand (ADR-222): the names people read, the accent of the
//! interface and the images it shows, set at installation with the
//! published images as they are. Each is checked here before the platform
//! starts: a name that would break an e-mail header, an accent people could
//! not read (WCAG 2 AA), a file that is not the image its name says, too
//! heavy, or an SVG that could run a script. Nothing set, nothing changes:
//! « Mimorya », the interface's own colours, no image.

mod colors;
mod images;
mod names;
mod svg;
mod theme;

pub use colors::Color;
pub use images::{ImageKind, ImageRole, named, refusal};
pub use names::{BrandNames, DEFAULT_PRODUCT_NAME, MAX_NAME_CHARS, file_prefix, name};
pub use theme::{Accent, MIN_CONTRAST, Palette, Scheme, palette};
