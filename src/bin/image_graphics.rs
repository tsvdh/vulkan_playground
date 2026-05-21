use image::{ImageBuffer, Rgba};
use log::info;
use vulkano::buffer::{Buffer, BufferContents, BufferCreateInfo, BufferUsage};
use vulkano::command_buffer::{AutoCommandBufferBuilder, CommandBufferUsage, CopyImageToBufferInfo, RenderPassBeginInfo, SubpassBeginInfo, SubpassContents, SubpassEndInfo};
use vulkano::format::Format;
use vulkano::image::{Image, ImageCreateInfo, ImageType, ImageUsage};
use vulkano::image::view::ImageView;
use vulkano::memory::allocator::{AllocationCreateInfo, MemoryTypeFilter};
use vulkano::pipeline::graphics::vertex_input::{Vertex, VertexDefinition};
use vulkano::pipeline::graphics::viewport::{Viewport, ViewportState};
use vulkano::pipeline::{GraphicsPipeline, PipelineLayout, PipelineShaderStageCreateInfo};
use vulkano::pipeline::graphics::color_blend::{ColorBlendAttachmentState, ColorBlendState};
use vulkano::pipeline::graphics::GraphicsPipelineCreateInfo;
use vulkano::pipeline::graphics::input_assembly::InputAssemblyState;
use vulkano::pipeline::graphics::multisample::MultisampleState;
use vulkano::pipeline::graphics::rasterization::RasterizationState;
use vulkano::pipeline::layout::{PipelineDescriptorSetLayoutCreateInfo};
use vulkano::render_pass::{Framebuffer, FramebufferCreateInfo, Subpass};
use vulkano::{single_pass_renderpass, sync};
use vulkano::device::QueueFlags;
use vulkano::sync::GpuFuture;

const RESOLUTION: [u32; 2] = [8 * 128, 8 * 128];

#[derive(BufferContents, Vertex)]
#[repr(C)]
struct BasicVertex {
    #[format(R32G32_SFLOAT)]
    position: [f32; 2]
}

fn main() {
    let vulkan_playground::CommonItems {
        library: _,
        instance: _,
        debug_callback: _,
        device,
        queue,
        memory_allocator,
        descriptor_set_allocator: _,
        command_buffer_allocator,
        uniform_buffer_allocator: _,
    } = vulkan_playground::get_common_vulkan_items(None, None, None, QueueFlags::GRAPHICS, None);

    let vertex1 = BasicVertex { position: [0.0, -0.5]};
    let vertex2 = BasicVertex { position: [0.5, 0.0]};
    let vertex3 = BasicVertex { position: [-0.5, 0.0]};
    let vertex4 = BasicVertex { position: [0.0, 0.5]};
    let vertex5 = BasicVertex { position: [-0.5, 0.0]};
    let vertex6 = BasicVertex { position: [0.5, 0.0]};

    let vertex_buffer = Buffer::from_iter(
        memory_allocator.clone(),
        BufferCreateInfo {
            usage: BufferUsage::VERTEX_BUFFER,
            ..Default::default()
        },
        AllocationCreateInfo {
            memory_type_filter: MemoryTypeFilter::PREFER_DEVICE | MemoryTypeFilter::HOST_SEQUENTIAL_WRITE,
            ..Default::default()
        },
        vec![vertex1, vertex2, vertex3, vertex4, vertex5, vertex6]
    ).unwrap();

    let render_pass = single_pass_renderpass!(
        device.clone(),
        attachments: {
            color: {
                format: Format::R8G8B8A8_UNORM,
                samples: 1,
                load_op: Clear,
                store_op: Store
            }
        },
        pass: {
            color: [color],
            depth_stencil: {}
        }
    ).unwrap();

    let image = Image::new(
        memory_allocator.clone(),
        ImageCreateInfo {
            image_type: ImageType::Dim2d,
            format: Format::R8G8B8A8_UNORM,
            extent: [RESOLUTION[0], RESOLUTION[1], 1],
            usage: ImageUsage::COLOR_ATTACHMENT | ImageUsage::TRANSFER_SRC,
            ..Default::default()
        },
        AllocationCreateInfo {
            memory_type_filter: MemoryTypeFilter::PREFER_DEVICE,
            ..Default::default()
        }
    ).unwrap();
    let view = ImageView::new_default(image.clone()).unwrap();

    let buffer = Buffer::from_iter(
        memory_allocator.clone(),
        BufferCreateInfo {
            usage: BufferUsage::TRANSFER_DST,
            ..Default::default()
        },
        AllocationCreateInfo {
            memory_type_filter: MemoryTypeFilter::PREFER_HOST | MemoryTypeFilter::HOST_RANDOM_ACCESS,
            ..Default::default()
        },
        (0..RESOLUTION[0] * RESOLUTION[1] * 4).map(|_| {0u8})
    ).expect("Failed to create buffer");

    let framebuffer = Framebuffer::new(
        render_pass.clone(),
        FramebufferCreateInfo {
            attachments: vec![view],
            ..Default::default()
        }
    ).unwrap();

    mod vertex_shader_module {
        vulkano_shaders::shader! {
            ty: "vertex",
            path: "shaders/image_graphics/shader.vert"
        }
    }
    mod fragment_shader_module {
        vulkano_shaders::shader! {
            ty: "fragment",
            path: "shaders/image_graphics/shader.frag"
        }
    }
    let vertex_shader_module = vertex_shader_module::load(device.clone()).expect("Failed to create vertex shader");
    let fragment_shader_module = fragment_shader_module::load(device.clone()).expect("Failed to create fragment shader");
    let vertex_shader = vertex_shader_module.entry_point("main").unwrap();
    let fragment_shader = fragment_shader_module.entry_point("main").unwrap();

    let viewport = Viewport {
        offset: [0.0, 0.0],
        extent: [RESOLUTION[0] as f32, RESOLUTION[1] as f32],
        depth_range: 0.0..=1.0
    };

    let vertex_input_state = BasicVertex::per_vertex()
        .definition(&vertex_shader)
        .unwrap();

    let stages = [
        PipelineShaderStageCreateInfo::new(vertex_shader),
        PipelineShaderStageCreateInfo::new(fragment_shader)
    ];

    let pipeline_layout = PipelineLayout::new(
        device.clone(),
        PipelineDescriptorSetLayoutCreateInfo::from_stages(&stages)
            .into_pipeline_layout_create_info(device.clone()).unwrap()
    ).unwrap();

    let subpass = Subpass::from(render_pass.clone(), 0).unwrap();

    let graphics_pipeline = GraphicsPipeline::new(
        device.clone(),
        None,
        GraphicsPipelineCreateInfo {
            stages: stages.into_iter().collect(),
            vertex_input_state: Some(vertex_input_state),
            input_assembly_state: Some(InputAssemblyState::default()),
            viewport_state: Some(ViewportState {
                viewports: [viewport].into_iter().collect(),
                ..Default::default()
            }),
            rasterization_state: Some(RasterizationState::default()),
            multisample_state: Some(MultisampleState::default()),
            color_blend_state: Some(ColorBlendState::with_attachment_states(
                subpass.num_color_attachments(),
                ColorBlendAttachmentState::default()
            )),
            subpass: Some(subpass.into()),
            ..GraphicsPipelineCreateInfo::layout(pipeline_layout)
        }
    ).unwrap();

    let mut command_buffer_builder = AutoCommandBufferBuilder::primary(
        command_buffer_allocator.clone(),
        queue.queue_family_index(),
        CommandBufferUsage::OneTimeSubmit
    ).unwrap();

    unsafe {
        command_buffer_builder
            .begin_render_pass(
                RenderPassBeginInfo {
                    clear_values: vec![Some([0.0, 0.0, 0.0, 1.0].into())],
                    ..RenderPassBeginInfo::framebuffer(framebuffer.clone())
                },
                SubpassBeginInfo {
                    contents: SubpassContents::Inline,
                    ..Default::default()
                }
            ).unwrap()
            .bind_pipeline_graphics(graphics_pipeline.clone()).unwrap()
            .bind_vertex_buffers(0, vertex_buffer.clone()).unwrap()
            .draw(6, 1, 0, 0).unwrap()
            .end_render_pass(SubpassEndInfo::default()).unwrap()
            .copy_image_to_buffer(CopyImageToBufferInfo::image_buffer(image.clone(), buffer.clone())).unwrap();
    }

    let command_buffer = command_buffer_builder.build().unwrap();

    let future = sync::now(device.clone())
        .then_execute(queue.clone(), command_buffer.clone()).unwrap()
        .then_signal_fence_and_flush().unwrap();

    future.wait(None).unwrap();

    let buffer_content = buffer.read().unwrap();
    let image_buffer = ImageBuffer::<Rgba<u8>, _>::from_raw(
        RESOLUTION[0], RESOLUTION[1], &buffer_content[..]
    ).unwrap();
    image_buffer.save("image_graphics.png").unwrap();

    info!("Success")
}