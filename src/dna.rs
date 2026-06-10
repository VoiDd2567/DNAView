use cgmath::{vec3, InnerSpace, Vector3};
use std::{collections::BTreeSet, f32::consts::PI};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Base {
    A,
    T,
    C,
    G,
    N,
}

#[derive(Debug, Clone)]
pub struct GenePair {
    pub index: usize,
    pub left: Base,
    pub right: Base,
    pub left_position: Vector3<f32>,
    pub right_position: Vector3<f32>,
}

#[derive(Debug, Clone)]
pub struct InvisibleAxis {
    pub start: Vector3<f32>,
    pub end: Vector3<f32>,
}

impl InvisibleAxis {
    pub fn center(&self) -> Vector3<f32> {
        (self.start + self.end) * 0.5
    }

    pub fn center_y(&self) -> f32 {
        self.center().y
    }

    pub fn height(&self) -> f32 {
        (self.end.y - self.start.y).abs()
    }

    pub fn length(&self) -> f32 {
        (self.end - self.start).magnitude()
    }
}

#[derive(Debug, Clone)]
pub struct DnaObject {
    pub gene_pairs: Vec<GenePair>,
    pub central_axis: InvisibleAxis,
}

#[derive(Debug, Clone)]
pub struct DnaModel {
    pub object: DnaObject,
    pub selected_indices: BTreeSet<usize>,
}

#[derive(Debug, Clone)]
pub struct HelixSettings {
    pub radius: f32,
    pub vertical_spacing: f32,
    pub angle_step: f32,
}

impl DnaModel {
    pub fn from_sequence(sequence: &str, settings: &HelixSettings) -> Result<Self, String> {
        let mut bases = Vec::new();
        let mut invalid = Vec::new();

        for (offset, ch) in sequence.char_indices() {
            if ch.is_whitespace() {
                continue;
            }

            match Base::try_from(ch) {
                Ok(base) => bases.push(base),
                Err(()) => invalid.push((offset, ch)),
            }
        }

        if !invalid.is_empty() {
            let summary = invalid
                .iter()
                .take(6)
                .map(|(offset, ch)| format!("'{ch}' at byte {offset}"))
                .collect::<Vec<_>>()
                .join(", ");
            return Err(format!(
                "Invalid DNA character(s): {summary}. Use only A, T, C, G, N."
            ));
        }

        let gene_pairs = bases
            .into_iter()
            .enumerate()
            .map(|(index, left)| {
                let angle = index as f32 * settings.angle_step;
                let y = index as f32 * settings.vertical_spacing;
                let left_position = vec3(
                    angle.cos() * settings.radius,
                    y,
                    angle.sin() * settings.radius,
                );
                let right_angle = angle + PI;
                let right_position = vec3(
                    right_angle.cos() * settings.radius,
                    y,
                    right_angle.sin() * settings.radius,
                );
                GenePair {
                    index,
                    left,
                    right: left.complement(),
                    left_position,
                    right_position,
                }
            })
            .collect::<Vec<_>>();
        let axis_end_y = gene_pairs
            .last()
            .map(|pair| pair.left_position.y)
            .unwrap_or_default();

        Ok(Self {
            object: DnaObject {
                gene_pairs,
                central_axis: InvisibleAxis {
                    start: vec3(0.0, 0.0, 0.0),
                    end: vec3(0.0, axis_end_y, 0.0),
                },
            },
            selected_indices: BTreeSet::new(),
        })
    }

    pub fn pairs(&self) -> &[GenePair] {
        &self.object.gene_pairs
    }

    pub fn pair_count(&self) -> usize {
        self.object.gene_pairs.len()
    }

    pub fn visible_pairs(&self, start: usize, limit: usize) -> &[GenePair] {
        let start = start.min(self.object.gene_pairs.len());
        let end = (start + limit).min(self.object.gene_pairs.len());
        &self.object.gene_pairs[start..end]
    }

    pub fn set_selected(&mut self, indices: impl IntoIterator<Item = usize>) {
        self.selected_indices = indices.into_iter().collect();
    }
}

impl Base {
    pub fn complement(self) -> Self {
        match self {
            Self::A => Self::T,
            Self::T => Self::A,
            Self::C => Self::G,
            Self::G => Self::C,
            Self::N => Self::N,
        }
    }
}

impl TryFrom<char> for Base {
    type Error = ();

    fn try_from(value: char) -> Result<Self, Self::Error> {
        match value.to_ascii_uppercase() {
            'A' => Ok(Self::A),
            'T' => Ok(Self::T),
            'C' => Ok(Self::C),
            'G' => Ok(Self::G),
            'N' => Ok(Self::N),
            _ => Err(()),
        }
    }
}
