use eframe::egui::{self, Color32, FontId, TextStyle};
#[derive(Clone, Copy, Default, PartialEq, Eq)]
pub enum Theme {
    #[default]
    Gruvbox,
    Dark,
    Light,
}
#[derive(Clone, Copy)]
pub struct Palette {
    pub bg: Color32,
    pub panel: Color32,
    pub raised: Color32,
    pub text: Color32,
    pub muted: Color32,
    pub line: Color32,
    pub accent: Color32,
    pub blue: Color32,
    pub own: Color32,
}
impl Theme {
    pub fn palette(self) -> Palette {
        let rgb = |r, g, b| Color32::from_rgb(r, g, b);
        match self {
            Self::Gruvbox => Palette {
                bg: rgb(29, 32, 33),
                panel: rgb(40, 40, 40),
                raised: rgb(60, 56, 54),
                text: rgb(235, 219, 178),
                muted: rgb(168, 153, 132),
                line: rgb(70, 66, 60),
                accent: rgb(91, 202, 177),
                blue: rgb(126, 174, 239),
                own: rgb(34, 60, 55),
            },
            Self::Dark => Palette {
                bg: rgb(22, 24, 28),
                panel: rgb(29, 32, 37),
                raised: rgb(44, 48, 56),
                text: rgb(233, 236, 242),
                muted: rgb(151, 160, 176),
                line: rgb(53, 59, 69),
                accent: rgb(137, 180, 250),
                blue: rgb(137, 180, 250),
                own: rgb(33, 46, 64),
            },
            Self::Light => Palette {
                bg: rgb(245, 246, 248),
                panel: Color32::WHITE,
                raised: rgb(232, 235, 241),
                text: rgb(35, 40, 49),
                muted: rgb(104, 113, 130),
                line: rgb(219, 224, 233),
                accent: rgb(69, 101, 166),
                blue: rgb(69, 101, 166),
                own: rgb(232, 239, 250),
            },
        }
    }
    pub fn apply(self, ctx: &egui::Context) {
        let p = self.palette();
        let mut visuals = if self == Self::Light {
            egui::Visuals::light()
        } else {
            egui::Visuals::dark()
        };
        visuals.override_text_color = Some(p.text);
        visuals.panel_fill = p.bg;
        visuals.window_fill = p.panel;
        visuals.extreme_bg_color = p.bg;
        visuals.faint_bg_color = p.panel;
        visuals.selection.bg_fill = p.raised;
        visuals.selection.stroke = egui::Stroke::new(1.0_f32, p.accent);
        visuals.hyperlink_color = p.blue;
        visuals.widgets.noninteractive.bg_fill = p.panel;
        visuals.widgets.noninteractive.bg_stroke = egui::Stroke::new(1.0_f32, p.line);
        visuals.widgets.inactive.bg_fill = p.raised;
        visuals.widgets.inactive.weak_bg_fill = p.raised;
        visuals.widgets.inactive.bg_stroke = egui::Stroke::NONE;
        visuals.widgets.hovered.bg_fill = p.raised;
        visuals.widgets.hovered.weak_bg_fill = p.raised;
        visuals.widgets.hovered.bg_stroke = egui::Stroke::new(1.0_f32, p.accent);
        visuals.window_corner_radius = 12.into();
        ctx.set_visuals(visuals);
        ctx.style_mut(|s| {
            s.spacing.item_spacing = egui::vec2(8., 8.);
            s.spacing.button_padding = egui::vec2(11., 7.);
            s.spacing.window_margin = egui::Margin::same(20);
            s.text_styles
                .insert(TextStyle::Body, FontId::proportional(15.));
            s.text_styles
                .insert(TextStyle::Button, FontId::proportional(14.));
            s.text_styles
                .insert(TextStyle::Small, FontId::proportional(12.));
            s.text_styles
                .insert(TextStyle::Heading, FontId::proportional(23.));
        });
    }
    pub fn name(self) -> &'static str {
        match self {
            Self::Gruvbox => "Gruvbox dark",
            Self::Dark => "Dark",
            Self::Light => "Light",
        }
    }
}

pub fn service_color(source: usize) -> Color32 {
    let (r, g, b) = match source {
        1 => (192, 139, 229),
        2 => (130, 148, 255),
        3 => (85, 199, 131),
        4 => (85, 195, 216),
        5 => (239, 180, 96),
        6 => (240, 133, 143),
        _ => (91, 202, 177),
    };
    Color32::from_rgb(r, g, b)
}
pub fn avatar_color(name: &str) -> Color32 {
    let palette = [
        (90, 155, 208),
        (157, 117, 211),
        (211, 116, 137),
        (199, 149, 73),
        (72, 167, 145),
        (90, 160, 170),
        (190, 118, 84),
        (111, 148, 207),
    ];
    let hash = name.bytes().fold(0_usize, |hash, byte| {
        hash.wrapping_mul(31).wrapping_add(byte as usize)
    });
    let (r, g, b) = palette[hash % palette.len()];
    Color32::from_rgb(r, g, b)
}
pub fn tint(base: Color32, accent: Color32, amount: f32) -> Color32 {
    let mix = |a: u8, b: u8| (a as f32 * (1. - amount) + b as f32 * amount) as u8;
    Color32::from_rgb(
        mix(base.r(), accent.r()),
        mix(base.g(), accent.g()),
        mix(base.b(), accent.b()),
    )
}
