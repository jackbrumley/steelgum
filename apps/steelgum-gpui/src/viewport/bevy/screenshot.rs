use std::sync::mpsc::{self, Receiver};
use std::time::Duration;

use anyhow::{Context, Result, ensure};
use image::{ExtendedColorType, ImageEncoder, codecs::png::PngEncoder};

const READBACK_TIMEOUT: Duration = Duration::from_secs(10);

pub(super) struct Readback {
    device: wgpu::Device,
    buffer: wgpu::Buffer,
    submission: wgpu::SubmissionIndex,
    mapped: Receiver<Result<(), wgpu::BufferAsyncError>>,
    width: u32,
    height: u32,
    row_bytes: u32,
    padded_row_bytes: u32,
}

impl Readback {
    pub(super) fn start(
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        texture: &wgpu::Texture,
    ) -> Result<Self> {
        let (width, height) = (texture.width(), texture.height());
        ensure!(
            texture.format() == wgpu::TextureFormat::Rgba8UnormSrgb,
            "Unsupported viewport capture format"
        );
        let row_bytes = width * 4;
        let alignment = wgpu::COPY_BYTES_PER_ROW_ALIGNMENT;
        let padded_row_bytes = row_bytes.div_ceil(alignment) * alignment;
        let size = u64::from(padded_row_bytes) * u64::from(height);
        ensure!(
            size <= device.limits().max_buffer_size,
            "The viewport is too large to capture"
        );
        let buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("Steelgum viewport screenshot"),
            size,
            usage: wgpu::BufferUsages::MAP_READ | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        let mut encoder = device.create_command_encoder(&wgpu::CommandEncoderDescriptor {
            label: Some("Steelgum viewport screenshot"),
        });
        encoder.copy_texture_to_buffer(
            wgpu::TexelCopyTextureInfo {
                texture,
                mip_level: 0,
                origin: wgpu::Origin3d::ZERO,
                aspect: wgpu::TextureAspect::All,
            },
            wgpu::TexelCopyBufferInfo {
                buffer: &buffer,
                layout: wgpu::TexelCopyBufferLayout {
                    offset: 0,
                    bytes_per_row: Some(padded_row_bytes),
                    rows_per_image: Some(height),
                },
            },
            texture.size(),
        );
        // Submit before returning to the event loop so this matches the frame
        // the reviewer was looking at when they requested the capture.
        let submission = queue.submit([encoder.finish()]);
        let (sender, mapped) = mpsc::channel();
        buffer
            .slice(..)
            .map_async(wgpu::MapMode::Read, move |result| {
                let _ = sender.send(result);
            });
        Ok(Self {
            device: device.clone(),
            buffer,
            submission,
            mapped,
            width,
            height,
            row_bytes,
            padded_row_bytes,
        })
    }

    pub(super) fn png(self) -> Result<Vec<u8>> {
        self.device
            .poll(wgpu::PollType::Wait {
                submission_index: Some(self.submission),
                timeout: Some(READBACK_TIMEOUT),
            })
            .context("Could not wait for the viewport capture")?;
        self.mapped
            .recv_timeout(READBACK_TIMEOUT)
            .context("Did not receive the viewport capture")?
            .context("Could not map the viewport capture")?;

        let mapped = self.buffer.slice(..).get_mapped_range()?;
        let pixel_count = usize::try_from(u64::from(self.row_bytes) * u64::from(self.height))?;
        let mut pixels = Vec::with_capacity(pixel_count);
        for row in mapped.chunks_exact(self.padded_row_bytes as usize) {
            pixels.extend_from_slice(&row[..self.row_bytes as usize]);
        }
        drop(mapped);
        self.buffer.unmap();

        let mut png = Vec::new();
        PngEncoder::new(&mut png)
            .write_image(&pixels, self.width, self.height, ExtendedColorType::Rgba8)
            .context("Could not encode the viewport screenshot")?;
        Ok(png)
    }
}
