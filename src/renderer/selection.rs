use crate::{dna::DnaModel, renderer::camera::Camera};
use egui::{pos2, Color32, Pos2, Rect, Stroke};

#[derive(Debug, Clone, Copy)]
pub struct SelectionRect {
    pub start: Pos2,
    pub current: Pos2,
}

impl SelectionRect {
    pub fn rect(self) -> Rect {
        Rect::from_two_pos(self.start, self.current)
    }
}

pub fn draw_overlay(ctx: &egui::Context, selection: Option<SelectionRect>) {
    if let Some(selection) = selection {
        let pixels_per_point = ctx.pixels_per_point();
        let physical_rect = selection.rect();
        let rect = Rect::from_min_max(
            pos2(
                physical_rect.min.x / pixels_per_point,
                physical_rect.min.y / pixels_per_point,
            ),
            pos2(
                physical_rect.max.x / pixels_per_point,
                physical_rect.max.y / pixels_per_point,
            ),
        );
        let painter = ctx.layer_painter(egui::LayerId::new(
            egui::Order::Foreground,
            egui::Id::new("selection_rect"),
        ));
        painter.rect_filled(rect, 0.0, Color32::from_rgba_unmultiplied(80, 140, 255, 36));
        painter.rect_stroke(rect, 0.0, Stroke::new(1.0, Color32::from_rgb(90, 160, 255)));
    }
}

pub fn select_visible(
    dna: &DnaModel,
    camera: &Camera,
    visible_start: usize,
    visible_pairs: usize,
    rect: Rect,
    width: f32,
    height: f32,
) -> Vec<usize> {
    let screen_rect = Rect::from_min_max(
        pos2(rect.min.x.min(rect.max.x), rect.min.y.min(rect.max.y)),
        pos2(rect.min.x.max(rect.max.x), rect.min.y.max(rect.max.y)),
    );

    let visible = dna.visible_pairs(visible_start, visible_pairs);
    let y_offset = visible
        .first()
        .map(|pair| pair.left_position.y)
        .unwrap_or_default();
    let offset = cgmath::vec3(0.0, -y_offset, 0.0);

    visible
        .iter()
        .filter_map(|pair| {
            let left_position = pair.left_position + offset;
            let right_position = pair.right_position + offset;
            let left = camera.project(left_position, width, height)?;
            let right = camera.project(right_position, width, height)?;
            let center = camera.project((left_position + right_position) * 0.5, width, height)?;
            let inside = [left, right, center]
                .iter()
                .any(|point| screen_rect.contains(pos2(point[0], point[1])));
            inside.then_some(pair.index)
        })
        .collect()
}
