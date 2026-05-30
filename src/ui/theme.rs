//! Custom egui theme and color palette tuned for K8S tooling (dark-first, status colors).

use egui::{Color32, Rounding, Stroke, Style, TextStyle, Visuals};

pub const K8S_BLUE: Color32 = Color32::from_rgb(50, 108, 229);
pub const K3S_PURPLE: Color32 = Color32::from_rgb(139, 92, 246);
pub const STATUS_RUNNING: Color32 = Color32::from_rgb(34, 197, 94);
pub const STATUS_PENDING: Color32 = Color32::from_rgb(234, 179, 8);
pub const STATUS_FAILED: Color32 = Color32::from_rgb(239, 68, 68);
pub const STATUS_SUCCEEDED: Color32 = Color32::from_rgb(100, 149, 237);

/// Applies a polished K8s/K3s-oriented dark theme.
pub fn apply_kube_theme(ctx: &egui::Context) {
    let mut style = (*ctx.style()).clone();
    let mut visuals = Visuals::dark();

    // Base colors
    visuals.window_fill = Color32::from_rgb(18, 20, 26);
    visuals.panel_fill = Color32::from_rgb(22, 24, 30);
    visuals.extreme_bg_color = Color32::from_rgb(12, 14, 18);

    // Accents
    visuals.selection.bg_fill = K8S_BLUE;
    visuals.hyperlink_color = K8S_BLUE;

    // Widgets
    visuals.widgets.noninteractive.bg_fill = Color32::from_rgb(28, 30, 36);
    visuals.widgets.inactive.bg_fill = Color32::from_rgb(36, 38, 46);
    visuals.widgets.hovered.bg_fill = Color32::from_rgb(48, 50, 60);
    visuals.widgets.active.bg_fill = K8S_BLUE;

    // Strokes
    visuals.widgets.noninteractive.bg_stroke = Stroke::new(1.0, Color32::from_rgb(50, 52, 60));
    visuals.widgets.hovered.bg_stroke = Stroke::new(1.0, K8S_BLUE);

    style.visuals = visuals;

    // Text styles
    style.text_styles.insert(
        TextStyle::Heading,
        egui::FontId::new(18.0, egui::FontFamily::Proportional),
    );
    style.text_styles.insert(
        TextStyle::Body,
        egui::FontId::new(13.0, egui::FontFamily::Proportional),
    );
    style.text_styles.insert(
        TextStyle::Monospace,
        egui::FontId::new(12.0, egui::FontFamily::Monospace),
    );

    // Spacing
    style.spacing.item_spacing = egui::vec2(8.0, 6.0);
    style.spacing.button_padding = egui::vec2(10.0, 4.0);
    style.spacing.window_margin = egui::Margin::same(8);

    ctx.set_style(style);
}