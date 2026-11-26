use anyhow::{anyhow, Context, Result};
use ash::{vk, Entry};
use std::ffi::CStr;

struct Device {
    index: usize,
    description: String,
    kind: i32,
    parakeet_compatible: bool,
}

fn devices() -> Result<Vec<Device>> {
    // The entry stays alive until after the instance is destroyed. Properties
    // and feature data are copied before returning.
    unsafe {
        let entry = Entry::load().context("loading Vulkan driver")?;
        let application = vk::ApplicationInfo::default().api_version(vk::API_VERSION_1_2);
        let info = vk::InstanceCreateInfo::default().application_info(&application);
        let instance = entry
            .create_instance(&info, None)
            .map_err(|error| anyhow!("creating Vulkan instance: {error:?}"))?;
        let result = instance
            .enumerate_physical_devices()
            .map_err(|error| anyhow!("enumerating Vulkan devices: {error:?}"))
            .map(|physical_devices| {
                physical_devices
                    .into_iter()
                    .enumerate()
                    .map(|(index, device)| {
                        let properties = instance.get_physical_device_properties(device);
                        let description = CStr::from_ptr(properties.device_name.as_ptr())
                            .to_string_lossy()
                            .into_owned();
                        let compute = instance
                            .get_physical_device_queue_family_properties(device)
                            .iter()
                            .any(|queue| queue.queue_flags.contains(vk::QueueFlags::COMPUTE));
                        let kind = match (compute, properties.device_type) {
                            (true, vk::PhysicalDeviceType::DISCRETE_GPU) => 1,
                            (true, vk::PhysicalDeviceType::INTEGRATED_GPU) => 2,
                            _ => 0,
                        };
                        let mut vulkan11 = vk::PhysicalDeviceVulkan11Features::default();
                        let mut features =
                            vk::PhysicalDeviceFeatures2::default().push_next(&mut vulkan11);
                        instance.get_physical_device_features2(device, &mut features);
                        Device {
                            index,
                            description,
                            kind,
                            parakeet_compatible: properties.api_version >= vk::API_VERSION_1_2
                                && vulkan11.storage_buffer16_bit_access != vk::FALSE,
                        }
                    })
                    .collect()
            });
        instance.destroy_instance(None);
        result
    }
}

pub fn parakeet_devices() -> Result<Vec<(String, String, i32)>> {
    Ok(devices()?
        .into_iter()
        .map(|device| {
            (
                format!("Vulkan{}", device.index),
                device.description,
                if device.parakeet_compatible {
                    device.kind
                } else {
                    0
                },
            )
        })
        .collect())
}

#[cfg(target_os = "linux")]
pub fn require_webgpu_adapter() -> Result<()> {
    let devices =
        devices().context("gpu_unavailable: WebGPU could not enumerate Vulkan devices")?;
    anyhow::ensure!(
        devices.iter().any(|device| device.kind != 0),
        "gpu_unavailable: WebGPU requires an available physical GPU"
    );
    Ok(())
}
