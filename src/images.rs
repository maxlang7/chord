use anyhow::{Context, Result, bail};
use base64::{
    Engine,
    engine::general_purpose::{STANDARD, URL_SAFE_NO_PAD},
};
use image::{DynamicImage, ImageReader};
use ratatui_image::{picker::Picker, protocol::StatefulProtocol};
use serde_json::Value;
use std::{cell::RefCell, collections::HashMap, io::Cursor, path::PathBuf};

pub const MAX_BYTES: usize = 20 * 1024 * 1024;
#[derive(Clone)]
pub struct Target {
    pub chat: String,
    pub message: String,
    pub index: usize,
    pub attachment: Value,
}
impl Target {
    pub fn key(&self) -> String {
        format!("{}\n{}\n{}", self.chat, self.message, self.index)
    }
    pub fn name(&self) -> &str {
        self.attachment["fileName"].as_str().unwrap_or("image")
    }
}
pub struct Decoded {
    pub bytes: Vec<u8>,
    pub image: DynamicImage,
    pub extension: &'static str,
    pub dimensions: (u32, u32),
}
pub struct Loaded {
    pub data: Decoded,
    pub protocol: RefCell<StatefulProtocol>,
}
pub struct Images {
    pub picker: Picker,
    pub cache: HashMap<String, Result<Loaded, String>>,
    pub pending: Option<String>,
    pub index: usize,
    pub message: String,
}
impl Default for Images {
    fn default() -> Self {
        Self {
            picker: Picker::halfblocks(),
            cache: HashMap::new(),
            pending: None,
            index: 0,
            message: String::new(),
        }
    }
}
impl Images {
    pub fn insert(&mut self, key: String, result: Result<Decoded, String>) {
        if self.pending.as_ref() == Some(&key) {
            self.pending = None;
        }
        if self.cache.len() >= 4 {
            self.cache.clear();
        }
        let loaded = result.map(|mut data| {
            let image = std::mem::replace(&mut data.image, DynamicImage::new_rgba8(0, 0));
            Loaded {
                protocol: RefCell::new(self.picker.new_resize_protocol(image)),
                data,
            }
        });
        self.cache.insert(key, loaded);
    }
}
pub fn is_image(a: &Value) -> bool {
    a["type"] == "img"
        || a["type"] == "image"
        || a["mimeType"]
            .as_str()
            .is_some_and(|m| m.starts_with("image/"))
}
pub fn decode(data: &str) -> Result<Decoded> {
    if data.len() > MAX_BYTES.div_ceil(3) * 4 {
        bail!("Image exceeds 20 MB")
    }
    let bytes = URL_SAFE_NO_PAD
        .decode(data)
        .or_else(|_| STANDARD.decode(data))
        .context("Invalid image attachment encoding")?;
    if bytes.len() > MAX_BYTES {
        bail!("Image exceeds 20 MB")
    }
    let format = image::guess_format(&bytes).context("This attachment is not a supported image")?;
    let (width, height) = ImageReader::with_format(Cursor::new(&bytes), format)
        .into_dimensions()
        .context("Could not read image dimensions")?;
    if u64::from(width) * u64::from(height) > 24_000_000 {
        bail!("Image exceeds 24 megapixels");
    }
    let mut reader = ImageReader::with_format(Cursor::new(&bytes), format);
    let mut limits = image::Limits::default();
    limits.max_image_width = Some(16384);
    limits.max_image_height = Some(16384);
    limits.max_alloc = Some(96 * 1024 * 1024);
    reader.limits(limits);
    let image = reader
        .decode()
        .context("Could not decode image (PNG, JPEG, GIF, WebP and BMP supported)")?;
    let dimensions = (image.width(), image.height());
    let image = if image.width() > 2048 || image.height() > 2048 {
        image.thumbnail(2048, 2048)
    } else {
        image
    };
    Ok(Decoded {
        bytes,
        image,
        extension: format.extensions_str().first().copied().unwrap_or("img"),
        dimensions,
    })
}
pub fn save(data: &Decoded, name: &str) -> Result<PathBuf> {
    save_bytes(&data.bytes, name, data.extension)
}
pub fn save_bytes(bytes: &[u8], name: &str, extension: &str) -> Result<PathBuf> {
    use std::io::Write;
    let safe_extension: String = extension
        .chars()
        .filter(char::is_ascii_alphanumeric)
        .take(12)
        .collect();
    let extension = if safe_extension.is_empty() {
        "bin"
    } else {
        &safe_extension
    };
    let root = PathBuf::from(std::env::var_os("CHORD_DOWNLOAD_DIR").unwrap_or_else(|| {
        PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("private/downloads")
            .into_os_string()
    }));
    std::fs::create_dir_all(&root)?;
    let stem: String = std::path::Path::new(name)
        .file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or("image")
        .chars()
        .filter(|c| c.is_alphanumeric() || matches!(c, '-' | '_' | ' '))
        .take(80)
        .collect();
    let stem = if stem.is_empty() { "image" } else { &stem };
    for i in 0..10000 {
        let suffix = if i == 0 {
            String::new()
        } else {
            format!("-{i}")
        };
        let file = root.join(format!("{stem}{suffix}.{extension}"));
        let mut options = std::fs::OpenOptions::new();
        options.write(true).create_new(true);
        #[cfg(unix)]
        {
            use std::os::unix::fs::OpenOptionsExt;
            options.mode(0o600);
        }
        match options.open(&file) {
            Ok(mut f) => {
                f.write_all(bytes)?;
                return Ok(file);
            }
            Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => continue,
            Err(e) => return Err(e.into()),
        }
    }
    bail!("Too many files with this name")
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn decode_sniffs_and_limits_untrusted_media() {
        let image = DynamicImage::new_rgb8(40, 20);
        let mut bytes = Cursor::new(Vec::new());
        image.write_to(&mut bytes, image::ImageFormat::Png).unwrap();
        for encoded in [
            STANDARD.encode(bytes.get_ref()),
            URL_SAFE_NO_PAD.encode(bytes.get_ref()),
        ] {
            let decoded = decode(&encoded).unwrap();
            assert_eq!(decoded.dimensions, (40, 20));
            assert_eq!(decoded.extension, "png");
        }
        for format in [
            image::ImageFormat::Jpeg,
            image::ImageFormat::WebP,
            image::ImageFormat::Gif,
            image::ImageFormat::Bmp,
        ] {
            let mut encoded = Cursor::new(Vec::new());
            image.write_to(&mut encoded, format).unwrap();
            assert_eq!(
                decode(&STANDARD.encode(encoded.get_ref()))
                    .unwrap()
                    .dimensions,
                (40, 20)
            );
        }
        assert!(decode(&STANDARD.encode(b"<html>not an image</html>")).is_err());
        assert!(decode("!!!").is_err());
        assert!(decode(&"A".repeat(MAX_BYTES.div_ceil(3) * 4 + 1)).is_err());
    }
}
