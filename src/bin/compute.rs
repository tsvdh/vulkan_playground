use std::process::exit;
use std::time::Instant;
use log::{info};
use vulkano::buffer::{Buffer, BufferCreateInfo, BufferUsage};
use vulkano::command_buffer::{AutoCommandBufferBuilder, CommandBufferUsage};
use vulkano::descriptor_set::{DescriptorSet, WriteDescriptorSet};
use vulkano::memory::allocator::{AllocationCreateInfo, MemoryTypeFilter};
use vulkano::pipeline::{ComputePipeline, Pipeline, PipelineBindPoint, PipelineLayout, PipelineShaderStageCreateInfo};
use vulkano::pipeline::compute::ComputePipelineCreateInfo;
use vulkano::pipeline::layout::PipelineDescriptorSetLayoutCreateInfo;
use vulkano::{sync};
use vulkano::device::QueueFlags;
use vulkano::sync::GpuFuture;

const BATCH_SIZE: u32 = 1024;
const NUM_BATCHES: u32 = 2u32.pow(14);
const NUM_VALUES: u32 = BATCH_SIZE * NUM_BATCHES;

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
    } = vulkan_playground::get_common_vulkan_items(None, None, None, QueueFlags::COMPUTE, None);

    let gpu_setup_start = Instant::now();

    let content = 0..NUM_VALUES;
    let buffer = Buffer::from_iter(
        memory_allocator.clone(),
        BufferCreateInfo {
            usage: BufferUsage::STORAGE_BUFFER,
            ..Default::default()
        },
        AllocationCreateInfo {
            memory_type_filter: MemoryTypeFilter::PREFER_DEVICE
                | MemoryTypeFilter::HOST_RANDOM_ACCESS,
            ..Default::default()
        },
        content
    ).expect("Failed to create buffer");

    mod compute_shader_module {
        vulkano_shaders::shader!{
            ty: "compute",
            path: r"shaders\compute.glsl",
        }
    }
    let compute_shader_module = compute_shader_module::load(device.clone()).expect("Failed to create shader module");

    let compute_shader = compute_shader_module.entry_point("main").unwrap();
    let stage_create_info = PipelineShaderStageCreateInfo::new(compute_shader);
    let pipeline_layout = PipelineLayout::new(
        device.clone(),
        PipelineDescriptorSetLayoutCreateInfo::from_stages([&stage_create_info])
            .into_pipeline_layout_create_info(device.clone()).unwrap()
    ).unwrap();

    let compute_pipeline = ComputePipeline::new(
        device.clone(), None,
        ComputePipelineCreateInfo::stage_layout(stage_create_info, pipeline_layout.clone())
    ).expect("Failed to create compute pipeline");

    let descriptor_set_layouts = pipeline_layout.set_layouts();
    let descriptor_set_layout = descriptor_set_layouts.get(0).unwrap();
    let descriptor_set = DescriptorSet::new(
        descriptor_set_allocator.clone(),
        descriptor_set_layout.clone(),
        [WriteDescriptorSet::buffer(0, buffer.clone())],
        []
    ).unwrap();

    let mut command_buffer_builder = AutoCommandBufferBuilder::primary(
        command_buffer_allocator.clone(),
        queue.queue_family_index(),
        CommandBufferUsage::OneTimeSubmit
    ).unwrap();

    let work_group_counts = [NUM_VALUES / BATCH_SIZE, 1, 1];

    unsafe {
        command_buffer_builder
            .bind_pipeline_compute(compute_pipeline.clone()).unwrap()
            .bind_descriptor_sets(
                PipelineBindPoint::Compute,
                pipeline_layout.clone(),
                0u32,
                descriptor_set.clone()
            ).unwrap()
            .dispatch(work_group_counts).unwrap();
    }
    let command_buffer = command_buffer_builder.build().unwrap();

    info!("GPU setup took: {}ms", gpu_setup_start.elapsed().as_millis());
    let gpu_execution_start = Instant::now();

    let future = sync::now(device.clone())
        .then_execute(queue.clone(), command_buffer.clone()).unwrap()
        .then_signal_fence_and_flush().unwrap();

    future.wait(None).unwrap();

    info!("GPU execution took: {}ms", gpu_execution_start.elapsed().as_millis());

    info!("Checking GPU...");
    let buffer_content = buffer.read().unwrap();
    for (i, item) in buffer_content.iter().enumerate() {
        assert_eq!(*item, (i * 2) as u32);
    }
    info!("Done");

    let cpu_setup_start = Instant::now();
    let mut cpu_content = (0..NUM_VALUES).collect::<Vec<_>>();
    info!("CPU setup took: {}ms", cpu_setup_start.elapsed().as_millis());

    let cpu_execution_start = Instant::now();
    for i in 0..cpu_content.len() {
        cpu_content[i] *= 2;
    }
    info!("CPU execution took: {}ms", cpu_execution_start.elapsed().as_millis());
}