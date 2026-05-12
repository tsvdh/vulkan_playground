use std::sync::{Arc};
use log::{info, warn};
use vulkano::image::{Image, ImageCreateInfo, ImageType, ImageUsage};
use vulkano::pipeline::{DynamicState, GraphicsPipeline, Pipeline, PipelineBindPoint, PipelineLayout, PipelineShaderStageCreateInfo};
use vulkano::pipeline::graphics::color_blend::{ColorBlendAttachmentState, ColorBlendState};
use vulkano::pipeline::graphics::GraphicsPipelineCreateInfo;
use vulkano::pipeline::graphics::input_assembly::InputAssemblyState;
use vulkano::pipeline::graphics::multisample::MultisampleState;
use vulkano::pipeline::graphics::rasterization::RasterizationState;
use vulkano::pipeline::graphics::subpass::{PipelineRenderingCreateInfo};
use vulkano::pipeline::graphics::vertex_input::{Vertex, VertexDefinition};
use vulkano::pipeline::graphics::viewport::{Viewport, ViewportState};
use vulkano::pipeline::layout::PipelineDescriptorSetLayoutCreateInfo;
use vulkano::swapchain::{acquire_next_image, PresentMode, Surface, Swapchain, SwapchainAcquireFuture, SwapchainCreateInfo, SwapchainPresentInfo};
use vulkano::{Validated, VulkanError};
use vulkano::command_buffer::{AutoCommandBufferBuilder, CommandBufferUsage, RenderingAttachmentInfo, RenderingInfo};
use vulkano::descriptor_set::{DescriptorSet, WriteDescriptorSet};
use vulkano::format::Format;
use vulkano::image::view::ImageView;
use vulkano::memory::allocator::AllocationCreateInfo;
use vulkano::pipeline::graphics::depth_stencil::{DepthState, DepthStencilState};
use vulkano::render_pass::{AttachmentLoadOp, AttachmentStoreOp};
use vulkano::sync::GpuFuture;
use winit::window::Window;
use vulkan_playground::CommonItems;
use crate::{App, RenderContext};
use crate::shader_modules::{fragment_shader_module, vertex_shader_module};

impl App {
    pub fn init_render_context(&mut self, window: Arc<Window>) {
        let surface = Surface::from_window(self.vulkan_items.instance.clone(), window.clone()).unwrap();

        let (swapchain, images) = {
            let surface_capabilities = self.vulkan_items.device.physical_device()
                .surface_capabilities(&surface, Default::default()).unwrap();

            let (image_format, _) = self.vulkan_items.device.physical_device()
                .surface_formats(&surface, Default::default()).unwrap()[0];

            Swapchain::new(
                self.vulkan_items.device.clone(),
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

        let (color_image_views, depth_image_view) = Self::make_image_views(&self.vulkan_items, &images);

        let pipeline = {
            
            let vertex_shader_module = vertex_shader_module::load(self.vulkan_items.device.clone()).expect("Failed to create vertex shader");
            let fragment_shader_module = fragment_shader_module::load(self.vulkan_items.device.clone()).expect("Failed to create fragment shader");
            let vertex_shader = vertex_shader_module.entry_point("main").unwrap();
            let fragment_shader = fragment_shader_module.entry_point("main").unwrap();

            let vertex_input_state = obj::Vertex::per_vertex().definition(&vertex_shader).unwrap();

            let stages = [
                PipelineShaderStageCreateInfo::new(vertex_shader),
                PipelineShaderStageCreateInfo::new(fragment_shader)
            ];

            let layout = PipelineLayout::new(
                self.vulkan_items.device.clone(),
                PipelineDescriptorSetLayoutCreateInfo::from_stages(&stages)
                    .into_pipeline_layout_create_info(self.vulkan_items.device.clone()).unwrap()
            ).unwrap();

            let dynamic_rendering_info = PipelineRenderingCreateInfo {
                color_attachment_formats: vec![Some(swapchain.image_format())],
                depth_attachment_format: Some(Format::D16_UNORM),
                ..Default::default()
            };

            GraphicsPipeline::new(
                self.vulkan_items.device.clone(),
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

        let vertex_shader_uniform_buffer = self.uniform_buffer_allocator.allocate_sized().unwrap();
        let fragment_shader_uniform_buffer = self.uniform_buffer_allocator.allocate_sized().unwrap();

        self.render_context = Some(RenderContext {
            window,
            swapchain,
            color_attachment_image_views: color_image_views,
            depth_attachment_image_view: depth_image_view,
            pipeline,
            viewport,
            recreate_swapchain: false,
            vertex_shader_uniform_buffer,
            fragment_shader_uniform_buffer,
        });
    }

    pub fn frame_rendering_prep(&mut self) -> Option<SwapchainAcquireFuture> {
        let render_context = self.render_context.as_mut().unwrap();

        let new_window_size = render_context.window.inner_size();
        if new_window_size.width == 0 {
            return None;
        }

        let mut previous_frame_render_end = self.timing_items.frame_render_end.lock().unwrap();
        if previous_frame_render_end.is_some() {
            previous_frame_render_end.as_mut().unwrap().cleanup_finished();
        }
        drop(previous_frame_render_end);

        if render_context.recreate_swapchain {
            info!("Recreating swapchain");
            let (new_swapchain, new_images) = render_context.swapchain.recreate(
                SwapchainCreateInfo {
                    image_extent: new_window_size.into(),
                    ..render_context.swapchain.create_info()
                }
            ).unwrap();

            render_context.swapchain = new_swapchain;
            (render_context.color_attachment_image_views,
             render_context.depth_attachment_image_view) = Self::make_image_views(&self.vulkan_items, &new_images);
            render_context.viewport.extent = new_window_size.into();
            render_context.recreate_swapchain = false;
        }

        let (_image_index, suboptimal, acquire_future) =
            match acquire_next_image(render_context.swapchain.clone(), None).map_err(Validated::unwrap) {
                Ok(result) => result,
                Err(VulkanError::OutOfDate) => {
                    render_context.recreate_swapchain = true;
                    return None;
                },
                Err(error) => panic!("Failed to acquire next image: {error}")
            };

        if suboptimal {
            render_context.recreate_swapchain = true;
            return None;
        }

        Some(acquire_future)
    }

    pub fn frame_render(&mut self, acquire_future: SwapchainAcquireFuture) {
        let render_context = self.render_context.as_mut().unwrap();

        *render_context.vertex_shader_uniform_buffer.write().unwrap() = self.logic_items.vertex_shader_uniform.unwrap();
        *render_context.fragment_shader_uniform_buffer.write().unwrap() = self.logic_items.fragment_shader_uniform.unwrap();

        let descriptor_set_layout = render_context.pipeline.layout().set_layouts()[0].clone();
        let descriptor_set = DescriptorSet::new(
            self.vulkan_items.descriptor_set_allocator.clone(),
            descriptor_set_layout.clone(),
            [
                WriteDescriptorSet::buffer(0, render_context.vertex_shader_uniform_buffer.clone()),
                WriteDescriptorSet::buffer(1, render_context.fragment_shader_uniform_buffer.clone())
            ],
            []
        ).unwrap();

        let image_index = acquire_future.image_index();
        let image_view = render_context.color_attachment_image_views[image_index as usize].clone();

        let mut command_buffer_builder = AutoCommandBufferBuilder::primary(
            self.vulkan_items.command_buffer_allocator.clone(),
            self.vulkan_items.queue.queue_family_index(),
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
                        ..RenderingAttachmentInfo::image_view(render_context.depth_attachment_image_view.clone())
                    }),
                    ..Default::default()
                }
            ).unwrap()
            .set_viewport(0, [render_context.viewport.clone()].into_iter().collect()).unwrap()
            .bind_pipeline_graphics(render_context.pipeline.clone()).unwrap()
            .bind_descriptor_sets(PipelineBindPoint::Graphics, render_context.pipeline.layout().clone(), 0, descriptor_set).unwrap()
            .bind_vertex_buffers(0, self.vertex_buffer.clone()).unwrap()
            .bind_index_buffer(self.index_buffer.clone()).unwrap();

        unsafe { command_buffer_builder.draw_indexed(self.index_buffer.len() as u32, 1, 0, 0, 0).unwrap(); }

        command_buffer_builder
            .end_rendering().unwrap();

        let command_buffer = command_buffer_builder.build().unwrap();

        let scene_future = acquire_future
            .then_execute(self.vulkan_items.queue.clone(), command_buffer.clone()).unwrap();

        let complete_future = self.egui.as_mut().unwrap()
            .draw_on_image(scene_future, image_view.clone())
            .then_swapchain_present(self.vulkan_items.queue.clone(),
                                    SwapchainPresentInfo::swapchain_image_index(render_context.swapchain.clone(), image_index))
            .boxed_send()
            .then_signal_fence_and_flush();

        match complete_future.map_err(Validated::unwrap) {
            Ok(future) => {
                *self.timing_items.frame_render_end.lock().unwrap() = Some(future);
            }
            Err(error) => {
                if error == VulkanError::OutOfDate {
                    render_context.recreate_swapchain = true;
                }
                *self.timing_items.frame_render_end.lock().unwrap() = None;

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