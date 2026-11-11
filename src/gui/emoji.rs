//! Offline Twemoji graphics (CC BY 4.0), packed by scripts/build-emoji-atlas.py.
use eframe::egui::{self, Color32, FontFamily, FontId, Rect, TextureHandle, Vec2};
use std::{collections::HashMap, sync::OnceLock};
use unicode_segmentation::UnicodeSegmentation;

fn index() -> &'static HashMap<String, usize> {
    static INDEX: OnceLock<HashMap<String, usize>> = OnceLock::new();
    INDEX.get_or_init(|| {
        serde_json::from_str(include_str!("../../assets/emoji-atlas.json"))
            .expect("bundled emoji index")
    })
}
fn lookup(emoji: &str) -> Option<usize> {
    let first = emoji.chars().next()?;
    if first < '\u{2000}' && first != '©' && first != '®' && !emoji.contains('\u{20e3}') {
        return None;
    }
    let key = emoji
        .chars()
        .map(|c| format!("{:x}", c as u32))
        .collect::<Vec<_>>()
        .join("-");
    index()
        .get(&key)
        .or_else(|| {
            index().get(
                &key.split('-')
                    .filter(|part| *part != "fe0f")
                    .collect::<Vec<_>>()
                    .join("-"),
            )
        })
        .copied()
}
fn texture(ctx: &egui::Context) -> TextureHandle {
    let id = egui::Id::new("color-emoji-atlas");
    if let Some(texture) = ctx.data(|data| data.get_temp::<TextureHandle>(id)) {
        return texture;
    }
    let image = image::load_from_memory(include_bytes!("../../assets/emoji-atlas.png"))
        .expect("bundled emoji atlas")
        .to_rgba8();
    let texture = ctx.load_texture(
        "color-emoji-atlas",
        egui::ColorImage::from_rgba_unmultiplied(
            [image.width() as usize, image.height() as usize],
            image.as_raw(),
        ),
        egui::TextureOptions::LINEAR,
    );
    ctx.data_mut(|data| data.insert_temp(id, texture.clone()));
    texture
}
fn uv(index: usize, texture: &TextureHandle) -> Rect {
    let cell = Vec2::splat(32.) / texture.size_vec2();
    let min = egui::vec2((index % 64) as f32, (index / 64) as f32) * cell;
    Rect::from_min_size(egui::pos2(min.x, min.y), cell)
}

// Retain original Unicode for selection, copying, and link hit-testing. The
// leading glyph reserves a square; invisible continuation glyphs keep ZWJ,
// flags, and skin-tone sequences together without taking extra visible space.
pub fn append(job: &mut egui::text::LayoutJob, text: &str, format: egui::TextFormat) {
    for grapheme in text.graphemes(true) {
        if lookup(grapheme).is_some() {
            let first_len = grapheme.chars().next().unwrap().len_utf8();
            let mut head = format.clone();
            head.font_id = FontId::new(format.font_id.size, FontFamily::Name("emoji".into()));
            head.color = Color32::TRANSPARENT;
            head.underline = egui::Stroke::NONE;
            job.append(&grapheme[..first_len], 0., head.clone());
            if first_len < grapheme.len() {
                head.font_id.size = 0.01;
                job.append(&grapheme[first_len..], 0., head);
            }
        } else {
            job.append(grapheme, 0., format.clone());
        }
    }
}
pub fn paint(ui: &egui::Ui, galley: &egui::Galley, origin: egui::Pos2) {
    paint_clipped(ui, galley, origin, ui.clip_rect());
}
pub fn paint_clipped(ui: &egui::Ui, galley: &egui::Galley, origin: egui::Pos2, clip: Rect) {
    let mut character = 0;
    for grapheme in galley.job.text.graphemes(true) {
        let visible_end = galley
            .end()
            .index
            .saturating_sub(usize::from(galley.elided));
        if character + grapheme.chars().count() > visible_end {
            break;
        }
        if let Some(index) = lookup(grapheme) {
            let cursor = galley.pos_from_cursor(egui::text::CCursor::new(character));
            let rect = cursor.translate(origin.to_vec2());
            let next = galley.pos_from_cursor(egui::text::CCursor::new(character + 1));
            let advance = next.left() - cursor.left();
            let size = if advance > 1. {
                rect.height().min(advance)
            } else {
                rect.height()
            }
            .max(1.);
            let target = Rect::from_min_size(
                egui::pos2(rect.left(), rect.center().y - size / 2.),
                Vec2::splat(size),
            );
            if ui.is_rect_visible(target) {
                let texture = texture(ui.ctx());
                ui.painter().with_clip_rect(clip).image(
                    texture.id(),
                    target,
                    uv(index, &texture),
                    Color32::WHITE,
                );
            }
        }
        character += grapheme.chars().count();
    }
}
pub fn button(ui: &egui::Ui, key: &str, label: &str) -> egui::Button<'static> {
    if let Some(index) = lookup(key) {
        let texture = texture(ui.ctx());
        let image = egui::Image::new((texture.id(), Vec2::splat(22.))).uv(uv(index, &texture));
        egui::Button::image_and_text(image, label.to_owned())
    } else {
        egui::Button::new(format!("{key} {label}"))
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn complete_sequences_and_new_emoji_are_bundled() {
        for emoji in ["👍🏽", "❤️", "🇺🇸", "👩‍💻", "👨‍👩‍👧‍👦", "🫠", "🩷", "✅", "🎉"]
        {
            assert!(lookup(emoji).is_some(), "{emoji}");
        }
        assert!(lookup("A").is_none());
        let mut job = egui::text::LayoutJob::default();
        let text = "Hi 👍🏽 🇺🇸 👩‍💻!";
        append(&mut job, text, egui::TextFormat::default());
        assert_eq!(job.text, text);
    }
    #[test]
    fn atlas_contains_actual_color_pixels() {
        let image = image::load_from_memory(include_bytes!("../../assets/emoji-atlas.png"))
            .unwrap()
            .to_rgba8();
        let index = lookup("❤️").unwrap();
        let x = (index % 64) as u32 * 32;
        let y = (index / 64) as u32 * 32;
        assert!((x..x + 32).any(|px| (y..y + 32).any(|py| {
            let p = image.get_pixel(px, py);
            p[3] > 100 && p[0] > p[1].saturating_add(50)
        })));
    }
}
