//! Shared-memory double-buffered frame transport between a `profile`
//! worker process and its host (the shell). Resolves the "pendente:
//! definir sincronização" note from the spec: double buffering via one
//! atomic index, not a lock — the writer only ever writes into the
//! buffer that *isn't* currently published, so a reader copying the
//! published buffer can never observe a torn (partially-written) frame,
//! no matter when it reads relative to the writer's next publish.
//!
//! Layout in the shared memory region:
//! ```text
//! [ready_index: AtomicU32][generation: AtomicU32][buffer 0][buffer 1]
//! ```
//! Each buffer is `width * height * 4` bytes (RGBA8, row-major - the same
//! format `render::GpuRenderer`/`Canvas2D` already produce). `width`/
//! `height` aren't stored in the region itself: both ends must agree on
//! them out of band (the real system would send this as part of process
//! setup/a resize command; not modeled here, see `profile`).
use std::sync::atomic::{AtomicU32, Ordering};

use shared_memory::{Shmem, ShmemConf, ShmemError};

const HEADER_LEN: usize = 8; // two u32 atomics, no padding needed (4-byte aligned)

fn region_len(width: u32, height: u32) -> usize {
  HEADER_LEN + 2 * (width as usize * height as usize * 4)
}

fn ready_index_ptr(shmem: &Shmem) -> *const AtomicU32 {
  shmem.as_ptr() as *const AtomicU32
}

fn generation_ptr(shmem: &Shmem) -> *const AtomicU32 {
  unsafe { shmem.as_ptr().add(4) as *const AtomicU32 }
}

fn buffer_ptr(shmem: &Shmem, index: u32, frame_len: usize) -> *mut u8 {
  let offset = HEADER_LEN + index as usize * frame_len;
  unsafe { shmem.as_ptr().add(offset) }
}

/// Opens (creating if needed) the named shared-memory region for a
/// `width` × `height` RGBA8 double-buffered frame.
fn open_or_create(name: &str, width: u32, height: u32) -> Result<Shmem, ShmemError> {
  let size = region_len(width, height);
  match ShmemConf::new().size(size).flink(name).create() {
    Ok(shmem) => Ok(shmem),
    Err(ShmemError::LinkExists) => ShmemConf::new().flink(name).open(),
    Err(e) => Err(e),
  }
}

pub struct FrameWriter {
  shmem: Shmem,
  width: u32,
  height: u32,
  frame_len: usize,
  /// The writer tracks its own next-write target locally rather than
  /// re-reading `ready_index` each call - it's the only writer, so
  /// there's nothing to race with on this side.
  next_index: u32,
}

impl FrameWriter {
  pub fn new(name: &str, width: u32, height: u32) -> Result<Self, ShmemError> {
    let shmem = open_or_create(name, width, height)?;
    unsafe {
      (*ready_index_ptr(&shmem)).store(0, Ordering::Relaxed);
      (*generation_ptr(&shmem)).store(0, Ordering::Relaxed);
    }
    Ok(FrameWriter {
      shmem,
      width,
      height,
      frame_len: width as usize * height as usize * 4,
      next_index: 1, // first publish writes into buffer 1, leaving buffer 0's (garbage) initial content unpublished
    })
  }

  pub fn width(&self) -> u32 {
    self.width
  }

  pub fn height(&self) -> u32 {
    self.height
  }

  /// Writes `pixels` (must be exactly `width * height * 4` bytes) into
  /// the back buffer, then atomically publishes it. Never touches the
  /// currently-published buffer, so a concurrent reader is never torn.
  pub fn publish(&mut self, pixels: &[u8]) {
    assert_eq!(
      pixels.len(),
      self.frame_len,
      "pixel buffer size must match width*height*4"
    );

    let dst = buffer_ptr(&self.shmem, self.next_index, self.frame_len);
    unsafe {
      std::ptr::copy_nonoverlapping(pixels.as_ptr(), dst, self.frame_len);
      // Release: everything above (the pixel copy) happens-before a
      // reader's Acquire load of ready_index observes this value.
      (*ready_index_ptr(&self.shmem)).store(self.next_index, Ordering::Release);
      (*generation_ptr(&self.shmem)).fetch_add(1, Ordering::Release);
    }
    self.next_index = 1 - self.next_index;
  }
}

pub struct FrameReader {
  shmem: Shmem,
  width: u32,
  height: u32,
  frame_len: usize,
}

impl FrameReader {
  /// Opens an existing region (or creates it, if the reader happens to
  /// attach before any writer has) - the region's existence doesn't
  /// imply a published frame yet; see `has_frame`.
  pub fn new(name: &str, width: u32, height: u32) -> Result<Self, ShmemError> {
    let shmem = open_or_create(name, width, height)?;
    Ok(FrameReader {
      shmem,
      width,
      height,
      frame_len: width as usize * height as usize * 4,
    })
  }

  pub fn width(&self) -> u32 {
    self.width
  }

  pub fn height(&self) -> u32 {
    self.height
  }

  /// Monotonically increasing count of published frames - `0` means no
  /// writer has published yet. Callers polling for "is there a new
  /// frame since I last checked" compare this against their last seen
  /// value instead of diffing pixels.
  pub fn generation(&self) -> u32 {
    unsafe { (*generation_ptr(&self.shmem)).load(Ordering::Acquire) }
  }

  /// Copies out the currently-published buffer. Always a complete,
  /// non-torn frame (see module docs) - `None` if nothing has been
  /// published yet (buffer 0's contents are otherwise undefined).
  pub fn latest_frame(&self) -> Option<Vec<u8>> {
    if self.generation() == 0 {
      return None;
    }
    let index = unsafe { (*ready_index_ptr(&self.shmem)).load(Ordering::Acquire) };
    let src = buffer_ptr(&self.shmem, index, self.frame_len);
    let mut out = vec![0u8; self.frame_len];
    unsafe {
      std::ptr::copy_nonoverlapping(src, out.as_mut_ptr(), self.frame_len);
    }
    Some(out)
  }
}
