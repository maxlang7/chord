use eframe::egui::{self, Color32, Pos2, Rect, Stroke, Vec2};

// Vector service marks stay sharp at both sidebar sizes and need no remote assets.
pub fn service(p: &egui::Painter, rect: Rect, source: usize, ink: Color32) {
    let pt = |x: f32, y: f32| {
        Pos2::new(
            rect.left() + x * rect.width(),
            rect.top() + y * rect.height(),
        )
    };
    let line = |a: (f32, f32), b: (f32, f32), color: Color32, width: f32| {
        p.line_segment([pt(a.0, a.1), pt(b.0, b.1)], Stroke::new(width, color));
    };
    match source {
        1 => {
            for (a, b, c) in [
                ((0.36, 0.12), (0.36, 0.65), Color32::from_rgb(54, 197, 240)),
                ((0.12, 0.36), (0.65, 0.36), Color32::from_rgb(224, 30, 90)),
                ((0.65, 0.36), (0.65, 0.89), Color32::from_rgb(236, 178, 46)),
                ((0.36, 0.65), (0.89, 0.65), Color32::from_rgb(46, 182, 125)),
            ] {
                line(a, b, c, 4.);
            }
        }
        2 => {
            let c = Color32::from_rgb(128, 142, 255);
            p.add(egui::Shape::convex_polygon(
                vec![
                    pt(0.2, 0.26),
                    pt(0.8, 0.26),
                    pt(0.94, 0.76),
                    pt(0.72, 0.86),
                    pt(0.65, 0.74),
                    pt(0.35, 0.74),
                    pt(0.28, 0.86),
                    pt(0.06, 0.76),
                ],
                c,
                Stroke::NONE,
            ));
            for x in [0.34, 0.66] {
                p.circle_filled(pt(x, 0.53), 2.3, Color32::from_rgb(35, 38, 59));
            }
            line((0.32, 0.2), (0.68, 0.2), c, 2.);
        }
        3 => {
            let c = Color32::from_rgb(37, 190, 100);
            p.circle_stroke(pt(0.5, 0.46), rect.width() * 0.38, Stroke::new(2.0_f32, c));
            line((0.21, 0.72), (0.12, 0.94), c, 2.);
            line((0.12, 0.94), (0.39, 0.82), c, 2.);
            p.add(egui::Shape::line(
                vec![
                    pt(0.34, 0.28),
                    pt(0.32, 0.43),
                    pt(0.5, 0.62),
                    pt(0.67, 0.64),
                ],
                Stroke::new(3.0_f32, c),
            ));
        }
        4 => {
            p.rect_stroke(
                Rect::from_two_pos(pt(0.25, 0.03), pt(0.75, 0.97)),
                4.,
                Stroke::new(2.0_f32, ink),
                egui::StrokeKind::Inside,
            );
            line((0.42, 0.13), (0.58, 0.13), ink, 2.);
            p.circle_filled(pt(0.5, 0.85), 1.2, ink);
        }
        5 | 6 => {
            let c = if source == 6 {
                Color32::from_rgb(224, 91, 85)
            } else {
                Color32::from_rgb(230, 174, 82)
            };
            p.rect_stroke(
                Rect::from_two_pos(pt(0.07, 0.2), pt(0.93, 0.82)),
                3.,
                Stroke::new(2.0_f32, c),
                egui::StrokeKind::Inside,
            );
            line((0.08, 0.23), (0.5, 0.55), c, 2.);
            line((0.5, 0.55), (0.92, 0.23), c, 2.);
        }
        _ => {
            p.rect_stroke(
                Rect::from_two_pos(pt(0.1, 0.22), pt(0.9, 0.85)),
                3.,
                Stroke::new(2.0_f32, ink),
                egui::StrokeKind::Inside,
            );
            line((0.1, 0.53), (0.35, 0.53), ink, 2.);
            line((0.35, 0.53), (0.43, 0.65), ink, 2.);
            line((0.43, 0.65), (0.57, 0.65), ink, 2.);
            line((0.57, 0.65), (0.65, 0.53), ink, 2.);
            line((0.65, 0.53), (0.9, 0.53), ink, 2.);
        }
    }
}

pub fn button(
    ui: &mut egui::Ui,
    source: usize,
    label: &str,
    compact: bool,
    selected: bool,
    p: super::Palette,
) -> egui::Response {
    let (rect, response) =
        ui.allocate_exact_size(Vec2::new(ui.available_width(), 38.), egui::Sense::click());
    if selected || response.hovered() {
        ui.painter().rect_filled(
            rect,
            9.,
            if selected {
                super::theme::tint(p.panel, super::theme::service_color(source), 0.18)
            } else {
                p.panel
            },
        );
    }
    let origin = if compact {
        rect.center() - Vec2::splat(12.)
    } else {
        egui::pos2(rect.left() + 10., rect.center().y - 12.)
    };
    service(
        ui.painter(),
        Rect::from_min_size(origin, Vec2::splat(24.)),
        source,
        if selected {
            super::theme::service_color(source)
        } else {
            p.text
        },
    );
    if !compact {
        ui.painter().text(
            egui::pos2(rect.left() + 44., rect.center().y),
            egui::Align2::LEFT_CENTER,
            label,
            egui::FontId::proportional(14.),
            if selected {
                super::theme::service_color(source)
            } else {
                p.text
            },
        );
    }
    response.widget_info(|| {
        egui::WidgetInfo::selected(egui::WidgetType::Button, ui.is_enabled(), selected, label)
    });
    response.on_hover_text(label)
}

/// Paint navigation arrows directly so they do not depend on font fallback.
pub fn back_button(ui: &mut egui::Ui, label: &str) -> egui::Response {
    let response = ui.button(format!("    {label}"));
    if ui.is_rect_visible(response.rect) {
        let color = ui.style().interact(&response).text_color();
        let tip = egui::pos2(response.rect.left() + 8., response.rect.center().y);
        let stroke = Stroke::new(1.5_f32, color);
        ui.painter()
            .line_segment([tip, tip + Vec2::new(9., 0.)], stroke);
        ui.painter().add(egui::Shape::line(
            vec![tip + Vec2::new(4., -4.), tip, tip + Vec2::new(4., 4.)],
            stroke,
        ));
    }
    response
}

pub enum GalleryIcon {
    Download,
    External,
    Close,
    Previous,
    Next,
}

pub fn gallery_button(ui: &mut egui::Ui, icon: GalleryIcon, label: &str) -> egui::Response {
    let (rect, response) = ui.allocate_exact_size(Vec2::splat(36.), egui::Sense::click());
    let ink = if ui.is_enabled() {
        Color32::from_gray(225)
    } else {
        Color32::from_gray(90)
    };
    if response.hovered() {
        ui.painter()
            .rect_filled(rect, 6., Color32::from_white_alpha(22));
    }
    let r = Rect::from_center_size(rect.center(), Vec2::splat(20.));
    let pt = |x: f32, y: f32| r.min + Vec2::new(x * r.width(), y * r.height());
    let line = |a: (f32, f32), b: (f32, f32)| {
        ui.painter()
            .line_segment([pt(a.0, a.1), pt(b.0, b.1)], Stroke::new(1.8_f32, ink));
    };
    match icon {
        GalleryIcon::Close => {
            line((0.2, 0.2), (0.8, 0.8));
            line((0.8, 0.2), (0.2, 0.8));
        }
        GalleryIcon::Download => {
            line((0.5, 0.05), (0.5, 0.65));
            line((0.25, 0.4), (0.5, 0.65));
            line((0.75, 0.4), (0.5, 0.65));
            line((0.1, 0.65), (0.1, 0.9));
            line((0.1, 0.9), (0.9, 0.9));
            line((0.9, 0.9), (0.9, 0.65));
        }
        GalleryIcon::External => {
            line((0.5, 0.1), (0.9, 0.1));
            line((0.9, 0.1), (0.9, 0.5));
            line((0.9, 0.1), (0.4, 0.6));
            line((0.35, 0.2), (0.1, 0.2));
            line((0.1, 0.2), (0.1, 0.9));
            line((0.1, 0.9), (0.8, 0.9));
            line((0.8, 0.9), (0.8, 0.65));
        }
        GalleryIcon::Previous => {
            line((0.65, 0.15), (0.3, 0.5));
            line((0.3, 0.5), (0.65, 0.85));
        }
        GalleryIcon::Next => {
            line((0.35, 0.15), (0.7, 0.5));
            line((0.7, 0.5), (0.35, 0.85));
        }
    }
    response
        .on_hover_text(label)
        .on_hover_cursor(egui::CursorIcon::PointingHand)
}
