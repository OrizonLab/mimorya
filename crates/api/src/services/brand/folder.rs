//! The brand's folder (`MIMORYA_BRAND_DIR`): only the images' fixed names,
//! a file of each at most, each read within its weight and checked from
//! its content. Hidden files (`.DS_Store`…) are left alone.

use std::fs;
use std::path::Path;

use anyhow::bail;
use mimorya_core::brand::{ImageRole, named, refusal};

use super::BrandImage;

const KEY: &str = "MIMORYA_BRAND_DIR";

/// The images the folder holds, or why it is refused.
pub fn read_folder(folder: &Path) -> anyhow::Result<Vec<(ImageRole, BrandImage)>> {
    let shown = folder.display();
    let entries = match fs::read_dir(folder) {
        Ok(entries) => entries,
        Err(error) => bail!("{KEY} : dossier {shown} illisible ({error})"),
    };
    let mut names = Vec::new();
    for entry in entries {
        let entry = entry.map_err(|error| anyhow::anyhow!("{KEY} : dossier {shown} ({error})"))?;
        let name = entry.file_name().to_string_lossy().into_owned();
        if !name.starts_with('.') {
            names.push(name);
        }
    }
    names.sort();
    let mut images: Vec<(ImageRole, BrandImage, String)> = Vec::new();
    for name in names {
        let Some((role, kind)) = named(&name) else {
            bail!(
                "{KEY} : fichier inattendu « {name} » dans {shown} ; noms reconnus : {}",
                known_names()
            );
        };
        if let Some((_, _, other)) = images.iter().find(|(found, ..)| *found == role) {
            bail!(
                "{KEY} : deux fichiers pour {} dans {shown}, {other} et {name} ; n’en laissez \
                 qu’un",
                role.label()
            );
        }
        let path = folder.join(&name);
        let bytes = read_within(&path, role.max_bytes() as u64)
            .map_err(|problem| anyhow::anyhow!("{KEY} : {name} : {problem}"))?;
        if let Some(problem) = refusal(role, kind, &bytes) {
            bail!("{KEY} : {name} : {problem}");
        }
        images.push((role, BrandImage::new(kind, bytes), name));
    }
    Ok(images
        .into_iter()
        .map(|(role, image, _)| (role, image))
        .collect())
}

/// The file's bytes, one more than `most` at most: never a huge file read
/// whole to be refused.
fn read_within(path: &Path, most: u64) -> Result<Vec<u8>, String> {
    use std::io::Read;
    let file = fs::File::open(path).map_err(|error| format!("illisible ({error})"))?;
    if !file
        .metadata()
        .map_err(|error| error.to_string())?
        .is_file()
    {
        return Err("ce n’est pas un fichier".into());
    }
    let mut bytes = Vec::new();
    file.take(most + 1)
        .read_to_end(&mut bytes)
        .map_err(|error| format!("illisible ({error})"))?;
    Ok(bytes)
}

fn known_names() -> String {
    ImageRole::ALL
        .iter()
        .flat_map(|role| role.file_names())
        .collect::<Vec<_>>()
        .join(", ")
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use mimorya_core::brand::ImageKind;

    use super::*;

    const PNG: &[u8] = b"\x89PNG\r\n\x1a\n\0\0\0\rIHDR";
    const SVG: &[u8] = br#"<svg xmlns="http://www.w3.org/2000/svg"><path d="M0 0h1v1z"/></svg>"#;

    /// A folder of this test alone, emptied first.
    fn folder(test: &str, files: &[(&str, &[u8])]) -> PathBuf {
        let path =
            std::env::temp_dir().join(format!("mimorya-brand-{}-{test}", std::process::id()));
        let _ = fs::remove_dir_all(&path);
        fs::create_dir_all(&path).unwrap();
        for (name, bytes) in files {
            fs::write(path.join(name), bytes).unwrap();
        }
        path
    }

    fn refused(test: &str, files: &[(&str, &[u8])]) -> String {
        read_folder(&folder(test, files)).unwrap_err().to_string()
    }

    #[test]
    fn the_logo_and_the_favicon_are_read() {
        let found = read_folder(&folder(
            "read",
            &[
                ("logo.svg", SVG),
                ("favicon.png", PNG),
                (".DS_Store", b"\0"),
            ],
        ))
        .unwrap();
        let kinds: Vec<_> = found
            .iter()
            .map(|(role, image)| (*role, image.kind))
            .collect();
        assert_eq!(
            kinds,
            [
                (ImageRole::Favicon, ImageKind::Png),
                (ImageRole::Logo, ImageKind::Svg)
            ]
        );
        assert_eq!(found[1].1.version.len(), 16);
        assert!(read_folder(&folder("empty", &[])).unwrap().is_empty());
    }

    #[test]
    fn a_folder_holding_anything_else_is_refused_and_said_so() {
        let unknown = refused("unknown", &[("Logo.png", PNG)]);
        assert!(
            unknown.starts_with("MIMORYA_BRAND_DIR : fichier inattendu « Logo.png »"),
            "{unknown}"
        );
        assert!(
            unknown
                .ends_with("logo.svg, logo.png, logo.webp, favicon.svg, favicon.png, favicon.ico")
        );
        let twice = refused("twice", &[("logo.png", PNG), ("logo.svg", SVG)]);
        assert!(twice.contains("deux fichiers pour le logo"), "{twice}");
        let lying = refused("lying", &[("favicon.png", SVG)]);
        assert_eq!(
            lying,
            "MIMORYA_BRAND_DIR : favicon.png : ce n’est pas une image PNG"
        );
        let scripted = refused(
            "scripted",
            &[(
                "logo.svg",
                br#"<svg xmlns="http://www.w3.org/2000/svg" onload="alert(1)"/>"#,
            )],
        );
        assert!(
            scripted.contains("logo.svg : SVG refusé : attribut onload refusé"),
            "{scripted}"
        );
        let heavy = [PNG, &vec![0; ImageRole::Favicon.max_bytes() * 2]].concat();
        let heavy = refused("heavy", &[("favicon.png", &heavy)]);
        assert!(
            heavy.ends_with("favicon.png : le favicon dépasse 128 Kio"),
            "{heavy}"
        );
        let missing = read_folder(Path::new("/nonexistent/mimorya-brand")).unwrap_err();
        assert!(
            missing
                .to_string()
                .starts_with("MIMORYA_BRAND_DIR : dossier /nonexistent")
        );
    }

    #[test]
    fn a_directory_named_as_an_image_is_refused() {
        let path = folder("directory", &[]);
        fs::create_dir(path.join("logo.png")).unwrap();
        let refused = read_folder(&path).unwrap_err().to_string();
        assert!(refused.contains("logo.png : "), "{refused}");
    }
}
