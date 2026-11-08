use crate::{
    api::Api,
    images::{self, Decoded, Target},
};
use eframe::egui::{self, TextureHandle};
use std::{collections::HashMap, time::Instant};
use tokio::sync::mpsc::{UnboundedReceiver, UnboundedSender, unbounded_channel};

pub enum Request {
    Image(Target),
    Avatar(String),
}
impl Request {
    pub fn key(&self) -> String {
        match self {
            Self::Image(t) => t.key(),
            Self::Avatar(id) => format!("avatar:{id}"),
        }
    }
}
pub enum Entry {
    Loading,
    Ready {
        texture: TextureHandle,
        data: Decoded,
        used: Instant,
    },
    Failed(String),
}
pub enum Event {
    Media(String, Box<Result<Decoded, String>>),
    File(std::path::PathBuf),
    Notice(String),
    Search(u64, Result<serde_json::Value, String>),
}
pub struct Media {
    pub cache: HashMap<String, Entry>,
    pub tx: UnboundedSender<Event>,
    rx: UnboundedReceiver<Event>,
    pending: usize,
}
impl Default for Media {
    fn default() -> Self {
        let (tx, rx) = unbounded_channel();
        Self {
            cache: HashMap::new(),
            tx,
            rx,
            pending: 0,
        }
    }
}
impl Media {
    pub fn request(&mut self, request: Request, api: Option<Api>, ctx: &egui::Context) {
        let key = request.key();
        if self.cache.contains_key(&key) || self.pending >= 4 {
            return;
        }
        let Some(api) = api else { return };
        self.cache.insert(key.clone(), Entry::Loading);
        self.pending += 1;
        let tx = self.tx.clone();
        let ctx = ctx.clone();
        tokio::spawn(async move {
            let result=async{
    let value=match request {
     Request::Image(t)=>api.post(&Api::conversation(&t.chat,"attachment"),serde_json::json!({"messageID":t.message,"index":t.index,"attachmentID":t.attachment["id"]})).await?,
     Request::Avatar(id)=>api.get(&format!("/api/contacts/{}/avatar",urlencoding::encode(&id))).await?,
    };
    let encoded=value["data"].as_str().ok_or_else(||anyhow::anyhow!("No image data"))?.to_owned();
    tokio::task::spawn_blocking(move||images::decode(&encoded)).await?
   }.await.map_err(|e:anyhow::Error|format!("{e:#}"));
            let _ = tx.send(Event::Media(key, Box::new(result)));
            ctx.request_repaint();
        });
    }
    pub fn poll(&mut self, ctx: &egui::Context) -> Vec<Event> {
        let mut others = vec![];
        while let Ok(event) = self.rx.try_recv() {
            match event {
                Event::Media(key, result) => {
                    self.pending = self.pending.saturating_sub(1);
                    match *result {
                        Ok(mut data) => {
                            let avatar = key.starts_with("avatar:");
                            let img = if avatar {
                                data.image.thumbnail(128, 128)
                            } else {
                                data.image.thumbnail(1440, 1440)
                            };
                            let rgba = img.to_rgba8();
                            let texture = ctx.load_texture(
                                &key,
                                egui::ColorImage::from_rgba_unmultiplied(
                                    [rgba.width() as usize, rgba.height() as usize],
                                    rgba.as_raw(),
                                ),
                                egui::TextureOptions::LINEAR,
                            );
                            data.image = image::DynamicImage::new_rgba8(0, 0);
                            self.cache.insert(
                                key,
                                Entry::Ready {
                                    texture,
                                    data,
                                    used: Instant::now(),
                                },
                            );
                        }
                        Err(error) => {
                            self.cache.insert(key, Entry::Failed(error));
                        }
                    }
                }
                event => others.push(event),
            }
        }
        // Bound original-file and texture storage, retaining the most recently shown media.
        let mut bytes: usize = self
            .cache
            .values()
            .map(|e| match e {
                Entry::Ready { data, .. } => data.bytes.len(),
                _ => 0,
            })
            .sum();
        while bytes > 48 * 1024 * 1024 || self.cache.len() > 96 {
            let oldest = self
                .cache
                .iter()
                .filter_map(|(k, v)| match v {
                    Entry::Ready { used, data, .. } => Some((k.clone(), *used, data.bytes.len())),
                    _ => None,
                })
                .min_by_key(|(_, used, _)| *used);
            let Some((key, _, size)) = oldest else { break };
            self.cache.remove(&key);
            bytes = bytes.saturating_sub(size);
        }
        others
    }
    pub fn texture(&mut self, key: &str) -> Option<TextureHandle> {
        match self.cache.get_mut(key) {
            Some(Entry::Ready { texture, used, .. }) => {
                *used = Instant::now();
                Some(texture.clone())
            }
            _ => None,
        }
    }
    pub fn download_file(&self, target: Target, api: Option<Api>, ctx: &egui::Context) {
        let Some(api) = api else {
            return;
        };
        let tx = self.tx.clone();
        let ctx = ctx.clone();
        tokio::spawn(async move {
            use base64::Engine;
            let result = async {
                let value = api.post(&Api::conversation(&target.chat, "attachment"), serde_json::json!({"messageID":target.message,"index":target.index,"attachmentID":target.attachment["id"]})).await?;
                let encoded = value["data"].as_str().ok_or_else(|| anyhow::anyhow!("Attachment has no file data"))?;
                if encoded.len() > images::MAX_BYTES * 4 / 3 + 8 { anyhow::bail!("Attachment exceeds 20 MB"); }
                let bytes = base64::engine::general_purpose::URL_SAFE_NO_PAD.decode(encoded).or_else(|_| base64::engine::general_purpose::STANDARD.decode(encoded))?;
                if bytes.len() > images::MAX_BYTES { anyhow::bail!("Attachment exceeds 20 MB"); }
                let name = value["fileName"].as_str().unwrap_or_else(|| target.name());
                let extension = std::path::Path::new(name).extension().and_then(|s| s.to_str()).unwrap_or("bin");
                images::save_bytes(&bytes, name, extension)
            }.await;
            let notice = match result {
                Ok(file) => format!("Saved {}", file.display()),
                Err(e) => format!("Download failed: {e:#}"),
            };
            let _ = tx.send(Event::Notice(notice));
            ctx.request_repaint();
        });
    }
    pub fn save(&self, target: &Target, brave: bool) -> Result<std::path::PathBuf, String> {
        let Some(Entry::Ready { data, .. }) = self.cache.get(&target.key()) else {
            return Err("Image is still loading".into());
        };
        let file = images::save(data, target.name()).map_err(|e| e.to_string())?;
        if brave {
            std::process::Command::new("brave")
                .arg(&file)
                .stdin(std::process::Stdio::null())
                .stdout(std::process::Stdio::null())
                .stderr(std::process::Stdio::null())
                .spawn()
                .map_err(|e| e.to_string())?;
        }
        Ok(file)
    }
}
