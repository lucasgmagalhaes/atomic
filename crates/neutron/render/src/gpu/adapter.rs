//! GPU adapter identity/enumeration — split out from `gpu.rs`.

/// One real GPU adapter's identity, for a caller (Settings > Performance
/// > GPU) to show actual hardware options instead of fake ones. Mirrors
/// the fields of `wgpu::AdapterInfo` this crate's callers actually need
/// to display/distinguish adapters by, without leaking the `wgpu` type
/// itself into every caller's dependency surface.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AdapterInfo {
    pub name: String,
    /// e.g. "Vulkan", "Dx12", "Metal", "Gl" — `wgpu::Backend`'s own
    /// `Display` impl, not reinvented here.
    pub backend: String,
    pub device_type: AdapterDeviceType,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AdapterDeviceType {
    DiscreteGpu,
    IntegratedGpu,
    VirtualGpu,
    Cpu,
    Other,
}

impl From<wgpu::DeviceType> for AdapterDeviceType {
    fn from(dt: wgpu::DeviceType) -> Self {
        match dt {
            wgpu::DeviceType::DiscreteGpu => AdapterDeviceType::DiscreteGpu,
            wgpu::DeviceType::IntegratedGpu => AdapterDeviceType::IntegratedGpu,
            wgpu::DeviceType::VirtualGpu => AdapterDeviceType::VirtualGpu,
            wgpu::DeviceType::Cpu => AdapterDeviceType::Cpu,
            wgpu::DeviceType::Other => AdapterDeviceType::Other,
        }
    }
}

/// Real enumeration of every GPU adapter `wgpu` can see on this machine
/// (`wgpu::Instance::enumerate_adapters` + each `Adapter::get_info()`) -
/// not a fixed/fake list. Order matches what [`super::GpuRenderer::
/// new_with_adapter`]'s `index` indexes into. Synchronous - enumeration
/// itself doesn't need `request_adapter`'s async device negotiation.
pub fn list_adapters() -> Vec<AdapterInfo> {
    let instance = wgpu::Instance::new(wgpu::InstanceDescriptor {
        backends: wgpu::Backends::PRIMARY,
        ..Default::default()
    });
    instance
        .enumerate_adapters(wgpu::Backends::PRIMARY)
        .into_iter()
        .map(|adapter| {
            let info = adapter.get_info();
            AdapterInfo {
                name: info.name,
                backend: info.backend.to_string(),
                device_type: info.device_type.into(),
            }
        })
        .collect()
}
