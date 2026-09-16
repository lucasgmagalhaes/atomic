//! Pixel readback/write and image export - `getImageData`/`putImageData`
//! (the raw RGBA8 round trip most other draw ops in this module reuse for
//! their own CPU-side work, e.g. `text::draw_text`), plus `to_png_bytes`/
//! `to_data_url` (real PNG encoding via `image_decode`).
use super::Canvas2D;

impl Canvas2D {
    /// `ctx.getImageData(0, 0, width, height).data` — tightly-packed RGBA8
    /// pixels, row-major top-to-bottom.
    pub fn get_image_data(&self) -> Vec<u8> {
        let unpadded_bytes_per_row = self.width * 4;
        let align = wgpu::COPY_BYTES_PER_ROW_ALIGNMENT;
        let padded_bytes_per_row = unpadded_bytes_per_row.div_ceil(align) * align;

        let output_buffer = self.device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("canvas2d-readback"),
            size: (padded_bytes_per_row * self.height) as wgpu::BufferAddress,
            usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
            mapped_at_creation: false,
        });

        let mut encoder = self
            .device
            .create_command_encoder(&wgpu::CommandEncoderDescriptor { label: None });
        encoder.copy_texture_to_buffer(
            wgpu::ImageCopyTexture {
                texture: &self.texture,
                mip_level: 0,
                origin: wgpu::Origin3d::ZERO,
                aspect: wgpu::TextureAspect::All,
            },
            wgpu::ImageCopyBuffer {
                buffer: &output_buffer,
                layout: wgpu::ImageDataLayout {
                    offset: 0,
                    bytes_per_row: Some(padded_bytes_per_row),
                    rows_per_image: Some(self.height),
                },
            },
            wgpu::Extent3d {
                width: self.width,
                height: self.height,
                depth_or_array_layers: 1,
            },
        );
        self.queue.submit(std::iter::once(encoder.finish()));

        let slice = output_buffer.slice(..);
        let (tx, rx) = std::sync::mpsc::channel();
        slice.map_async(wgpu::MapMode::Read, move |result| {
            tx.send(result).unwrap();
        });
        self.device.poll(wgpu::Maintain::Wait);
        rx.recv().unwrap().expect("failed to map readback buffer");

        let padded = slice.get_mapped_range();
        let mut pixels = Vec::with_capacity((unpadded_bytes_per_row * self.height) as usize);
        for row in 0..self.height as usize {
            let start = row * padded_bytes_per_row as usize;
            let end = start + unpadded_bytes_per_row as usize;
            pixels.extend_from_slice(&padded[start..end]);
        }
        drop(padded);
        output_buffer.unmap();
        pixels
    }

    /// Real PNG encoding (`image_decode::encode_png`) of the current
    /// canvas contents - the shared byte-producing step [`Self::to_data_url`]
    /// (base64-encoded) and the JS binding's `toBlob` (wrapped in a real
    /// `Blob`) both build on. **Always PNG** regardless of what MIME type
    /// a caller asks for - this crate has no JPEG/WebP *encoder*
    /// (`image_decode`'s own `image` crate dependency is decode-only for
    /// those two formats here) - matching this crate's general
    /// "best-effort, no separate error path" convention rather than an
    /// `image/jpeg` MIME lie with truly different bytes underneath.
    pub fn to_png_bytes(&self) -> Vec<u8> {
        let pixels = self.get_image_data();
        image_decode::encode_png(self.width, self.height, &pixels).unwrap_or_default()
    }

    /// `ctx.toDataURL()`/`ctx.toDataURL('image/png')` — [`Self::to_png_bytes`]
    /// plus base64, producing a genuine `data:image/png;base64,...` URL a
    /// page could actually decode - not a stub/placeholder string.
    pub fn to_data_url(&self) -> String {
        use base64::Engine;
        let encoded = base64::engine::general_purpose::STANDARD.encode(self.to_png_bytes());
        format!("data:image/png;base64,{encoded}")
    }

    /// `ctx.putImageData(imageData, x, y)` — writes `rgba` (tightly packed
    /// RGBA8, row-major top-to-bottom, `w * h * 4` bytes, same layout
    /// [`Canvas2D::get_image_data`] returns) directly into the backing
    /// texture at `(x, y)`, clipped to stay inside `[0, width) x [0,
    /// height)` (a region that doesn't fully fit is silently cropped
    /// rather than erroring, matching this crate's general "best-effort,
    /// no separate error path" paint convention). A raw memory write, not
    /// a blended draw — like `clear_rect`'s `REPLACE` pipeline, existing
    /// pixels underneath are fully overwritten, not blended with.
    pub fn put_image_data(&mut self, x: i32, y: i32, w: u32, h: u32, rgba: &[u8]) {
        let src_row_bytes = (w * 4) as usize;
        debug_assert!(rgba.len() >= src_row_bytes * h as usize);

        let dst_x = x.max(0) as u32;
        let dst_y = y.max(0) as u32;
        if dst_x >= self.width || dst_y >= self.height {
            return;
        }
        // How many of `rgba`'s own rows/columns a negative `x`/`y` already
        // skips (0 when `x`/`y` is `>= 0`) - clamping `copy_w`/`copy_h`
        // needs both this *and* how much room is left in the destination
        // canvas, or a negative origin alone (destination clip: none) would
        // read past `rgba`'s own end - see this test suite's own
        // `put_image_data_clips_a_negative_origin` for the regression this
        // closes.
        let src_x_skip = (dst_x as i32 - x) as u32;
        let src_y_skip = (dst_y as i32 - y) as u32;
        let copy_w = w
            .saturating_sub(src_x_skip)
            .min(self.width.saturating_sub(dst_x));
        let copy_h = h
            .saturating_sub(src_y_skip)
            .min(self.height.saturating_sub(dst_y));
        if copy_w == 0 || copy_h == 0 {
            return;
        }

        // A cropped copy (x/y negative, or the region spills past the
        // canvas edge) needs its own tightly-packed buffer - `write_texture`
        // has no "skip these source bytes" concept, only a uniform
        // `bytes_per_row` stride over the *destination* rectangle's own
        // width.
        let src_x_offset = src_x_skip as usize * 4;
        let src_y_offset = src_y_skip as usize;
        let mut cropped = Vec::with_capacity((copy_w * copy_h * 4) as usize);
        for row in 0..copy_h as usize {
            let start = (src_y_offset + row) * src_row_bytes + src_x_offset;
            let end = start + (copy_w * 4) as usize;
            cropped.extend_from_slice(&rgba[start..end]);
        }

        self.queue.write_texture(
            wgpu::ImageCopyTexture {
                texture: &self.texture,
                mip_level: 0,
                origin: wgpu::Origin3d {
                    x: dst_x,
                    y: dst_y,
                    z: 0,
                },
                aspect: wgpu::TextureAspect::All,
            },
            &cropped,
            wgpu::ImageDataLayout {
                offset: 0,
                bytes_per_row: Some(copy_w * 4),
                rows_per_image: Some(copy_h),
            },
            wgpu::Extent3d {
                width: copy_w,
                height: copy_h,
                depth_or_array_layers: 1,
            },
        );
    }
}
