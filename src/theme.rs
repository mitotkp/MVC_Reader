//! Colores y estilo visual de MVCreader (temas claro, sepia y oscuro).

use eframe::egui::{self, Color32, CornerRadius, Shadow, Stroke, Theme, Visuals, vec2};

use crate::store::ThemeKind;

pub const HL_COLORS: [(u8, u8, u8); 4] = [(255, 205, 0), (100, 205, 80), (80, 160, 255), (255, 100, 175)];

pub struct Palette {
    /// Fondo detrás de la página.
    pub desk: Color32,
    pub paper: Color32,
    pub text: Color32,
    pub weak: Color32,
    pub rule: Color32,
    pub shadow: Color32,
    pub accent: Color32,
    /// Fondo suave para tarjetas y grupos de botones.
    pub card: Color32,
    pub card_stroke: Color32,
}

pub fn palette(t: ThemeKind) -> Palette {
    match t {
        ThemeKind::Light => Palette {
            desk: Color32::from_rgb(233, 234, 240),
            paper: Color32::WHITE,
            text: Color32::from_rgb(28, 28, 32),
            weak: Color32::from_rgb(130, 132, 145),
            rule: Color32::from_rgb(200, 202, 212),
            shadow: Color32::from_black_alpha(26),
            accent: Color32::from_rgb(79, 70, 229),
            card: Color32::from_rgb(255, 255, 255),
            card_stroke: Color32::from_rgb(226, 228, 236),
        },
        ThemeKind::Sepia => Palette {
            desk: Color32::from_rgb(226, 214, 190),
            paper: Color32::from_rgb(249, 241, 222),
            text: Color32::from_rgb(76, 58, 40),
            weak: Color32::from_rgb(150, 128, 100),
            rule: Color32::from_rgb(210, 192, 160),
            shadow: Color32::from_black_alpha(30),
            accent: Color32::from_rgb(166, 98, 42),
            card: Color32::from_rgb(250, 244, 230),
            card_stroke: Color32::from_rgb(222, 206, 176),
        },
        ThemeKind::Dark => Palette {
            desk: Color32::from_rgb(17, 17, 20),
            paper: Color32::from_rgb(34, 34, 38),
            text: Color32::from_rgb(216, 216, 212),
            weak: Color32::from_rgb(128, 128, 136),
            rule: Color32::from_rgb(70, 70, 78),
            shadow: Color32::from_black_alpha(100),
            accent: Color32::from_rgb(129, 140, 248),
            card: Color32::from_rgb(36, 36, 42),
            card_stroke: Color32::from_rgb(52, 52, 60),
        },
    }
}

pub fn hl_color(i: usize, theme: ThemeKind) -> Color32 {
    let (r, g, b) = HL_COLORS[i.min(3)];
    let a = if theme == ThemeKind::Dark { 85 } else { 105 };
    Color32::from_rgba_unmultiplied(r, g, b, a)
}

pub fn solid(i: usize) -> Color32 {
    let (r, g, b) = HL_COLORS[i.min(3)];
    Color32::from_rgb(r, g, b)
}

/// Color de la etiqueta de cada formato.
pub fn kind_color(kind: &str) -> Color32 {
    match kind {
        "PDF" => Color32::from_rgb(220, 72, 64),
        "EPUB" => Color32::from_rgb(46, 160, 98),
        _ => Color32::from_rgb(45, 120, 215),
    }
}

/// Aplica el tema a egui: colores, esquinas redondeadas y espaciado más amplio.
pub fn apply(ctx: &egui::Context, t: ThemeKind) {
    let pal = palette(t);
    let (theme, mut v) = match t {
        ThemeKind::Dark => (Theme::Dark, Visuals::dark()),
        _ => (Theme::Light, Visuals::light()),
    };
    match t {
        ThemeKind::Light => {
            v.panel_fill = Color32::from_rgb(246, 246, 250);
            v.window_fill = Color32::WHITE;
            v.extreme_bg_color = Color32::WHITE;
            v.faint_bg_color = Color32::from_rgb(238, 239, 245);
        }
        ThemeKind::Sepia => {
            v.panel_fill = Color32::from_rgb(240, 231, 211);
            v.window_fill = Color32::from_rgb(247, 240, 224);
            v.extreme_bg_color = Color32::from_rgb(252, 248, 238);
            v.faint_bg_color = Color32::from_rgb(234, 224, 202);
        }
        ThemeKind::Dark => {
            v.panel_fill = Color32::from_rgb(26, 26, 30);
            v.window_fill = Color32::from_rgb(32, 32, 37);
            v.extreme_bg_color = Color32::from_rgb(20, 20, 24);
            v.faint_bg_color = Color32::from_rgb(36, 36, 42);
        }
    }
    let radius = CornerRadius::same(7);
    for w in [
        &mut v.widgets.noninteractive,
        &mut v.widgets.inactive,
        &mut v.widgets.hovered,
        &mut v.widgets.active,
        &mut v.widgets.open,
    ] {
        w.corner_radius = radius;
    }
    v.widgets.noninteractive.bg_stroke = Stroke::new(1.0, pal.card_stroke);
    v.widgets.hovered.expansion = 0.0;
    v.widgets.active.expansion = 0.0;
    v.widgets.hovered.bg_stroke = Stroke::new(1.0, pal.accent.gamma_multiply(0.5));
    v.selection.bg_fill = pal.accent.gamma_multiply(if t == ThemeKind::Dark { 0.55 } else { 0.85 });
    v.selection.stroke = Stroke::new(1.0, Color32::WHITE);
    v.hyperlink_color = pal.accent;
    v.window_corner_radius = CornerRadius::same(12);
    v.menu_corner_radius = CornerRadius::same(10);
    v.window_stroke = Stroke::new(1.0, pal.card_stroke);
    v.window_shadow = Shadow { offset: [0, 8], blur: 28, spread: 0, color: Color32::from_black_alpha(if t == ThemeKind::Dark { 120 } else { 40 }) };
    v.popup_shadow = Shadow { offset: [0, 6], blur: 20, spread: 0, color: Color32::from_black_alpha(if t == ThemeKind::Dark { 120 } else { 36 }) };
    v.slider_trailing_fill = true;
    ctx.set_visuals_of(theme, v);
    ctx.set_theme(theme);
    ctx.all_styles_mut(|s| {
        s.spacing.item_spacing = vec2(8.0, 6.0);
        s.spacing.button_padding = vec2(9.0, 4.0);
        s.spacing.interact_size.y = 26.0;
        s.spacing.slider_width = 170.0;
    });
}
