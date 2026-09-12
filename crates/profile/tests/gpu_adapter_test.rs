use profile::Profile;

mod common;
use common::*;

#[test]
fn spawn_with_gpu_adapter_zero_opens_a_real_adapter_and_renders_correctly() {
    // Adapter 0 always exists if this dev machine can run any of this
    // workspace's other GPU tests at all - proves the index actually
    // reaches `render::GpuRenderer::new_with_adapter`, not just that the
    // worker started.
    let name = unique_shmem_name("gpu-adapter");
    let profile = Profile::spawn_with_gpu_adapter(worker_path(), &name, 32, 32, Some(0))
        .expect("spawn with a real adapter index should succeed");
    wait_for_a_frame(&profile);

    // The demo page's own real render - not a specific expected color, just
    // proof the process is alive, rendering, and didn't panic on startup.
    assert!(profile.latest_frame().is_some());

    profile.quit();
}

#[test]
fn spawn_with_an_out_of_range_gpu_adapter_index_never_publishes_a_frame() {
    // `ipc::FrameReader::new` creates the shared-memory region itself if it
    // attaches before any writer does (see that method's own doc), so
    // `spawn` succeeding here doesn't mean the worker is healthy - the
    // real proof the out-of-range index actually reached
    // `GpuRenderer::new_with_adapter` and panicked (no real machine
    // enumerates 9999 adapters) is that no frame ever gets published,
    // since the worker crashes before it ever reaches its own
    // `FrameWriter::new`/`publish` call.
    let name = unique_shmem_name("gpu-adapter-bad");
    let profile = Profile::spawn_with_gpu_adapter(worker_path(), &name, 32, 32, Some(9999))
        .expect("spawn itself succeeds - the region self-creates on the reader side");

    std::thread::sleep(std::time::Duration::from_millis(500));
    assert_eq!(
        profile.frame_generation(),
        0,
        "a worker that panicked opening the adapter should never publish a frame"
    );
    assert!(profile.latest_frame().is_none());
}
