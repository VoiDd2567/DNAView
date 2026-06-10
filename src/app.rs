use crate::{
    config::AppConfig,
    dna::{Base, DnaModel, HelixSettings},
    renderer::{selection, Renderer},
};
use egui_wgpu::ScreenDescriptor;
use std::{fs, path::PathBuf, sync::Arc, time::Instant};
use winit::{
    dpi::{PhysicalPosition, PhysicalSize},
    event::{ElementState, MouseButton, MouseScrollDelta, WindowEvent},
    keyboard::{Key, ModifiersState, NamedKey},
    window::{Fullscreen, Window},
};

pub struct App {
    window: Arc<Window>,
    renderer: Renderer,
    egui_ctx: egui::Context,
    egui_state: egui_winit::State,
    egui_renderer: egui_wgpu::Renderer,
    config: AppConfig,
    dna: DnaModel,
    sequence_input: String,
    validation_error: Option<String>,
    mesh_dirty: bool,
    camera_y: f32,
    auto_rotate: bool,
    last_update: Instant,
    last_cursor: Option<PhysicalPosition<f64>>,
    rotating: bool,
    panning: bool,
    selecting: bool,
    modifiers: ModifiersState,
    selection_rect: Option<selection::SelectionRect>,
    fullscreen: bool,
    show_settings: bool,
    settings_config: AppConfig,
    sequence_name: String,
    sequence_files: Vec<PathBuf>,
    sequence_message: Option<String>,
    map_start: usize,
    dna_map_hovered: bool,
    dna_map_wheel_handled: bool,
}

impl App {
    pub async fn new(window: Window) -> Self {
        let window = Arc::new(window);
        let config = AppConfig::load("config.toml");
        let settings = HelixSettings {
            radius: config.radius,
            vertical_spacing: config.effective_vertical_spacing(),
            angle_step: config.angle_step,
        };
        let sequence_input = "ATCG ATCG ATCG ATCG ATCG ATCG ATCG ATCG ATCG ATCG".to_owned();
        let dna = DnaModel::from_sequence(&sequence_input, &settings)
            .expect("default sequence must be valid");

        let mut renderer = Renderer::new(window.clone()).await;
        renderer.rebuild_mesh(&dna, &config, 0, dna.pair_count());

        let egui_ctx = egui::Context::default();
        let egui_state = egui_winit::State::new(
            egui_ctx.clone(),
            egui::ViewportId::ROOT,
            &window,
            Some(window.scale_factor() as f32),
            None,
        );
        let egui_renderer =
            egui_wgpu::Renderer::new(&renderer.device, renderer.config.format, None, 1);
        let camera_y = dna_center_y(&dna, &config);
        let settings_config = config.clone();
        let sequence_files = list_sequence_files();

        Self {
            window,
            renderer,
            egui_ctx,
            egui_state,
            egui_renderer,
            config,
            dna,
            sequence_input,
            validation_error: None,
            mesh_dirty: false,
            camera_y,
            auto_rotate: true,
            last_update: Instant::now(),
            last_cursor: None,
            rotating: false,
            panning: false,
            selecting: false,
            modifiers: ModifiersState::empty(),
            selection_rect: None,
            fullscreen: false,
            show_settings: false,
            settings_config,
            sequence_name: "sequence_1".to_owned(),
            sequence_files,
            sequence_message: None,
            map_start: 0,
            dna_map_hovered: false,
            dna_map_wheel_handled: false,
        }
    }

    pub fn window(&self) -> &Window {
        &self.window
    }

    pub fn resize(&mut self, size: PhysicalSize<u32>) {
        self.renderer.resize(size);
        self.mesh_dirty = true;
    }

    pub fn update(&mut self) {
        let now = Instant::now();
        let delta = now.duration_since(self.last_update).as_secs_f32();
        self.last_update = now;

        self.sync_camera_target();

        if self.auto_rotate && !self.selecting {
            self.renderer.model_rotation += delta * 0.45;
        }

        if self.mesh_dirty {
            self.renderer
                .rebuild_mesh(&self.dna, &self.config, 0, self.dna.pair_count());
            self.mesh_dirty = false;
        }
    }

    pub fn handle_window_event(&mut self, event: &WindowEvent) -> bool {
        let egui_response = self.egui_state.on_window_event(&self.window, event);
        if egui_response.repaint {
            self.window.request_redraw();
        }

        match event {
            WindowEvent::ModifiersChanged(modifiers) => {
                self.modifiers = modifiers.state();
            }
            WindowEvent::CursorMoved { position, .. } => {
                self.handle_cursor(*position, egui_response.consumed);
            }
            WindowEvent::MouseInput { state, button, .. } => {
                self.handle_mouse_button(*button, *state, egui_response.consumed);
            }
            WindowEvent::MouseWheel { delta, .. } => {
                if self.dna_map_hovered && self.max_map_start() > 0 {
                    self.scroll_dna_map(delta);
                    self.dna_map_wheel_handled = true;
                } else if !egui_response.consumed {
                    let scroll = match delta {
                        MouseScrollDelta::LineDelta(_, y) => *y * 72.0,
                        MouseScrollDelta::PixelDelta(pos) => pos.y as f32,
                    };
                    self.renderer.camera.zoom(scroll);
                    self.clamp_camera_y();
                    self.apply_camera_target();
                }
            }
            WindowEvent::KeyboardInput { event, .. }
                if event.state == ElementState::Pressed
                    && matches!(event.logical_key, Key::Named(NamedKey::Escape)) =>
            {
                if self.fullscreen {
                    self.set_fullscreen(false);
                }
            }
            _ => {}
        }

        egui_response.consumed
    }

    fn handle_cursor(&mut self, position: PhysicalPosition<f64>, egui_consumed: bool) {
        let Some(last) = self.last_cursor.replace(position) else {
            return;
        };
        let dx = (position.x - last.x) as f32;
        let dy = (position.y - last.y) as f32;

        if self.selecting {
            if let Some(selection) = &mut self.selection_rect {
                selection.current = egui::pos2(position.x as f32, position.y as f32);
            }
            return;
        }

        if self.rotating && !egui_consumed {
            if dx.abs() >= dy.abs() {
                self.renderer.camera.roll_vertical_axis(dx);
            } else {
                self.renderer.model_tilt = wrap_angle(self.renderer.model_tilt + dy * 0.01);
            }
        } else if self.panning && !egui_consumed {
            self.renderer.camera.pan_screen(dx, dy);
        }
    }

    fn handle_mouse_button(
        &mut self,
        button: MouseButton,
        state: ElementState,
        egui_consumed: bool,
    ) {
        match (button, state) {
            (MouseButton::Left, ElementState::Pressed) if !egui_consumed => {
                if self.modifiers.control_key() {
                    self.selecting = true;
                    if let Some(cursor) = self.last_cursor {
                        let point = egui::pos2(cursor.x as f32, cursor.y as f32);
                        self.selection_rect = Some(selection::SelectionRect {
                            start: point,
                            current: point,
                        });
                    }
                } else if self.modifiers.shift_key() {
                    self.rotating = true;
                } else {
                    self.panning = true;
                }
            }
            (MouseButton::Left, ElementState::Released) => {
                if self.selecting {
                    self.finish_selection();
                }
                self.selecting = false;
                self.rotating = false;
                self.panning = false;
            }
            _ => {}
        }
    }

    fn finish_selection(&mut self) {
        let Some(selection) = self.selection_rect.take() else {
            return;
        };
        let selected = selection::select_visible(
            &self.dna,
            &self.renderer.camera,
            0,
            self.dna.pair_count(),
            selection.rect(),
            self.renderer.size.width as f32,
            self.renderer.size.height as f32,
        );
        self.dna.set_selected(selected);
        self.mesh_dirty = true;
    }

    pub fn render(&mut self) -> Result<(), wgpu::SurfaceError> {
        let raw_input = self.egui_state.take_egui_input(&self.window);
        let egui_ctx = self.egui_ctx.clone();
        let full_output = egui_ctx.run(raw_input, |ctx| {
            if !self.fullscreen {
                self.ui(ctx);
                selection::draw_overlay(ctx, self.selection_rect);
            }
        });
        self.egui_state
            .handle_platform_output(&self.window, full_output.platform_output);

        for (texture_id, image_delta) in &full_output.textures_delta.set {
            self.egui_renderer.update_texture(
                &self.renderer.device,
                &self.renderer.queue,
                *texture_id,
                image_delta,
            );
        }

        let pixels_per_point = self.egui_ctx.pixels_per_point();
        let paint_jobs = self
            .egui_ctx
            .tessellate(full_output.shapes, pixels_per_point);
        let screen_descriptor = ScreenDescriptor {
            size_in_pixels: [self.renderer.size.width, self.renderer.size.height],
            pixels_per_point,
        };

        self.renderer.update_camera();
        let output = self.renderer.begin_frame()?;
        let mut encoder =
            self.renderer
                .device
                .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                    label: Some("render encoder"),
                });

        self.egui_renderer.update_buffers(
            &self.renderer.device,
            &self.renderer.queue,
            &mut encoder,
            &paint_jobs,
            &screen_descriptor,
        );

        {
            let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("dna render pass"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: &output.view,
                    resolve_target: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Clear(wgpu::Color {
                            r: 0.92,
                            g: 0.965,
                            b: 1.0,
                            a: 1.0,
                        }),
                        store: wgpu::StoreOp::Store,
                    },
                })],
                depth_stencil_attachment: Some(wgpu::RenderPassDepthStencilAttachment {
                    view: self.renderer.depth_view(),
                    depth_ops: Some(wgpu::Operations {
                        load: wgpu::LoadOp::Clear(1.0),
                        store: wgpu::StoreOp::Store,
                    }),
                    stencil_ops: None,
                }),
                timestamp_writes: None,
                occlusion_query_set: None,
            });
            self.renderer.render_background(&mut pass);
            self.renderer.render_dna(&mut pass);
        }

        {
            let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("egui render pass"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: &output.view,
                    resolve_target: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Load,
                        store: wgpu::StoreOp::Store,
                    },
                })],
                depth_stencil_attachment: None,
                timestamp_writes: None,
                occlusion_query_set: None,
            });
            self.egui_renderer
                .render(&mut pass, &paint_jobs, &screen_descriptor);
        }

        for texture_id in &full_output.textures_delta.free {
            self.egui_renderer.free_texture(texture_id);
        }

        self.renderer.queue.submit(Some(encoder.finish()));
        output.frame.present();
        Ok(())
    }

    fn ui(&mut self, ctx: &egui::Context) {
        egui::TopBottomPanel::top("top_panel").show(ctx, |ui| {
            ui.horizontal(|ui| {
                ui.label("DNA strand");
                ui.separator();
                ui.checkbox(&mut self.auto_rotate, "Auto rotate");
                if ui.button("Auto zoom").clicked() {
                    self.reset_camera_to_dna();
                }
                if ui.button("Reset angles").clicked() {
                    self.reset_angles();
                }
                if ui.button("Fullscreen").clicked() {
                    self.set_fullscreen(true);
                }
                if ui.button("Settings").clicked() {
                    self.show_settings = true;
                }
            });

            let response = ui.add(
                egui::TextEdit::singleline(&mut self.sequence_input)
                    .desired_width(f32::INFINITY)
                    .hint_text("Paste A, T, C, G, N sequence"),
            );
            if response.changed() {
                let formatted = format_sequence_groups(&self.sequence_input);
                if formatted != self.sequence_input {
                    self.sequence_input = formatted;
                }
                self.rebuild_dna_from_input();
            }

            ui.horizontal(|ui| {
                if let Some(error) = &self.validation_error {
                    ui.colored_label(egui::Color32::from_rgb(255, 120, 120), error);
                } else {
                    ui.label(format!(
                        "{} total pairs, rendering {} spheres, selected {}",
                        self.dna.pair_count(),
                        self.dna.pair_count() * 2,
                        self.dna.selected_indices.len()
                    ));
                }
            });
        });

        if self.needs_camera_scrollbar() {
            egui::SidePanel::right("camera_scroll_panel")
                .resizable(false)
                .exact_width(46.0)
                .show(ctx, |ui| {
                    ui.vertical_centered(|ui| {
                        ui.label("Y");
                        ui.add_space(6.0);
                        let (min_y, max_y) = self.camera_y_range();
                        let response = ui.add_sized(
                            [26.0, (ui.available_height() - 22.0).max(80.0)],
                            egui::Slider::new(&mut self.camera_y, min_y..=max_y)
                                .vertical()
                                .show_value(false),
                        );
                        if response.changed() {
                            self.apply_camera_target();
                        }
                    });
                });
        }

        self.color_memo(ctx);
        self.right_panel(ctx);
        self.settings_window(ctx);
    }

    fn color_memo(&self, ctx: &egui::Context) {
        egui::Area::new(egui::Id::new("base_color_memo"))
            .fixed_pos(egui::pos2(12.0, 76.0))
            .show(ctx, |ui| {
                egui::Frame::none()
                    .fill(egui::Color32::from_rgba_unmultiplied(246, 252, 255, 218))
                    .stroke(egui::Stroke::new(
                        1.0,
                        egui::Color32::from_rgb(185, 215, 235),
                    ))
                    .rounding(egui::Rounding::same(6.0))
                    .inner_margin(egui::Margin::symmetric(10.0, 8.0))
                    .show(ui, |ui| {
                        ui.label(egui::RichText::new("Base colors").strong());
                        ui.add_space(4.0);
                        for (label, base) in [
                            ("A", Base::A),
                            ("T", Base::T),
                            ("C", Base::C),
                            ("G", Base::G),
                            ("N", Base::N),
                        ] {
                            ui.horizontal(|ui| {
                                let (rect, _) = ui.allocate_exact_size(
                                    egui::vec2(14.0, 14.0),
                                    egui::Sense::hover(),
                                );
                                ui.painter().rect_filled(
                                    rect,
                                    3.0,
                                    egui_color(self.config.color_for(base)),
                                );
                                ui.label(label);
                            });
                        }
                    });
            });
    }

    fn right_panel(&mut self, ctx: &egui::Context) {
        egui::SidePanel::right("dna_overview_panel")
            .resizable(false)
            .exact_width(170.0)
            .show(ctx, |ui| {
                ui.heading("DNA map");
                self.draw_dna_overview(ui);
                ui.separator();
                ui.label("Sequences");
                ui.horizontal(|ui| {
                    ui.label("Name");
                    ui.text_edit_singleline(&mut self.sequence_name);
                });
                ui.horizontal(|ui| {
                    if ui.button("Save").clicked() {
                        self.save_current_sequence();
                    }
                    if ui.button("Refresh").clicked() {
                        self.sequence_files = list_sequence_files();
                    }
                });
                if let Some(message) = &self.sequence_message {
                    ui.label(message);
                }
                ui.separator();
                egui::ScrollArea::vertical()
                    .max_height(180.0)
                    .show(ui, |ui| {
                        for path in self.sequence_files.clone() {
                            let label = path
                                .file_stem()
                                .and_then(|name| name.to_str())
                                .unwrap_or("sequence")
                                .to_owned();
                            if ui.button(label).clicked() {
                                self.load_sequence(path);
                            }
                        }
                    });
            });
    }

    fn draw_dna_overview(&mut self, ui: &mut egui::Ui) {
        let desired = egui::vec2(146.0, (ui.available_height() * 0.6).max(300.0));
        let (rect, response) = ui.allocate_exact_size(desired, egui::Sense::click_and_drag());
        self.dna_map_hovered = response.hovered();
        if self.dna_map_hovered && self.max_map_start() > 0 && !self.dna_map_wheel_handled {
            let scroll_delta = ui.ctx().input(|input| input.raw_scroll_delta.y);
            if scroll_delta.abs() > f32::EPSILON {
                self.scroll_dna_map_by_points(scroll_delta);
            }
        }
        let painter = ui.painter();
        painter.rect_filled(
            rect,
            6.0,
            egui::Color32::from_rgba_unmultiplied(246, 252, 255, 210),
        );
        painter.rect_stroke(
            rect,
            6.0,
            egui::Stroke::new(1.0, egui::Color32::from_rgb(175, 210, 230)),
        );

        if self.dna.pairs().is_empty() {
            return;
        }

        let map_visible_pairs = self.config.map_visible_pairs.max(1);
        let window_end = (self.map_start + map_visible_pairs).min(self.dna.pair_count());
        let window = &self.dna.pairs()[self.map_start..window_end];
        if window.is_empty() {
            return;
        }

        let start_y = window
            .first()
            .map(|pair| pair.left_position.y)
            .unwrap_or_default();
        let end_y = window
            .last()
            .map(|pair| pair.left_position.y)
            .unwrap_or(start_y);
        let window_height = (end_y - start_y).max(self.config.effective_vertical_spacing());
        let gutter_right = rect.left() + 38.0;
        let map_x = rect.center().x + 14.0;
        let top = rect.top() + 12.0;
        let bottom = rect.bottom() - 12.0;

        let mut focus_y = None;
        if (response.clicked() || response.dragged()) && response.interact_pointer_pos().is_some() {
            if let Some(position) = response.interact_pointer_pos() {
                let t = ((bottom - position.y) / (bottom - top)).clamp(0.0, 1.0);
                focus_y = Some(start_y + t * window_height);
            }
        }

        painter.line_segment(
            [egui::pos2(map_x, top), egui::pos2(map_x, bottom)],
            egui::Stroke::new(2.0, egui::Color32::from_rgb(95, 120, 135)),
        );
        painter.line_segment(
            [
                egui::pos2(gutter_right, top),
                egui::pos2(gutter_right, bottom),
            ],
            egui::Stroke::new(1.0, egui::Color32::from_rgb(205, 220, 230)),
        );

        let row_spacing = if window.len() > 1 {
            (bottom - top) / (window.len() - 1) as f32
        } else {
            bottom - top
        };
        let pair_label_font_size = (row_spacing * 0.76).clamp(7.0, 12.0);
        let base_letter_font_size = (row_spacing * 0.88).clamp(9.0, 17.0);

        for pair in window {
            let t = (pair.left_position.y - start_y) / window_height;
            let y = egui::lerp(bottom..=top, t);
            let base_font = egui::FontId::monospace(base_letter_font_size);
            painter.text(
                egui::pos2(map_x - 22.0, y),
                egui::Align2::CENTER_CENTER,
                base_label(pair.left),
                base_font.clone(),
                egui_color(self.config.color_for(pair.left)),
            );
            painter.text(
                egui::pos2(map_x + 22.0, y),
                egui::Align2::CENTER_CENTER,
                base_label(pair.right),
                base_font,
                egui_color(self.config.color_for(pair.right)),
            );
            if (pair.index + 1) % 2 == 1 {
                painter.text(
                    egui::pos2(gutter_right - 5.0, y),
                    egui::Align2::RIGHT_CENTER,
                    (pair.index + 1).to_string(),
                    egui::FontId::monospace(pair_label_font_size),
                    egui::Color32::from_rgb(84, 105, 118),
                );
            }
        }

        let visible_half = self.renderer.camera.visible_world_height() * 0.5;
        let center_y = self.renderer.camera.pan.y;
        let visible_min = (center_y - visible_half - start_y) / window_height;
        let visible_max = (center_y + visible_half - start_y) / window_height;
        if visible_max >= 0.0 && visible_min <= 1.0 {
            let clipped_min = visible_min.clamp(0.0, 1.0);
            let clipped_max = visible_max.clamp(0.0, 1.0);
            let y_min = egui::lerp(bottom..=top, clipped_min);
            let y_max = egui::lerp(bottom..=top, clipped_max);
            let visible_rect = egui::Rect::from_min_max(
                egui::pos2(gutter_right + 6.0, y_max),
                egui::pos2(rect.right() - 16.0, y_min),
            );
            painter.rect_stroke(
                visible_rect,
                4.0,
                egui::Stroke::new(2.0, egui::Color32::from_rgb(25, 120, 210)),
            );
        }

        if let Some(y) = focus_y {
            self.focus_camera_y(y);
        }

        if self.dna.pair_count() > map_visible_pairs {
            let scrollbar_rect = egui::Rect::from_min_max(
                egui::pos2(rect.right() - 9.0, top),
                egui::pos2(rect.right() - 3.0, bottom),
            );
            painter.rect_filled(
                scrollbar_rect,
                3.0,
                egui::Color32::from_rgba_unmultiplied(160, 190, 210, 90),
            );

            let max_start = self.max_map_start().max(1);
            let window_fraction =
                (map_visible_pairs as f32 / self.dna.pair_count() as f32).clamp(0.08, 1.0);
            let track_height = scrollbar_rect.height();
            let thumb_height = (track_height * window_fraction).max(18.0);
            let travel = (track_height - thumb_height).max(0.0);
            let scroll_t = 1.0 - self.map_start as f32 / max_start as f32;
            let thumb_top = scrollbar_rect.top() + travel * scroll_t;
            let thumb_rect = egui::Rect::from_min_max(
                egui::pos2(scrollbar_rect.left(), thumb_top),
                egui::pos2(scrollbar_rect.right(), thumb_top + thumb_height),
            );
            painter.rect_filled(thumb_rect, 3.0, egui::Color32::from_rgb(70, 135, 190));

            let scrollbar_response = ui.interact(
                scrollbar_rect,
                egui::Id::new("dna_map_scrollbar"),
                egui::Sense::click_and_drag(),
            );
            self.dna_map_hovered |= scrollbar_response.hovered();
            if (scrollbar_response.clicked() || scrollbar_response.dragged())
                && scrollbar_response.interact_pointer_pos().is_some()
            {
                if let Some(position) = scrollbar_response.interact_pointer_pos() {
                    let t = ((position.y - scrollbar_rect.top() - thumb_height * 0.5)
                        / travel.max(1.0))
                    .clamp(0.0, 1.0);
                    let map_start = ((1.0 - t) * max_start as f32).round() as usize;
                    self.set_map_start_from_user(map_start);
                }
            }
        }
        self.dna_map_wheel_handled = false;
    }

    fn settings_window(&mut self, ctx: &egui::Context) {
        if !self.show_settings {
            return;
        }

        let mut open = self.show_settings;
        egui::Window::new("Settings")
            .open(&mut open)
            .resizable(false)
            .show(ctx, |ui| {
                let mut changed = false;
                changed |= ui
                    .add(
                        egui::Slider::new(&mut self.settings_config.radius, 0.2..=3.0)
                            .text("Radius"),
                    )
                    .changed();
                changed |= ui
                    .add(
                        egui::Slider::new(&mut self.settings_config.vertical_spacing, 0.05..=1.2)
                            .text("Vertical spacing"),
                    )
                    .changed();
                changed |= ui
                    .add(
                        egui::Slider::new(&mut self.settings_config.angle_step, 0.05..=1.5)
                            .text("Angle step"),
                    )
                    .changed();
                changed |= ui
                    .add(
                        egui::Slider::new(&mut self.settings_config.sphere_radius, 0.04..=0.8)
                            .text("Sphere radius"),
                    )
                    .changed();
                changed |= ui
                    .add(
                        egui::Slider::new(&mut self.settings_config.stick_radius, 0.004..=0.08)
                            .text("Stick radius"),
                    )
                    .changed();
                changed |= ui
                    .add(
                        egui::Slider::new(&mut self.settings_config.map_visible_pairs, 1..=500)
                            .text("Map visible pairs"),
                    )
                    .changed();

                ui.separator();
                ui.label("Colors");
                changed |= color_edit(ui, "A", &mut self.settings_config.colors.a);
                changed |= color_edit(ui, "T", &mut self.settings_config.colors.t);
                changed |= color_edit(ui, "C", &mut self.settings_config.colors.c);
                changed |= color_edit(ui, "G", &mut self.settings_config.colors.g);
                changed |= color_edit(ui, "N", &mut self.settings_config.colors.n);

                if changed {
                    self.apply_settings();
                }

                if ui.button("Save config.toml").clicked() {
                    match fs::write("config.toml", format_config_toml(&self.config)) {
                        Ok(()) => self.sequence_message = Some("Saved config.toml".to_owned()),
                        Err(err) => {
                            self.sequence_message = Some(format!("Config save failed: {err}"))
                        }
                    }
                }
            });
        self.show_settings = open;
    }

    fn rebuild_dna_from_input(&mut self) {
        let settings = HelixSettings {
            radius: self.config.radius,
            vertical_spacing: self.config.effective_vertical_spacing(),
            angle_step: self.config.angle_step,
        };

        match DnaModel::from_sequence(&self.sequence_input, &settings) {
            Ok(dna) => {
                self.dna = dna;
                self.camera_y = dna_center_y(&self.dna, &self.config);
                self.map_start = self.map_start.min(self.max_map_start());
                self.validation_error = None;
                self.mesh_dirty = true;
            }
            Err(err) => {
                self.validation_error = Some(err);
            }
        }
    }

    fn sync_camera_target(&mut self) {
        self.clamp_camera_y();
        self.apply_camera_target();
        if !self.dna_map_hovered && !self.dna_map_wheel_handled {
            self.sync_map_to_camera_view();
        }
    }

    fn focus_camera_y(&mut self, y: f32) {
        self.renderer.camera.pan_offset.y = 0.0;
        self.camera_y = y;
        self.clamp_camera_y();
        self.apply_camera_target();
    }

    fn reset_camera_to_dna(&mut self) {
        self.renderer.camera.pan_offset = cgmath::vec3(0.0, 0.0, 0.0);
        self.renderer.camera.view_offset = cgmath::vec3(0.0, 0.0, 0.0);
        self.reset_angles();
        self.camera_y = dna_center_y(&self.dna, &self.config);
        let dna_height = self
            .dna_height()
            .max(self.config.effective_sphere_radius() * 2.0);
        let fit_distance = dna_height * 1.18;
        self.renderer.camera.distance = fit_distance.clamp(4.0, 120.0);
        self.apply_camera_target();
    }

    fn reset_angles(&mut self) {
        self.renderer.model_rotation = 0.0;
        self.renderer.model_tilt = 0.0;
        self.renderer.camera.roll = 0.0;
        self.renderer.camera.pitch = 0.3;
    }

    fn apply_settings(&mut self) {
        self.config = self.settings_config.clone();
        self.map_start = self.map_start.min(self.max_map_start());
        self.rebuild_dna_from_input();
        self.mesh_dirty = true;
    }

    fn set_fullscreen(&mut self, fullscreen: bool) {
        self.fullscreen = fullscreen;
        if fullscreen {
            self.window
                .set_fullscreen(Some(Fullscreen::Borderless(self.window.current_monitor())));
        } else {
            self.window.set_fullscreen(None);
        }
    }

    fn save_current_sequence(&mut self) {
        let name = sanitize_sequence_name(&self.sequence_name);
        let directory = sequence_directory();
        if let Err(err) = fs::create_dir_all(&directory) {
            self.sequence_message = Some(format!("Save failed: {err}"));
            return;
        }

        let path = directory.join(format!("{name}.dna.txt"));
        match fs::write(&path, &self.sequence_input) {
            Ok(()) => {
                self.sequence_message = Some(format!("Saved {name}"));
                self.sequence_files = list_sequence_files();
            }
            Err(err) => {
                self.sequence_message = Some(format!("Save failed: {err}"));
            }
        }
    }

    fn load_sequence(&mut self, path: PathBuf) {
        match fs::read_to_string(&path) {
            Ok(text) => {
                self.sequence_input = format_sequence_groups(&text);
                self.rebuild_dna_from_input();
                if let Some(name) = path.file_stem().and_then(|name| name.to_str()) {
                    self.sequence_name = name.trim_end_matches(".dna").to_owned();
                }
                self.sequence_message = Some("Loaded sequence".to_owned());
            }
            Err(err) => {
                self.sequence_message = Some(format!("Load failed: {err}"));
            }
        }
    }

    fn needs_camera_scrollbar(&self) -> bool {
        false
    }

    fn camera_y_range(&self) -> (f32, f32) {
        let height = self.dna_height();
        if height <= f32::EPSILON {
            return (0.0, 0.0);
        }

        let half_visible = self.renderer.camera.visible_world_height() * 0.42;
        let min_y = half_visible.min(height * 0.5);
        let max_y = (height - half_visible).max(min_y);
        (min_y, max_y)
    }

    fn clamp_camera_y(&mut self) {
        let (min_y, max_y) = self.camera_y_range();
        self.camera_y = self.camera_y.clamp(min_y, max_y);
    }

    fn apply_camera_target(&mut self) {
        let target = self.renderer.dna_axis_world_point(self.camera_y);
        self.renderer.camera.set_target(target);
    }

    fn dna_height(&self) -> f32 {
        self.dna.object.central_axis.height()
    }

    fn max_map_start(&self) -> usize {
        self.dna
            .pair_count()
            .saturating_sub(self.config.map_visible_pairs.max(1))
    }

    fn sync_map_to_camera_view(&mut self) {
        let pair_count = self.dna.pair_count();
        let map_visible_pairs = self.config.map_visible_pairs.max(1);
        if pair_count <= map_visible_pairs {
            self.map_start = 0;
            return;
        }

        let spacing = self.config.effective_vertical_spacing();
        if spacing <= f32::EPSILON {
            return;
        }

        let center_pair = (self.camera_y / spacing)
            .round()
            .clamp(0.0, pair_count.saturating_sub(1) as f32) as usize;
        let desired_start = center_pair.saturating_sub(map_visible_pairs / 2);
        self.map_start = desired_start.min(self.max_map_start());
    }

    fn scroll_dna_map(&mut self, delta: &MouseScrollDelta) {
        let scroll_pairs = match delta {
            MouseScrollDelta::LineDelta(_, y) => *y * 3.0,
            MouseScrollDelta::PixelDelta(position) => {
                self.map_scroll_points_to_pairs(position.y as f32)
            }
        };
        self.scroll_dna_map_by_pairs(scroll_pairs);
    }

    fn scroll_dna_map_by_points(&mut self, points: f32) {
        self.scroll_dna_map_by_pairs(self.map_scroll_points_to_pairs(points));
    }

    fn map_scroll_points_to_pairs(&self, points: f32) -> f32 {
        points / 24.0
    }

    fn scroll_dna_map_by_pairs(&mut self, scroll_pairs: f32) {
        if scroll_pairs.abs() < 0.5 {
            return;
        }

        let next = self.map_start as isize + scroll_pairs.round() as isize;
        self.set_map_start_from_user(next.clamp(0, self.max_map_start() as isize) as usize);
    }

    fn set_map_start_from_user(&mut self, map_start: usize) {
        self.map_start = map_start.min(self.max_map_start());
        if let Some(center_y) = self.map_window_center_y() {
            self.focus_camera_y(center_y);
        }
    }

    fn map_window_center_y(&self) -> Option<f32> {
        let pair_count = self.dna.pair_count();
        if pair_count == 0 {
            return None;
        }

        let map_visible_pairs = self.config.map_visible_pairs.max(1);
        let window_end = (self.map_start + map_visible_pairs).min(pair_count);
        let center_index = self.map_start + (window_end.saturating_sub(self.map_start) / 2);
        self.dna
            .pairs()
            .get(center_index)
            .map(|pair| pair.left_position.y)
    }
}

fn dna_center_y(dna: &DnaModel, _config: &AppConfig) -> f32 {
    dna.object.central_axis.center_y()
}

fn format_sequence_groups(input: &str) -> String {
    let mut formatted = String::new();
    for (index, ch) in input.chars().filter(|ch| !ch.is_whitespace()).enumerate() {
        if index > 0 && index % 4 == 0 {
            formatted.push(' ');
        }
        formatted.push(ch.to_ascii_uppercase());
    }
    formatted
}

fn color_edit(ui: &mut egui::Ui, label: &str, value: &mut String) -> bool {
    let mut changed = false;
    ui.horizontal(|ui| {
        ui.label(label);
        changed = ui.text_edit_singleline(value).changed();
    });
    changed
}

fn format_config_toml(config: &AppConfig) -> String {
    format!(
        "visible_pairs = {}\nmap_visible_pairs = {}\nradius = {}\nvertical_spacing = {}\nangle_step = {}\nsphere_radius = {}\nstick_radius = {}\n\n[colors]\nA = \"{}\"\nT = \"{}\"\nC = \"{}\"\nG = \"{}\"\nN = \"{}\"\n",
        config.visible_pairs,
        config.map_visible_pairs,
        config.radius,
        config.vertical_spacing,
        config.angle_step,
        config.sphere_radius,
        config.stick_radius,
        config.colors.a,
        config.colors.t,
        config.colors.c,
        config.colors.g,
        config.colors.n,
    )
}

fn sequence_directory() -> PathBuf {
    PathBuf::from("sequences")
}

fn list_sequence_files() -> Vec<PathBuf> {
    let mut files = match fs::read_dir(sequence_directory()) {
        Ok(entries) => entries
            .filter_map(|entry| entry.ok().map(|entry| entry.path()))
            .filter(|path| path.extension().and_then(|ext| ext.to_str()) == Some("txt"))
            .collect::<Vec<_>>(),
        Err(_) => Vec::new(),
    };
    files.sort();
    files
}

fn sanitize_sequence_name(name: &str) -> String {
    let sanitized = name
        .chars()
        .map(|ch| {
            if ch.is_ascii_alphanumeric() || ch == '-' || ch == '_' {
                ch
            } else {
                '_'
            }
        })
        .collect::<String>();
    let trimmed = sanitized.trim_matches('_');
    if trimmed.is_empty() {
        "sequence".to_owned()
    } else {
        trimmed.to_owned()
    }
}

fn egui_color(color: [f32; 4]) -> egui::Color32 {
    egui::Color32::from_rgba_unmultiplied(
        (color[0].clamp(0.0, 1.0) * 255.0) as u8,
        (color[1].clamp(0.0, 1.0) * 255.0) as u8,
        (color[2].clamp(0.0, 1.0) * 255.0) as u8,
        (color[3].clamp(0.0, 1.0) * 255.0) as u8,
    )
}

fn base_label(base: Base) -> &'static str {
    match base {
        Base::A => "A",
        Base::T => "T",
        Base::C => "C",
        Base::G => "G",
        Base::N => "N",
    }
}

fn wrap_angle(angle: f32) -> f32 {
    let tau = std::f32::consts::TAU;
    (angle + std::f32::consts::PI).rem_euclid(tau) - std::f32::consts::PI
}
