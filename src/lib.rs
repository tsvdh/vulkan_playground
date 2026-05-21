use std::ops::{Deref, DerefMut};
use std::sync::Arc;
use log::{debug, error, info, warn};
use vulkano::buffer::allocator::{SubbufferAllocator, SubbufferAllocatorCreateInfo};
use vulkano::buffer::BufferUsage;
use vulkano::command_buffer::allocator::{StandardCommandBufferAllocator, StandardCommandBufferAllocatorCreateInfo};
use vulkano::descriptor_set::allocator::StandardDescriptorSetAllocator;
use vulkano::device::{Device, DeviceCreateInfo, DeviceExtensions, DeviceFeatures, Queue, QueueCreateInfo, QueueFlags};
use vulkano::device::physical::{PhysicalDeviceType};
use vulkano::instance::debug::{DebugUtilsMessageSeverity, DebugUtilsMessageType, DebugUtilsMessenger, DebugUtilsMessengerCallback, DebugUtilsMessengerCreateInfo};
use vulkano::instance::{Instance, InstanceCreateInfo, InstanceExtensions};
use vulkano::memory::allocator::{MemoryTypeFilter, StandardMemoryAllocator};
use vulkano::VulkanLibrary;
use winit::event_loop::EventLoop;

const DEFAULT_INSTANCE_EXTENSIONS: InstanceExtensions = InstanceExtensions {
    ext_debug_utils: true,
    ..InstanceExtensions::empty()
};
const LAYERS: [&str; 1] = ["VK_LAYER_KHRONOS_validation"];

pub struct CommonItems {
    pub library: Arc<VulkanLibrary>,
    pub instance: Arc<Instance>,
    pub debug_callback: DebugUtilsMessenger,
    pub device: Arc<Device>,
    pub queue: Arc<Queue>,
    pub memory_allocator: Arc<StandardMemoryAllocator>,
    pub descriptor_set_allocator: Arc<StandardDescriptorSetAllocator>,
    pub command_buffer_allocator: Arc<StandardCommandBufferAllocator>,
    pub uniform_buffer_allocator: Arc<SubbufferAllocator>,
}

pub fn get_debug_callback(instance: Arc<Instance>) -> DebugUtilsMessenger {
    pretty_env_logger::init();
    
    unsafe {
        DebugUtilsMessenger::new(
            instance.clone(),
            DebugUtilsMessengerCreateInfo {
                message_severity: DebugUtilsMessageSeverity::ERROR
                    | DebugUtilsMessageSeverity::WARNING
                    | DebugUtilsMessageSeverity::INFO
                    | DebugUtilsMessageSeverity::VERBOSE,
                message_type: DebugUtilsMessageType::GENERAL
                    | DebugUtilsMessageType::PERFORMANCE
                    | DebugUtilsMessageType::VALIDATION,
                ..DebugUtilsMessengerCreateInfo::user_callback(DebugUtilsMessengerCallback::new(
                    |message_severity,
                     message_type,
                     callback_data| {
                        if message_severity.intersects(DebugUtilsMessageSeverity::ERROR) {
                            error!("({:?}) {}", message_type, callback_data.message);
                        } else if message_severity.intersects(DebugUtilsMessageSeverity::WARNING) {
                            warn!("({:?}) {}", message_type, callback_data.message);
                        } else if message_severity.intersects(DebugUtilsMessageSeverity::INFO) {
                            info!("({:?}) {}", message_type, callback_data.message);
                        } else {
                            debug!("({:?}) {}", message_type, callback_data.message);
                        }
                    }
                ))

            }
        ).expect("Failed to create debug callback")
    }
}

pub fn get_common_vulkan_items(instance_extensions: Option<InstanceExtensions>,
                               device_extensions: Option<DeviceExtensions>,
                               device_features: Option<DeviceFeatures>,
                               queue_flag: QueueFlags,
                               event_loop: Option<&EventLoop<()>>
) -> CommonItems {
    let library = VulkanLibrary::new().expect("No local Vulkan library/dll");

    let mut library_layers = library.layer_properties().unwrap();
    LAYERS.iter().for_each(|layer| {
        library_layers.find(|l| {l.name() == *layer})
            .expect(format!("Layer {} not available in library", *layer).as_str());
    });

    let instance = Instance::new(
        library.clone(),
        InstanceCreateInfo {
            enabled_layers: LAYERS.iter().map(|l| {l.to_string()}).collect::<Vec<_>>(),
            enabled_extensions: DEFAULT_INSTANCE_EXTENSIONS.union(&instance_extensions.unwrap_or_default()),
            ..Default::default()
        }
    ).expect("Failed to create instance");

    let debug_callback = get_debug_callback(instance.clone());

    let physical_device = instance
        .enumerate_physical_devices().unwrap()
        .filter(|physical_device|
            physical_device.supported_extensions().contains(&device_extensions.unwrap_or_default()))
        .min_by_key(|physical_device| match physical_device.properties().device_type {
            PhysicalDeviceType::DiscreteGpu => 0,
            PhysicalDeviceType::IntegratedGpu => 1,
            _ => 2,
        }).unwrap();

    let queue_family_index = physical_device
        .queue_family_properties().iter().enumerate()
        .position(|(index, queue_family_properties)| {
            queue_family_properties.queue_flags.contains(queue_flag)
                && event_loop.is_some_and(|event_loop| physical_device.presentation_support(index as u32, event_loop).unwrap())
        })
        .expect("No queue with appropriate support available") as u32;

    let (device, mut queues) = Device::new(
        physical_device.clone(),
        DeviceCreateInfo {
            queue_create_infos: vec![QueueCreateInfo {
                queue_family_index,
                ..Default::default()
            }],
            enabled_extensions: device_extensions.unwrap_or_default(),
            enabled_features: device_features.unwrap_or_default(),
            ..Default::default()
        }
    ).expect("Failed to create device");

    let queue = queues.next().unwrap();

    let memory_allocator = Arc::new(StandardMemoryAllocator::new_default(
        device.clone())
    );
    let descriptor_set_allocator = Arc::new(StandardDescriptorSetAllocator::new(
        device.clone(), Default::default())
    );
    let command_buffer_allocator = Arc::new(StandardCommandBufferAllocator::new(
        device.clone(), StandardCommandBufferAllocatorCreateInfo::default()
    ));
    let uniform_buffer_allocator = Arc::new(SubbufferAllocator::new(
        memory_allocator.clone(),
        SubbufferAllocatorCreateInfo {
            buffer_usage: BufferUsage::UNIFORM_BUFFER,
            memory_type_filter: MemoryTypeFilter::PREFER_DEVICE | MemoryTypeFilter::HOST_SEQUENTIAL_WRITE,
            ..Default::default()
        }
    ));

    CommonItems {
        library,
        instance,
        debug_callback,
        device,
        queue,
        memory_allocator,
        descriptor_set_allocator,
        command_buffer_allocator,
        uniform_buffer_allocator,
    }
}

pub struct InitOption<T> {
    data: Option<T>
}

impl<T> InitOption<T> {

    pub fn none() -> Self {
        InitOption { data: None }
    }

    pub fn some(data: T) -> Self {
        InitOption { data: Some(data) }
    }

    pub fn get_ref(&self) -> &T {
        self.data.as_ref().unwrap()
    }

    pub fn get_mut(&mut self) -> &mut T {
        self.data.as_mut().unwrap()
    }
}

impl<T> Deref for InitOption<T> {
    type Target = T;

    fn deref(&self) -> &Self::Target {
        self.get_ref()
    }
}

impl<T> DerefMut for InitOption<T> {

    fn deref_mut(&mut self) -> &mut Self::Target {
        self.get_mut()
    }
}
