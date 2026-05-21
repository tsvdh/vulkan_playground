mod logic;
mod rendering;
mod shader_modules;
mod ui;
mod timing;
mod material;

use crate::logic::LogicItems;
use crate::rendering::RenderItems;
use crate::timing::TimingItems;
use crate::ui::GuiItems;
use log::info;
use obj::{load_obj, Obj};
use std::fs::File;
use std::io::BufReader;
use std::sync::{Arc};
use std::time::{Instant};
use std::env;
use vulkan_playground::{CommonItems, InitOption};
use vulkano::buffer::{Buffer, BufferCreateInfo, BufferUsage, Subbuffer};
use vulkano::device::{DeviceExtensions, DeviceFeatures, QueueFlags};
use vulkano::memory::allocator::{AllocationCreateInfo, MemoryTypeFilter};
use vulkano::swapchain::{Surface};
use vulkano::sync::future::FenceSignalFuture;
use vulkano::sync::GpuFuture;
use winit::application::ApplicationHandler;
use winit::dpi::PhysicalSize;
use winit::event::WindowEvent;
use winit::event_loop::{ActiveEventLoop, ControlFlow, EventLoop};
use winit::window::{Window, WindowId};

fn main() {
    let event_loop = EventLoop::new().unwrap();
    event_loop.set_control_flow(ControlFlow::Poll);
    let mut app = VulkanApp::new(&event_loop);
    event_loop.run_app(&mut app).unwrap();
}

struct VulkanApp {
    common_items: CommonItems,
    vertex_buffer: Subbuffer<[obj::Vertex]>,
    index_buffer: Subbuffer<[u16]>,
    render_items: InitOption<RenderItems>,
    logic_items: LogicItems,
    gui_items: InitOption<GuiItems>,
    timing_items: TimingItems,
}

type GpuFence = FenceSignalFuture<Box<dyn GpuFuture + Send>>;

impl VulkanApp {

    fn new(event_loop: &EventLoop<()>) -> Self {
        let instance_extensions = Surface::required_extensions(event_loop).unwrap();
        let device_extensions = DeviceExtensions {
            khr_swapchain: true,
            khr_dynamic_rendering: true,
            ..DeviceExtensions::empty()
        };
        let device_features = DeviceFeatures {
            dynamic_rendering: true,
            ..DeviceFeatures::empty()
        };

        let vulkan_items = vulkan_playground::get_common_vulkan_items(
            Some(instance_extensions),
            Some(device_extensions),
            Some(device_features),
            QueueFlags::GRAPHICS,
            Some(event_loop)
        );

        let working_dir = env::current_dir().unwrap();
        let obj_path = working_dir.join("resources/bunny_face_normals.obj");
        info!("Reading object at {:?}", obj_path);
        let buf_reader = BufReader::new(File::open(obj_path).unwrap());
        let obj: Obj<obj::Vertex, u16> = load_obj(buf_reader).unwrap();

        let vertex_buffer = Buffer::from_iter(
            vulkan_items.memory_allocator.clone(),
            BufferCreateInfo {
                usage: BufferUsage::VERTEX_BUFFER,
                ..Default::default()
            },
            AllocationCreateInfo {
                memory_type_filter: MemoryTypeFilter::PREFER_DEVICE | MemoryTypeFilter::HOST_SEQUENTIAL_WRITE,
                ..Default::default()
            },
            obj.vertices
        ).unwrap();

        let index_buffer = Buffer::from_iter(
            vulkan_items.memory_allocator.clone(),
            BufferCreateInfo {
                usage: BufferUsage::INDEX_BUFFER,
                ..Default::default()
            },
            AllocationCreateInfo {
                memory_type_filter: MemoryTypeFilter::PREFER_DEVICE | MemoryTypeFilter::HOST_SEQUENTIAL_WRITE,
                ..Default::default()
            },
            obj.indices
        ).unwrap();

        let logic_items = LogicItems::new();
        let timing_items = TimingItems::new();

        VulkanApp {
            common_items: vulkan_items,
            vertex_buffer,
            index_buffer,
            render_items: InitOption::none(),
            logic_items,
            gui_items: InitOption::none(),
            timing_items,
        }
    }
}

impl ApplicationHandler for VulkanApp {

    fn resumed(&mut self, event_loop: &ActiveEventLoop) {
        let window_attributes = Window::default_attributes()
            .with_title("Vulkan Playground")
            .with_inner_size(PhysicalSize::new(800, 600))
            .with_visible(false);
        let window = Arc::new(event_loop.create_window(window_attributes).unwrap());

        self.render_items = InitOption::some(RenderItems::new(&self.common_items, window.clone()));
        if self.render_items.swapchain.image_count() != 2 {
            panic!("Swapchain should contain exactly two images");
        }
        self.gui_items = InitOption::some(GuiItems::new(
            event_loop, &self.common_items, &self.render_items));

        // first frame render prep
        self.gui_items.build_ui();
        self.logic_items.base_logic(&mut self.timing_items, &self.render_items);

        window.set_visible(true);
    }

    fn window_event(&mut self, event_loop: &ActiveEventLoop, _window_id: WindowId, event: WindowEvent) {
        if self.gui_items.gui.update(&event) {
            return;
        }

        match event {
            WindowEvent::CloseRequested => {
                event_loop.exit();
            }
            WindowEvent::Resized(_) => {
                self.render_items.set_recreate_swapchain(true);
            }
            WindowEvent::MouseInput {device_id: _, state: _, button: _} => {

            }
            WindowEvent::KeyboardInput { device_id: _, event, is_synthetic: _} => {
                self.logic_items.process_keyboard_input(event);
            }
            WindowEvent::RedrawRequested => {
                if !self.timing_items.new_frame_start(&self.logic_items) {
                    return
                }
                self.logic_items.increment_frame_id();

                // new frame start

                let gpu_prep_start = Instant::now();
                let acquire_future = match self.render_items.frame_rendering_prep(
                    &self.common_items,
                    &mut self.timing_items,
                ) {
                    None => return,
                    Some(result) => result,
                };
                self.timing_items.frame_component_durations.gpu_prep_duration = Some(gpu_prep_start.elapsed());

                self.timing_items.get_async_logic_prod().send(()).unwrap();
                *self.timing_items.get_async_cpu_start_mutex() = Instant::now();

                let render_cpu_start = Instant::now();
                self.render_items.frame_render(
                    &self.common_items,
                    &mut self.timing_items,
                    &self.logic_items,
                    &mut self.gui_items,
                    acquire_future,
                    self.vertex_buffer.clone(),
                    self.index_buffer.clone(),
                );
                self.timing_items.frame_component_durations.render_cpu_duration = Some(render_cpu_start.elapsed());
                *self.timing_items.get_render_gpu_start_mutex() = Instant::now();

                let ui_start = Instant::now();
                self.gui_items.build_ui();
                self.timing_items.frame_component_durations.ui_duration = Some(ui_start.elapsed());

                let logic_start = Instant::now();
                self.logic_items.base_logic(&mut self.timing_items, &self.render_items);
                self.timing_items.frame_component_durations.base_logic_duration = Some(logic_start.elapsed());
            }
            _ => {}
        }
    }

    fn about_to_wait(&mut self, _event_loop: &ActiveEventLoop) {
        self.render_items.window.request_redraw();
    }
}
