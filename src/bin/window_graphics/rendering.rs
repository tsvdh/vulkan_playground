use crate::logic::LogicItems;
use crate::shader_modules::fragment_shader_module::FragmentData;
use crate::shader_modules::vertex_shader_module::VertexData;
use crate::shader_modules::{fragment_shader_module, vertex_shader_module};
use crate::timing::TimingItems;
use crate::ui::GuiItems;
use log::{info, warn};
use std::sync::Arc;
use vulkan_playground::CommonItems;
use vulkano::buffer::Subbuffer;
use vulkano::command_buffer::{AutoCommandBufferBuilder, CommandBufferUsage, RenderingAttachmentInfo, RenderingInfo};
use vulkano::descriptor_set::{DescriptorSet, WriteDescriptorSet};
use vulkano::format::Format;
use vulkano::image::view::ImageView;
use vulkano::image::{Image, ImageCreateInfo, ImageType, ImageUsage};
use vulkano::memory::allocator::AllocationCreateInfo;
use vulkano::pipeline::graphics::color_blend::{ColorBlendAttachmentState, ColorBlendState};
use vulkano::pipeline::graphics::depth_stencil::{DepthState, DepthStencilState};
use vulkano::pipeline::graphics::input_assembly::InputAssemblyState;
use vulkano::pipeline::graphics::multisample::MultisampleState;
use vulkano::pipeline::graphics::rasterization::RasterizationState;
use vulkano::pipeline::graphics::subpass::PipelineRenderingCreateInfo;
use vulkano::pipeline::graphics::vertex_input::{Vertex, VertexDefinition};
use vulkano::pipeline::graphics::viewport::{Viewport, ViewportState};
use vulkano::pipeline::graphics::GraphicsPipelineCreateInfo;
use vulkano::pipeline::layout::PipelineDescriptorSetLayoutCreateInfo;
use vulkano::pipeline::{DynamicState, GraphicsPipeline, Pipeline, PipelineBindPoint, PipelineLayout, PipelineShaderStageCreateInfo};
use vulkano::render_pass::{AttachmentLoadOp, AttachmentStoreOp};
use vulkano::swapchain::{acquire_next_image, PresentMode, Surface, Swapchain, SwapchainAcquireFuture, SwapchainCreateInfo, SwapchainPresentInfo};
use vulkano::sync::GpuFuture;
use vulkano::{Validated, VulkanError};
use winit::window::Window;

pub struct RenderItems {
    pub window: Arc<Window>,
    pub swapchain: Arc<Swapchain>,

    recreate_swapchain: bool,

    color_attachment_image_views: Vec<Arc<ImageView>>,
    depth_attachment_image_view: Arc<ImageView>,
    pipeline: Arc<GraphicsPipeline>,
    viewport: Viewport,
    vertex_shader_uniform_buffer: Subbuffer<VertexData>,
    fragment_shader_uniform_buffer: Subbuffer<FragmentData>,
}

impl RenderItems {

    pub fn set_recreate_swapchain(&mut self, value: bool) {
        self.recreate_swapchain = value;
    }

    pub fn new(vulkan_items: &CommonItems, window: Arc<Window>) -> Self {
        let surface = Surface::from_window(vulkan_items.instance.clone(), window.clone()).unwrap();

        let (swapchain, images) = {
            let surface_capabilities = vulkan_items.device.physical_device()
                .surface_capabilities(&surface, Default::default()).unwrap();

            let (image_format, _) = vulkan_items.device.physical_device()
                .surface_formats(&surface, Default::default()).unwrap()[0];

            Swapchain::new(
                vulkan_items.device.clone(),
                surface.clone(),
                SwapchainCreateInfo {
                    min_image_count: surface_capabilities.min_image_count.max(2),
                    image_format,
                    image_extent: window.inner_size().into(),
                    image_usage: ImageUsage::COLOR_ATTACHMENT,
                    present_mode: PresentMode::Mailbox,
                    ..Default::default()
                }
            ).unwrap()
        };

        let (color_image_views, depth_image_view) = Self::make_image_views(&vulkan_items, &images);

        let pipeline = {
            let vertex_shader_module = vertex_shader_module::load(vulkan_items.device.clone()).expect("Failed to create vertex shader");
            let fragment_shader_module = fragment_shader_module::load(vulkan_items.device.clone()).expect("Failed to create fragment shader");
            let vertex_shader = vertex_shader_module.entry_point("main").unwrap();
            let fragment_shader = fragment_shader_module.entry_point("main").unwrap();

            let vertex_input_state = obj::Vertex::per_vertex().definition(&vertex_shader).unwrap();

            let stages = [
                PipelineShaderStageCreateInfo::new(vertex_shader),
                PipelineShaderStageCreateInfo::new(fragment_shader)
            ];

            let layout = PipelineLayout::new(
                vulkan_items.device.clone(),
                PipelineDescriptorSetLayoutCreateInfo::from_stages(&stages)
                    .into_pipeline_layout_create_info(vulkan_items.device.clone()).unwrap()
            ).unwrap();

            let dynamic_rendering_info = PipelineRenderingCreateInfo {
                color_attachment_formats: vec![Some(swapchain.image_format())],
                depth_attachment_format: Some(Format::D16_UNORM),
                ..Default::default()
            };

            GraphicsPipeline::new(
                vulkan_items.device.clone(),
                None,
                GraphicsPipelineCreateInfo {
                    stages: stages.into_iter().collect(),
                    vertex_input_state: Some(vertex_input_state),
                    input_assembly_state: Some(InputAssemblyState::default()),
                    viewport_state: Some(ViewportState::default()),
                    rasterization_state: Some(RasterizationState::default()),
                    depth_stencil_state: Some(DepthStencilState {
                        depth: Some(DepthState::simple()),
                        ..Default::default()
                    }),
                    multisample_state: Some(MultisampleState::default()),
                    color_blend_state: Some(ColorBlendState::with_attachment_states(
                        dynamic_rendering_info.color_attachment_formats.len() as u32,
                        ColorBlendAttachmentState::default()
                    )),
                    dynamic_state: [DynamicState::Viewport].into_iter().collect(),
                    subpass: Some(dynamic_rendering_info.into()),
                    ..GraphicsPipelineCreateInfo::layout(layout.clone())
                }
            ).unwrap()
        };

        let viewport = Viewport {
            offset: [0.0, 0.0],
            extent: window.inner_size().into(),
            depth_range: 0.0..=1.0
        };

        let vertex_shader_uniform_buffer = vulkan_items.uniform_buffer_allocator.allocate_sized().unwrap();
        let fragment_shader_uniform_buffer = vulkan_items.uniform_buffer_allocator.allocate_sized().unwrap();

        RenderItems {
            window,
            swapchain,
            color_attachment_image_views: color_image_views,
            depth_attachment_image_view: depth_image_view,
            pipeline,
            viewport,
            recreate_swapchain: false,
            vertex_shader_uniform_buffer,
            fragment_shader_uniform_buffer,
        }
    }

    pub fn frame_rendering_prep(&mut self,
                                vulkan_items: &CommonItems,
                                timing_items: &mut TimingItems
    ) -> Option<SwapchainAcquireFuture>
    {
        let new_window_size = self.window.inner_size();
        if new_window_size.width == 0 {
            return None;
        }

        let mut frame_render_end_mutex = timing_items.get_frame_render_end_mutex();
        if frame_render_end_mutex.is_some() {
            frame_render_end_mutex.as_mut().unwrap().cleanup_finished();
        }
        drop(frame_render_end_mutex);

        if self.recreate_swapchain {
            info!("Recreating swapchain");
            let (new_swapchain, new_images) = self.swapchain.recreate(
                SwapchainCreateInfo {
                    image_extent: new_window_size.into(),
                    ..self.swapchain.create_info()
                }
            ).unwrap();

            self.swapchain = new_swapchain;
            (self.color_attachment_image_views,
             self.depth_attachment_image_view) = Self::make_image_views(vulkan_items, &new_images);
            self.viewport.extent = new_window_size.into();
            self.recreate_swapchain = false;
        }

        let (_image_index, suboptimal, acquire_future) =
            match acquire_next_image(self.swapchain.clone(), None).map_err(Validated::unwrap) {
                Ok(result) => result,
                Err(VulkanError::OutOfDate) => {
                    self.recreate_swapchain = true;
                    return None;
                },
                Err(error) => panic!("Failed to acquire next image: {error}")
            };

        if suboptimal {
            self.recreate_swapchain = true;
            return None;
        }

        Some(acquire_future)
    }

    pub fn frame_render(&mut self,
                        vulkan_items: &CommonItems,
                        timing_items: &mut TimingItems,
                        logic_items: &LogicItems,
                        gui_items: &mut GuiItems,
                        acquire_future: SwapchainAcquireFuture,
                        vertex_buffer: Subbuffer<[obj::Vertex]>,
                        index_buffer: Subbuffer<[u16]>,
    ) {
        *self.vertex_shader_uniform_buffer.write().unwrap() = *logic_items.get_vertex_shader_uniform();
        *self.fragment_shader_uniform_buffer.write().unwrap() = *logic_items.get_fragment_shader_uniform();

        let descriptor_set_layout = self.pipeline.layout().set_layouts()[0].clone();
        let descriptor_set = DescriptorSet::new(
            vulkan_items.descriptor_set_allocator.clone(),
            descriptor_set_layout.clone(),
            [
                WriteDescriptorSet::buffer(0, self.vertex_shader_uniform_buffer.clone()),
                WriteDescriptorSet::buffer(1, self.fragment_shader_uniform_buffer.clone())
            ],
            []
        ).unwrap();

        let image_index = acquire_future.image_index();
        let image_view = self.color_attachment_image_views[image_index as usize].clone();

        let mut command_buffer_builder = AutoCommandBufferBuilder::primary(
            vulkan_items.command_buffer_allocator.clone(),
            vulkan_items.queue.queue_family_index(),
            CommandBufferUsage::OneTimeSubmit
        ).unwrap();

        command_buffer_builder
            .begin_rendering(
                RenderingInfo {
                    color_attachments: vec![Some(RenderingAttachmentInfo {
                        load_op: AttachmentLoadOp::Clear,
                        store_op: AttachmentStoreOp::Store,
                        clear_value: Some([0.0, 0.0, 0.0, 1.0].into()),
                        ..RenderingAttachmentInfo::image_view(image_view.clone())
                    })],
                    depth_attachment: Some(RenderingAttachmentInfo {
                        load_op: AttachmentLoadOp::Clear,
                        store_op: AttachmentStoreOp::DontCare,
                        clear_value: Some(1f32.into()),
                        ..RenderingAttachmentInfo::image_view(self.depth_attachment_image_view.clone())
                    }),
                    ..Default::default()
                }
            ).unwrap()
            .set_viewport(0, [self.viewport.clone()].into_iter().collect()).unwrap()
            .bind_pipeline_graphics(self.pipeline.clone()).unwrap()
            .bind_descriptor_sets(PipelineBindPoint::Graphics, self.pipeline.layout().clone(), 0, descriptor_set).unwrap()
            .bind_vertex_buffers(0, vertex_buffer.clone()).unwrap()
            .bind_index_buffer(index_buffer.clone()).unwrap();

        unsafe { command_buffer_builder.draw_indexed(index_buffer.len() as u32, 1, 0, 0, 0).unwrap(); }

        command_buffer_builder
            .end_rendering().unwrap();

        let command_buffer = command_buffer_builder.build().unwrap();

        let scene_future = acquire_future
            .then_execute(vulkan_items.queue.clone(), command_buffer.clone()).unwrap();

        let complete_future = gui_items.gui
            .draw_on_image(scene_future, image_view.clone())
            .then_swapchain_present(vulkan_items.queue.clone(),
                                    SwapchainPresentInfo::swapchain_image_index(self.swapchain.clone(), image_index))
            .boxed_send()
            .then_signal_fence_and_flush();

        match complete_future.map_err(Validated::unwrap) {
            Ok(future) => {
                *timing_items.get_frame_render_end_mutex() = Some(future);
            }
            Err(error) => {
                if error == VulkanError::OutOfDate {
                    self.recreate_swapchain = true;
                }
                *timing_items.get_frame_render_end_mutex() = None;

                warn!("Rendering failed: {error}");
            }
        }
    }

    fn make_image_views(vulkan_items: &CommonItems, images: &[Arc<Image>]) -> (Vec<Arc<ImageView>>, Arc<ImageView>) {
        let color_image_views = images.iter().map(|image| {
            ImageView::new_default(image.clone()).unwrap()
        }).collect();

        let depth_image_view = ImageView::new_default(
            Image::new(
                vulkan_items.memory_allocator.clone(),
                ImageCreateInfo {
                    image_type: ImageType::Dim2d,
                    format: Format::D16_UNORM,
                    extent: images[0].extent(),
                    usage: ImageUsage::DEPTH_STENCIL_ATTACHMENT | ImageUsage::TRANSIENT_ATTACHMENT,
                    ..Default::default()
                },
                AllocationCreateInfo::default()
            ).unwrap()
        ).unwrap();

        (color_image_views, depth_image_view)
    }

}