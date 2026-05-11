use std::f32::consts::FRAC_PI_2;
use glam::{Mat4, Vec3};
use winit::event::{KeyEvent};
use winit::keyboard::{PhysicalKey};
use winit::keyboard::KeyCode::{ArrowDown, ArrowLeft, ArrowRight, ArrowUp, KeyT, PageDown, PageUp};
use crate::{App};
use crate::shader_modules::{fragment_shader_module, vertex_shader_module};

impl App {

    pub fn process_keyboard_input(&mut self, event: KeyEvent) {
        if event.repeat == true {
            return;
        }

        match event.physical_key {
            PhysicalKey::Code(key_code) => {
                if event.state.is_pressed() {
                    self.logic_items.keys_pressed.insert(key_code);
                    self.logic_items.keys_down.insert(key_code);
                } else {
                    self.logic_items.keys_down.remove(&key_code);
                }
            }
            PhysicalKey::Unidentified(_) => {}
        }
    }

    fn handle_input(&mut self, frame_duration: f32) {
        let keys_pressed = &self.logic_items.keys_pressed;
        let keys_down = &self.logic_items.keys_down;

        if keys_pressed.contains(&KeyT) {
            self.logic_items.show_frame_times = !self.logic_items.show_frame_times;
        }

        // camera controls
        // rotate 90 degrees (pi/2) in 1 sec
        // zoom 1m in 1 sec

        let mut vertical_angle_diff = FRAC_PI_2 * frame_duration;
        let mut horizontal_angle_diff = FRAC_PI_2 * frame_duration;
        if keys_down.contains(&ArrowDown) {
            vertical_angle_diff *= -1.0;
        }
        if keys_down.contains(&ArrowLeft) {
            horizontal_angle_diff *= -1.0;
        }

        if keys_down.contains(&ArrowUp) || keys_down.contains(&ArrowDown) {
            self.logic_items.eye_pos = self.logic_items.eye_pos.rotate_axis(self.logic_items.eye_horizon, vertical_angle_diff);
        }
        if keys_down.contains(&ArrowLeft) || keys_down.contains(&ArrowRight) {
            self.logic_items.eye_pos = self.logic_items.eye_pos.rotate_y(horizontal_angle_diff);
            self.logic_items.eye_horizon = self.logic_items.eye_horizon.rotate_y(horizontal_angle_diff);
        }

        let mut distance_diff = 1.0 * frame_duration;
        if keys_down.contains(&PageDown) {
            distance_diff *= -1.0;
        }

        if keys_down.contains(&PageUp) || keys_down.contains(&PageDown) {
            self.logic_items.eye_pos += (Vec3::ZERO - self.logic_items.eye_pos).normalize() * distance_diff;
        }
    }

    fn make_mvp_matrix(&self) -> Mat4 {
        let image_extent = self.render_context.as_ref().unwrap().swapchain.image_extent();
        let aspect_ratio = image_extent[0] as f32 / image_extent[1] as f32;
        let projection = Mat4::perspective_lh(
            FRAC_PI_2,
            aspect_ratio,
            0.1,
            1000.0
        );

        let view = Mat4::look_at_lh(
            self.logic_items.eye_pos,
            Vec3::ZERO,
            Vec3::NEG_Y
        );

        let model = Mat4::IDENTITY;

        projection * (view * model)
    }

    pub fn base_logic(&mut self) {
        let frame_duration = self.get_frame_duration();

        self.handle_input(frame_duration);
        
        self.logic_items.vertex_shader_uniform = Some(vertex_shader_module::VertexData {
            mvp: self.make_mvp_matrix().to_cols_array_2d(),
        });
        
        self.logic_items.fragment_shader_uniform = Some(fragment_shader_module::FragmentData {
            light_pos: self.logic_items.light_pos.to_array().into(),
            eye_pos: self.logic_items.eye_pos.to_array(),
        });

        self.logic_items.keys_pressed.clear();
    }
}