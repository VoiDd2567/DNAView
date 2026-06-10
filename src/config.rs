use crate::dna::Base;
use serde::Deserialize;
use std::{fs, path::Path};

#[derive(Debug, Clone, Deserialize)]
pub struct AppConfig {
    #[allow(dead_code)]
    #[serde(default = "default_visible_pairs")]
    pub visible_pairs: usize,
    #[serde(default = "default_map_visible_pairs")]
    pub map_visible_pairs: usize,
    #[serde(default = "default_radius")]
    pub radius: f32,
    #[serde(default = "default_vertical_spacing")]
    pub vertical_spacing: f32,
    #[serde(default = "default_angle_step")]
    pub angle_step: f32,
    #[serde(default = "default_sphere_radius")]
    pub sphere_radius: f32,
    #[serde(default = "default_stick_radius")]
    pub stick_radius: f32,
    #[serde(default)]
    pub colors: BaseColors,
}

#[derive(Debug, Clone, Deserialize)]
pub struct BaseColors {
    #[serde(default = "default_a")]
    #[serde(rename = "A")]
    pub a: String,
    #[serde(default = "default_t")]
    #[serde(rename = "T")]
    pub t: String,
    #[serde(default = "default_c")]
    #[serde(rename = "C")]
    pub c: String,
    #[serde(default = "default_g")]
    #[serde(rename = "G")]
    pub g: String,
    #[serde(default = "default_n")]
    #[serde(rename = "N")]
    pub n: String,
}

impl Default for AppConfig {
    fn default() -> Self {
        Self {
            visible_pairs: default_visible_pairs(),
            map_visible_pairs: default_map_visible_pairs(),
            radius: default_radius(),
            vertical_spacing: default_vertical_spacing(),
            angle_step: default_angle_step(),
            sphere_radius: default_sphere_radius(),
            stick_radius: default_stick_radius(),
            colors: BaseColors::default(),
        }
    }
}

impl Default for BaseColors {
    fn default() -> Self {
        Self {
            a: default_a(),
            t: default_t(),
            c: default_c(),
            g: default_g(),
            n: default_n(),
        }
    }
}

impl AppConfig {
    pub fn load(path: impl AsRef<Path>) -> Self {
        let path = path.as_ref();
        match fs::read_to_string(path) {
            Ok(text) => toml::from_str(&text).unwrap_or_else(|err| {
                log::warn!("failed to parse {}: {err}", path.display());
                Self::default()
            }),
            Err(err) => {
                log::warn!("failed to read {}: {err}", path.display());
                Self::default()
            }
        }
    }

    pub fn color_for(&self, base: Base) -> [f32; 4] {
        let text = match base {
            Base::A => &self.colors.a,
            Base::T => &self.colors.t,
            Base::C => &self.colors.c,
            Base::G => &self.colors.g,
            Base::N => &self.colors.n,
        };
        parse_hex_color(text).unwrap_or([1.0, 1.0, 1.0, 1.0])
    }

    pub fn effective_sphere_radius(&self) -> f32 {
        let strand_limit = self.radius * 0.42;
        self.sphere_radius.min(strand_limit).max(0.04)
    }

    pub fn effective_vertical_spacing(&self) -> f32 {
        let sphere_radius = self.effective_sphere_radius();
        let needed_center_distance = sphere_radius * 2.18;
        let horizontal_step = 2.0 * self.radius * (self.angle_step * 0.5).sin().abs();
        let needed_vertical = (needed_center_distance.powi(2) - horizontal_step.powi(2))
            .max(0.0)
            .sqrt();
        self.vertical_spacing
            .max(needed_vertical)
            .max(sphere_radius * 1.25)
    }
}

fn parse_hex_color(value: &str) -> Option<[f32; 4]> {
    let hex = value.strip_prefix('#').unwrap_or(value);
    if hex.len() != 6 {
        return None;
    }

    let r = u8::from_str_radix(&hex[0..2], 16).ok()?;
    let g = u8::from_str_radix(&hex[2..4], 16).ok()?;
    let b = u8::from_str_radix(&hex[4..6], 16).ok()?;
    Some([r as f32 / 255.0, g as f32 / 255.0, b as f32 / 255.0, 1.0])
}

fn default_visible_pairs() -> usize {
    40
}

fn default_map_visible_pairs() -> usize {
    50
}

fn default_radius() -> f32 {
    0.75
}

fn default_vertical_spacing() -> f32 {
    0.16
}

fn default_angle_step() -> f32 {
    0.36
}

fn default_sphere_radius() -> f32 {
    0.24
}

fn default_stick_radius() -> f32 {
    0.018
}

fn default_a() -> String {
    "#ff4444".to_owned()
}

fn default_t() -> String {
    "#44ff44".to_owned()
}

fn default_c() -> String {
    "#4444ff".to_owned()
}

fn default_g() -> String {
    "#ffff44".to_owned()
}

fn default_n() -> String {
    "#888888".to_owned()
}
