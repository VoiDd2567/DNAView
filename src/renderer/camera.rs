use cgmath::{ortho, vec3, InnerSpace, Matrix4, Point3, Vector3};

const OPENGL_TO_WGPU_MATRIX: Matrix4<f32> = Matrix4::new(
    1.0, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 0.0, 0.5, 0.0, 0.0, 0.0, 0.5, 1.0,
);

pub struct Camera {
    pub yaw: f32,
    pub pitch: f32,
    pub roll: f32,
    pub distance: f32,
    pub pan: Vector3<f32>,
    pub pan_offset: Vector3<f32>,
    pub view_offset: Vector3<f32>,
    pub aspect: f32,
    znear: f32,
    zfar: f32,
}

impl Camera {
    pub fn new(aspect: f32) -> Self {
        Self {
            yaw: 0.55,
            pitch: 0.3,
            roll: 0.0,
            distance: 8.0,
            pan: vec3(0.0, 3.0, 0.0),
            pan_offset: vec3(0.0, 0.0, 0.0),
            view_offset: vec3(0.0, 0.0, 0.0),
            aspect,
            znear: -500.0,
            zfar: 500.0,
        }
    }

    pub fn set_target(&mut self, target: Vector3<f32>) {
        self.pan = target + self.pan_offset;
    }

    pub fn zoom(&mut self, delta: f32) {
        self.distance = (self.distance * (1.0 - delta * 0.001)).clamp(2.0, 120.0);
    }

    pub fn roll_vertical_axis(&mut self, delta_x: f32) {
        self.roll = wrap_angle(self.roll + delta_x * 0.01);
    }

    pub fn pan_screen(&mut self, delta_x: f32, delta_y: f32) {
        let eye = self.eye_position();
        let forward = (self.pan - eye).normalize();
        let right = forward.cross(vec3(0.0, 1.0, 0.0)).normalize();
        let up = right.cross(forward).normalize();
        let scale = self.distance * 0.0016;
        self.pan_offset -= right * delta_x * scale;
        self.pan_offset += up * delta_y * scale;
    }

    pub fn view_projection(&self) -> Matrix4<f32> {
        self.projection_matrix() * self.view_matrix()
    }

    pub fn view_matrix(&self) -> Matrix4<f32> {
        let eye_position = self.eye_position();
        let eye = Point3::new(eye_position.x, eye_position.y, eye_position.z);
        let target = Point3::new(self.pan.x, self.pan.y, self.pan.z);
        let forward = (self.pan - eye_position).normalize();
        let right = forward.cross(vec3(0.0, 1.0, 0.0)).normalize();
        let base_up = right.cross(forward).normalize();
        let up = (base_up * self.roll.cos() + right * self.roll.sin()).normalize();
        Matrix4::look_at_rh(eye, target, up)
    }

    pub fn eye_position(&self) -> Vector3<f32> {
        let direction = Vector3::new(
            self.yaw.sin() * self.pitch.cos(),
            self.pitch.sin(),
            self.yaw.cos() * self.pitch.cos(),
        );
        let base_eye = self.pan + direction * self.distance;
        let forward = (self.pan - base_eye).normalize();
        let right = forward.cross(vec3(0.0, 1.0, 0.0)).normalize();
        let up = right.cross(forward).normalize();
        base_eye + right * self.view_offset.x + up * self.view_offset.y
    }

    pub fn projection_matrix(&self) -> Matrix4<f32> {
        let half_height = self.distance * 0.5;
        let half_width = half_height * self.aspect;
        OPENGL_TO_WGPU_MATRIX
            * ortho(
                -half_width,
                half_width,
                -half_height,
                half_height,
                self.znear,
                self.zfar,
            )
    }

    pub fn visible_world_height(&self) -> f32 {
        self.distance
    }

    pub fn project(&self, point: Vector3<f32>, width: f32, height: f32) -> Option<[f32; 2]> {
        let clip = self.view_projection() * point.extend(1.0);
        if clip.w <= 0.0 {
            return None;
        }
        let ndc = clip.truncate() / clip.w;
        if !(-1.0..=1.0).contains(&ndc.z) {
            return None;
        }
        Some([(ndc.x + 1.0) * 0.5 * width, (1.0 - ndc.y) * 0.5 * height])
    }
}

fn wrap_angle(angle: f32) -> f32 {
    let tau = std::f32::consts::TAU;
    (angle + std::f32::consts::PI).rem_euclid(tau) - std::f32::consts::PI
}
