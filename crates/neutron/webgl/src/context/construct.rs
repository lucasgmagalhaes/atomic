//! `WebGl` struct definition and construction — split out from `context.rs`.

pub struct WebGl {
    pub(super) device: wgpu::Device,
    pub(super) queue: wgpu::Queue,
}

impl WebGl {
    pub fn new() -> Self {
        pollster::block_on(Self::new_async())
    }

    pub(super) async fn new_async() -> Self {
        let instance = wgpu::Instance::new(wgpu::InstanceDescriptor {
            backends: wgpu::Backends::PRIMARY,
            ..Default::default()
        });
        let adapter = instance
            .request_adapter(&wgpu::RequestAdapterOptions::default())
            .await
            .expect(
                "no wgpu adapter available - this needs a GPU (or software fallback) on the host",
            );
        let (device, queue) = adapter
            .request_device(&wgpu::DeviceDescriptor::default(), None)
            .await
            .expect("failed to get wgpu device");
        WebGl { device, queue }
    }
}

impl Default for WebGl {
    fn default() -> Self {
        Self::new()
    }
}
