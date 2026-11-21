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
    // Background of the selected conversation row.
    pub select: Color32,
}
impl Theme {
    pub fn palette(self) -> Palette {
        let rgb = |r, g, b| Color32::from_rgb(r, g, b);
        match self {
            Self::Gruvbox => Palette {
                bg: rgb(29, 32, 33),
                panel: rgb(60, 56, 54),
                raised: rgb(102, 92, 84),
                text: rgb(251, 241, 199),
                muted: rgb(189, 174, 147),
                line: rgb(124, 111, 100),
                accent: rgb(254, 128, 25),
                blue: rgb(137, 180, 250),
                own: rgb(72, 52, 36),
                select: rgb(102, 92, 84),
            },
            // macOS system grays and accent blue (dark appearance).
            Self::Dark => Palette {
                bg: rgb(30, 30, 30),
                panel: rgb(40, 40, 42),
                raised: rgb(58, 58, 60),
                text: rgb(245, 245, 247),
                muted: rgb(152, 152, 157),
                line: rgb(56, 56, 58),
                accent: rgb(10, 132, 255),
                blue: rgb(10, 132, 255),
                own: rgb(28, 52, 84),
                select: rgb(38, 64, 102),
            },
            // macOS system grays and accent blue (light appearance).
            Self::Light => Palette {
                bg: rgb(245, 245, 247),
                panel: Color32::WHITE,
                raised: rgb(232, 232, 237),
                text: rgb(29, 29, 31),
                muted: rgb(110, 110, 115),
                line: rgb(222, 222, 226),
                accent: rgb(0, 122, 255),
                blue: rgb(0, 122, 255),
                own: rgb(229, 239, 255),
                select: rgb(214, 228, 255),
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
        // Text fields read as white (or raised, in dark themes) wells with a hairline border.
        visuals.extreme_bg_color = p.panel;
        visuals.faint_bg_color = p.panel;
        visuals.selection.bg_fill = p.raised;
        visuals.selection.stroke = egui::Stroke::new(1.0_f32, p.accent);
        visuals.hyperlink_color = p.blue;
        visuals.widgets.noninteractive.bg_fill = p.panel;
        visuals.widgets.noninteractive.bg_stroke = egui::Stroke::new(1.0_f32, p.line);
        visuals.widgets.inactive.bg_fill = p.raised;
        visuals.widgets.inactive.weak_bg_fill = p.raised;
        visuals.widgets.inactive.bg_stroke = egui::Stroke::new(0.5_f32, p.line);
        // Mac controls darken slightly on hover instead of gaining a colored outline.
        let hover = tint(p.raised, p.line, 0.5);
        visuals.widgets.hovered.bg_fill = hover;
        visuals.widgets.hovered.weak_bg_fill = hover;
        visuals.widgets.hovered.bg_stroke = egui::Stroke::new(0.5_f32, p.line);
        visuals.widgets.active.bg_stroke = egui::Stroke::new(0.5_f32, p.line);
        for widget in [
            &mut visuals.widgets.noninteractive,
            &mut visuals.widgets.inactive,
            &mut visuals.widgets.hovered,
            &mut visuals.widgets.active,
            &mut visuals.widgets.open,
        ] {
            widget.corner_radius = 6.into();
        }
        visuals.window_corner_radius = 12.into();
        ctx.set_visuals(visuals);
        ctx.style_mut(|s| {
            s.spacing.item_spacing = egui::vec2(8., 7.);
            s.spacing.button_padding = egui::vec2(10., 5.);
            s.spacing.window_margin = egui::Margin::same(20);
            s.text_styles
                .insert(TextStyle::Body, FontId::proportional(14.));
            s.text_styles
                .insert(TextStyle::Button, FontId::proportional(13.));
            s.text_styles
                .insert(TextStyle::Small, FontId::proportional(11.5));
            s.text_styles
                .insert(TextStyle::Heading, FontId::proportional(21.));
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

/// Sidebar color for one connected account. Slack workspaces are separate
/// accounts: `slack:T…` picks a stable color that stays off the other networks.
pub fn account_color(account_key: &str) -> Color32 {
    if let Some(team) = account_key.strip_prefix("slack:") {
        if !team.is_empty() {
            return slack_workspace_color(team);
        }
    }
    let index = crate::model::SOURCES
        .iter()
        .position(|(id, _, _)| *id == account_key)
        .unwrap_or(0);
    service_color(index)
}

fn slack_workspace_color(team: &str) -> Color32 {
    // Kept away from Discord blue, WhatsApp green, iMessage cyan, and both mails.
    const PALETTE: &[(u8, u8, u8)] = &[
        (176, 98, 214),
        (214, 112, 72),
        (214, 86, 140),
        (120, 112, 196),
        (196, 92, 72),
        (156, 108, 188),
    ];
    let hash = team.bytes().fold(0_u32, |hash, byte| {
        hash.wrapping_mul(31).wrapping_add(u32::from(byte))
    });
    let (r, g, b) = PALETTE[(hash as usize) % PALETTE.len()];
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn gruvbox_keeps_surfaces_and_accents_apart() {
        let p = Theme::Gruvbox.palette();
        let dist = |a: Color32, b: Color32| {
            u16::from(a.r().abs_diff(b.r()))
                + u16::from(a.g().abs_diff(b.g()))
                + u16::from(a.b().abs_diff(b.b()))
        };
        assert!(dist(p.bg, p.panel) >= 60, "background and panels");
        assert!(dist(p.panel, p.raised) >= 40, "panels and raised controls");
        assert!(dist(p.bg, p.own) >= 40, "thread and your messages");
        assert!(dist(p.text, p.muted) >= 80, "primary and secondary text");
        assert!(dist(p.panel, p.line) >= 80, "panels and separators");
        assert!(dist(p.accent, p.blue) >= 150, "accent and links");
    }
    #[test]
    fn each_account_has_its_own_color_and_slack_workspaces_differ() {
        let lab = account_color("slack:TEMMBGZU3");
        let course = account_color("slack:TSAU3JLKZ");
        assert_ne!(lab, course);
        for key in [
            "slack",
            "discord",
            "whatsapp",
            "iphone",
            "mail:home",
            "mail:cmu",
        ] {
            let color = account_color(key);
            assert_ne!(lab, color, "{key}");
            assert_ne!(course, color, "{key}");
        }
        assert_eq!(account_color("slack"), service_color(1));
        assert_eq!(account_color("discord"), service_color(2));
        assert_eq!(account_color("mail:home"), service_color(5));
        assert_eq!(account_color("mail:cmu"), service_color(6));
        assert_ne!(account_color("mail:home"), account_color("mail:cmu"));
    }
}
