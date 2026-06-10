use super::mesh::{MeshBuilder, Vertex};
use cgmath::{vec3, InnerSpace, Vector3};
use std::f32::consts::PI;

pub fn add_cylinder_between(
    builder: &mut MeshBuilder,
    start: Vector3<f32>,
    end: Vector3<f32>,
    radius: f32,
    segments: usize,
    color: [f32; 4],
) {
    let axis = end - start;
    let length = axis.magnitude();
    if length <= f32::EPSILON {
        return;
    }

    let direction = axis / length;
    let helper = if direction.y.abs() < 0.95 {
        vec3(0.0, 1.0, 0.0)
    } else {
        vec3(1.0, 0.0, 0.0)
    };
    let right = direction.cross(helper).normalize();
    let up = right.cross(direction).normalize();
    let base = builder.vertices.len() as u32;

    for i in 0..segments {
        let t = i as f32 / segments as f32 * PI * 2.0;
        let normal = (right * t.cos() + up * t.sin()).normalize();
        for point in [start, end] {
            let position = point + normal * radius;
            builder.vertices.push(Vertex {
                position: position.into(),
                normal: normal.into(),
                color,
            });
        }
    }

    for i in 0..segments {
        let next = (i + 1) % segments;
        let a = base + (i * 2) as u32;
        let b = base + (i * 2 + 1) as u32;
        let c = base + (next * 2) as u32;
        let d = base + (next * 2 + 1) as u32;
        builder.indices.extend_from_slice(&[a, b, c, b, d, c]);
    }
}
