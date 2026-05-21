use crate::rendering::RenderItems;
use egui_winit_vulkano::{Gui, GuiConfig};
use vulkan_playground::CommonItems;
use vulkano::image::SampleCount;
use winit::event_loop::ActiveEventLoop;

pub struct GuiItems {
    pub gui: Gui
}

impl GuiItems {

    pub fn new(event_loop: &ActiveEventLoop,
               vulkan_items: &CommonItems,
               render_items: &RenderItems,
    ) -> GuiItems
    {
        let swapchain = render_items.swapchain.clone();
        let egui_config = GuiConfig {
            allow_srgb_render_target: true,
            is_overlay: true,
            samples: SampleCount::Sample1,
        };
        
        let gui = Gui::new(
            event_loop,
            swapchain.surface().clone(),
            vulkan_items.queue.clone(),
            swapchain.image_format(),
            egui_config
        );
        
        GuiItems {
            gui
        }
    }

    pub fn build_ui(&mut self) {
        self.gui.immediate_ui(|egui| {
            let egui_context = egui.context();
            egui::Window::new("Hello world").show(&egui_context, |_ui| {});
        });
    }

}