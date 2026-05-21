use image::{ImageBuffer, Rgba};
use log::info;
use vulkano::memory::allocator::{AllocationCreateInfo, MemoryTypeFilter,};
use vulkano::{sync,};
use vulkano::buffer::{Buffer, BufferCreateInfo, BufferUsage};
use vulkano::command_buffer::{AutoCommandBufferBuilder, CommandBufferUsage, CopyImageToBufferInfo};
use vulkano::descriptor_set::{DescriptorSet, WriteDescriptorSet};
use vulkano::device::QueueFlags;
use vulkano::format::{Format};
use vulkano::image::{Image, ImageCreateInfo, ImageType, ImageUsage};
use vulkano::image::view::ImageView;
use vulkano::pipeline::{ComputePipeline, Pipeline, PipelineBindPoint, PipelineLayout, PipelineShaderStageCreateInfo};
use vulkano::pipeline::compute::ComputePipelineCreateInfo;
use vulkano::pipeline::layout::PipelineDescriptorSetLayoutCreateInfo;
use vulkano::sync::GpuFuture;

const RESOLUTION: [u32; 2] = [8 * 128, 8 * 128];

fn main() {
    let vulkan_playground::CommonItems {
        library: _,
        instance: _,
        debug_callback: _,
        device,
        queue,
        memory_allocator,
        descriptor_set_allocator,
        command_buffer_allocator,
        uniform_buffer_allocator: _,
    } = vulkan_playground::get_common_vulkan_items(None, None, None, QueueFlags::GRAPHICS, None);

    mod image_shader_module {
        vulkano_shaders::shader!{
            ty: "compute",
            path: r"shaders\image_compute.glsl",
        }
    }
    let shader_module = image_shader_module::load(device.clone()).expect("Failed to create shader module");

    let image_shader = shader_module.entry_point("main").unwrap();
    let stage = PipelineShaderStageCreateInfo::new(image_shader);
    let pipeline_layout = PipelineLayout::new(
        device.clone(),
        PipelineDescriptorSetLayoutCreateInfo::from_stages([&stage])
            .into_pipeline_layout_create_info(device.clone()).unwrap()
    ).unwrap();

    let compute_pipeline = ComputePipeline::new(
        device.clone(), None,
        ComputePipelineCreateInfo::stage_layout(stage, pipeline_layout.clone())
    ).expect("Failed to create compute pipeline");

    let image = Image::new(
        memory_allocator.clone(),
        ImageCreateInfo {
            image_type: ImageType::Dim2d,
            format: Format::R8G8B8A8_UNORM,
            extent: [RESOLUTION[0], RESOLUTION[1], 1],
            usage: ImageUsage::STORAGE | ImageUsage::TRANSFER_SRC,
            ..Default::default()
        },
        AllocationCreateInfo {
            memory_type_filter: MemoryTypeFilter::PREFER_DEVICE,
            ..Default::default()
        }
    ).unwrap();
    let view = ImageView::new_default(image.clone()).unwrap();

    let descriptor_set_layouts = pipeline_layout.set_layouts();
    let descriptor_set_layout = descriptor_set_layouts.get(0).unwrap();
    let descriptor_set = DescriptorSet::new(
        descriptor_set_allocator.clone(),
        descriptor_set_layout.clone(),
        [WriteDescriptorSet::image_view(0, view.clone())],
        []
    ).unwrap();

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

    let mut command_buffer_builder = AutoCommandBufferBuilder::primary(
        command_buffer_allocator.clone(),
        queue.queue_family_index(),
        CommandBufferUsage::OneTimeSubmit
    ).unwrap();

    unsafe {
        command_buffer_builder
            .bind_pipeline_compute(compute_pipeline.clone()).unwrap()
            .bind_descriptor_sets(
                PipelineBindPoint::Compute,
                pipeline_layout.clone(),
                0,
                descriptor_set
            ).unwrap()
            .dispatch([RESOLUTION[0] / 8, RESOLUTION[1] / 8, 1]).unwrap()
            .copy_image_to_buffer(
                CopyImageToBufferInfo::image_buffer(image.clone(), buffer.clone())
            ).unwrap();
    }

    let command_buffer = command_buffer_builder.build().unwrap();

    let future = sync::now(device.clone())
        .then_execute(queue.clone(), command_buffer.clone()).unwrap()
        .then_signal_fence_and_flush().unwrap();

    future.wait(None).unwrap();

    let buffer_content = buffer.read().unwrap();
    let image = ImageBuffer::<Rgba<u8>, _>::from_raw(
        RESOLUTION[0], RESOLUTION[1], &buffer_content[..]
    ).unwrap();
    image.save("image_compute.png").unwrap();

    info!("Success")
}