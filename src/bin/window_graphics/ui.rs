use egui_winit_vulkano::{Gui, GuiConfig};
use vulkano::image::SampleCount;
use winit::event_loop::ActiveEventLoop;
use crate::App;

impl App {

    pub fn init_egui(&mut self, event_loop: &ActiveEventLoop) {
        let render_context = self.render_context.as_ref().unwrap();
        let egui_config = GuiConfig {
            allow_srgb_render_target: true,
            is_overlay: true,
            samples: SampleCount::Sample1,
        };
        self.egui = Some(Gui::new(
            event_loop,
            render_context.swapchain.surface().clone(),
            self.vulkan_items.queue.clone(),
            render_context.swapchain.image_format(),
            egui_config
        ));
    }

    pub fn build_ui(&mut self) {
        self.egui.as_mut().unwrap().immediate_ui(|egui| {
            let egui_context = egui.context();
            egui::Window::new("Hello world").show(&egui_context, |_ui| {});
        });
    }

}