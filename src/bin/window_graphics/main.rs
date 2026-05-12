mod logic;
mod rendering;
mod shader_modules;
mod ui;
mod timing;
mod async_management;

use std::{env, thread};
use std::collections::{BTreeSet, VecDeque};
use std::fs::File;
use std::io::BufReader;
use std::sync::{Arc, Mutex};
use std::sync::mpsc::{channel, Receiver, Sender};
use std::time::{Duration, Instant};
use egui_winit_vulkano::{Gui};
use glam::Vec3;
use log::{info};
use obj::{load_obj, Obj, Vertex};
use vulkano::buffer::{Buffer, BufferCreateInfo, BufferUsage, Subbuffer};
use vulkano::buffer::allocator::{SubbufferAllocator, SubbufferAllocatorCreateInfo};
use vulkano::device::{DeviceExtensions, DeviceFeatures, QueueFlags};
use vulkano::image::view::ImageView;
use vulkano::memory::allocator::{AllocationCreateInfo, MemoryTypeFilter};
use vulkano::pipeline::graphics::viewport::{Viewport};
use vulkano::pipeline::{GraphicsPipeline};
use vulkano::swapchain::{Surface, Swapchain};
use vulkano::sync::future::FenceSignalFuture;
use vulkano::sync::GpuFuture;
use winit::application::ApplicationHandler;
use winit::dpi::PhysicalSize;
use winit::event::{WindowEvent};
use winit::event_loop::{ActiveEventLoop, ControlFlow, EventLoop};
use winit::keyboard::{KeyCode};
use winit::window::{Window, WindowId};
use vulkan_playground::CommonItems;
use crate::shader_modules::vertex_shader_module::VertexData;
use crate::shader_modules::fragment_shader_module::FragmentData;

fn main() {
    let event_loop = EventLoop::new().unwrap();
    event_loop.set_control_flow(ControlFlow::Poll);
    let mut app = App::new(&event_loop);
    event_loop.run_app(&mut app).unwrap();
}

struct App {
    vulkan_items: CommonItems,
    uniform_buffer_allocator: SubbufferAllocator,
    vertex_buffer: Subbuffer<[Vertex]>,
    index_buffer: Subbuffer<[u16]>,
    render_context: Option<RenderContext>,
    logic_items: LogicItems,
    egui: Option<Gui>,
    timing_items: TimingItems,
    async_management: AsyncManagement,
}

struct RenderContext {
    window: Arc<Window>,
    swapchain: Arc<Swapchain>,
    color_attachment_image_views: Vec<Arc<ImageView>>,
    depth_attachment_image_view: Arc<ImageView>,
    pipeline: Arc<GraphicsPipeline>,
    viewport: Viewport,
    recreate_swapchain: bool,
    vertex_shader_uniform_buffer: Subbuffer<VertexData>,
    fragment_shader_uniform_buffer: Subbuffer<FragmentData>,
}

struct LogicItems {
    frame_id: i32,
    show_frame_times: bool,
    keys_pressed: BTreeSet<KeyCode>,
    keys_down: BTreeSet<KeyCode>,
    vertex_shader_uniform: Option<VertexData>,
    fragment_shader_uniform: Option<FragmentData>,
    eye_pos: Vec3,
    eye_horizon: Vec3,
    light_pos: Vec3,
}

struct TimingItems {
    frame_component_durations:FrameComponentDurations,
    frame_render_end: Arc<Mutex<Option<FenceSignalFuture<Box<dyn GpuFuture + Send>>>>>,
    render_gpu_start: Arc<Mutex<Instant>>,
    async_cpu_start: Arc<Mutex<Instant>>,
    frame_start_moments: VecDeque<Instant>,
    min_frame_duration: Duration,
}

struct AsyncManagement {
    async_logic_prod: Sender<()>,
    main_thread_cons: Receiver<(AsynchronousTask, Duration)>,
}

struct FrameComponentDurations {
    base_logic_duration: Option<Duration>,
    async_logic_duration: Option<Duration>,
    ui_duration: Option<Duration>,
    render_cpu_duration: Option<Duration>,
    render_gpu_duration: Option<Duration>,
    gpu_prep_duration: Option<Duration>,
}

enum AsynchronousTask {
    CpuLogic,
    GpuRender,
}

impl App {
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

        let uniform_buffer_allocator = SubbufferAllocator::new(
            vulkan_items.memory_allocator.clone(),
            SubbufferAllocatorCreateInfo {
                buffer_usage: BufferUsage::UNIFORM_BUFFER,
                memory_type_filter: MemoryTypeFilter::PREFER_DEVICE | MemoryTypeFilter::HOST_SEQUENTIAL_WRITE,
                ..Default::default()
            }
        );

        let working_dir = env::current_dir().unwrap();
        let obj_path = working_dir.join("resources/bunny_face_normals.obj");
        info!("Reading object at {:?}", obj_path);
        let buf_reader = BufReader::new(File::open(obj_path).unwrap());
        let obj: Obj<Vertex, u16> = load_obj(buf_reader).unwrap();

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

        let logic_items = LogicItems {
            frame_id: 0,
            show_frame_times: true,
            keys_pressed: BTreeSet::new(),
            keys_down: BTreeSet::new(),
            vertex_shader_uniform: None,
            fragment_shader_uniform: None,
            eye_pos: Vec3::new(0.0, 0.0, -1.5),
            eye_horizon: Vec3::X,
            light_pos: Vec3::new(0.0, 10.0, 0.0),
        };

        let timing_items = TimingItems::new();

        let async_management = AsyncManagement::new(&timing_items);

        App {
            vulkan_items,
            uniform_buffer_allocator,
            vertex_buffer,
            index_buffer,
            render_context: None,
            logic_items,
            egui: None,
            timing_items,
            async_management,
        }
    }
}

impl ApplicationHandler for App {

    fn resumed(&mut self, event_loop: &ActiveEventLoop) {
        let window_attributes = Window::default_attributes()
            .with_title("Vulkan Playground")
            .with_inner_size(PhysicalSize::new(800, 600));
        let window = Arc::new(event_loop.create_window(window_attributes).unwrap());

        self.init_render_context(window.clone());
        if self.render_context.as_ref().unwrap().swapchain.image_count() != 2 {
            panic!("Swapchain should contain exactly two images");
        }

        self.init_egui(event_loop);

        // first frame render prep
        self.build_ui();
        self.base_logic();
    }

    fn window_event(&mut self, event_loop: &ActiveEventLoop, _window_id: WindowId, event: WindowEvent) {
        if self.egui.as_mut().unwrap().update(&event) {
            return;
        }

        match event {
            WindowEvent::CloseRequested => {
                event_loop.exit();
            }
            WindowEvent::Resized(_) => {
                self.render_context.as_mut().unwrap().recreate_swapchain = true;
            }
            WindowEvent::MouseInput {device_id: _, state: _, button: _} => {

            }
            WindowEvent::KeyboardInput { device_id: _, event, is_synthetic: _} => {
                self.process_keyboard_input(event);
            }
            WindowEvent::RedrawRequested => {
                if !self.new_frame_start() {
                    return
                }

                let frame_prep_start = Instant::now();

                if self.logic_items.show_frame_times {
                    info!("Frame {:5} | {}", self.logic_items.frame_id, self.timing_items.frame_component_durations)
                }
                self.timing_items.frame_component_durations = FrameComponentDurations::empty();
                self.logic_items.frame_id += 1;

                // new frame start

                let acquire_future = match self.frame_rendering_prep() {
                    None => return,
                    Some(result) => result,
                };
                self.timing_items.frame_component_durations.gpu_prep_duration = Some(frame_prep_start.elapsed());

                self.async_management.async_logic_prod.send(()).unwrap();
                *self.timing_items.async_cpu_start.lock().unwrap() = Instant::now();

                let render_cpu_start = Instant::now();
                self.frame_render(acquire_future);
                self.timing_items.frame_component_durations.render_cpu_duration = Some(render_cpu_start.elapsed());
                *self.timing_items.render_gpu_start.lock().unwrap() = Instant::now();

                let ui_start = Instant::now();
                self.build_ui();
                self.timing_items.frame_component_durations.ui_duration = Some(ui_start.elapsed());

                let logic_start = Instant::now();
                self.base_logic();
                self.timing_items.frame_component_durations.base_logic_duration = Some(logic_start.elapsed());
            }
            _ => {}
        }
    }

    fn about_to_wait(&mut self, _event_loop: &ActiveEventLoop) {
        self.render_context.as_mut().unwrap().window.request_redraw();
    }
}
