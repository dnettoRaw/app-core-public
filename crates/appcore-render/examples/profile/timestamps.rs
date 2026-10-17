// =============================================================================
//        #######
//     ###       ###     F: timestamps.rs
//    ##   ## ##   ##    P: AppCore-Runtime
//         ## ##
//                       C: 2026/10/15 08:11:39 by dnettoRaw
//    ##   ## ##   ##    U: 2026/10/15 10:24:40 by dnettoRaw
//      ###########      S: 0.1.0-alpha.1
// =============================================================================

use appcore_render::{gpu, RenderError};
use std::time::{Duration, Instant};
/// Two timestamps (16 bytes), two indirect records (40), resolve payload (16).
/// Query values add 16 bytes and the 1x1 marker adds 4; driver metadata is excluded.
pub const PAYLOAD_BYTES: u64 = 92;
pub struct Clock {
    queries: gpu::QuerySet,
    resolve: gpu::Buffer,
    staging: gpu::Buffer,
    period: f64,
    marker: gpu::TextureView,
    levels: u32,
}
#[derive(Debug)]
pub struct Measurement {
    pub gpu_ns: f64,
    pub visible: [u32; 2],
    pub levels: [[u32; 2]; 2],
}
impl Clock {
    pub fn new(device: &gpu::Device, queue: &gpu::Queue, levels: u32) -> Result<Self, RenderError> {
        if !(1..=2).contains(&levels) {
            return Err(RenderError::InvalidLimits);
        }
        let period = f64::from(queue.get_timestamp_period());
        if !period.is_finite() || period <= 0. {
            return Err(RenderError::DeviceUnavailable);
        }
        let queries = device.create_query_set(&gpu::QuerySetDescriptor {
            label: Some("E3DN profile timestamps"),
            ty: gpu::QueryType::Timestamp,
            count: 2,
        });
        let buffer = |size, usage| {
            device.create_buffer(&gpu::BufferDescriptor {
                label: Some("E3DN profile capture"),
                size,
                usage,
                mapped_at_creation: false,
            })
        };
        let marker = device
            .create_texture(&gpu::TextureDescriptor {
                label: Some("E3DN timestamp marker"),
                size: gpu::Extent3d {
                    width: 1,
                    height: 1,
                    depth_or_array_layers: 1,
                },
                mip_level_count: 1,
                sample_count: 1,
                dimension: gpu::TextureDimension::D2,
                format: gpu::TextureFormat::Rgba8Unorm,
                usage: gpu::TextureUsages::RENDER_ATTACHMENT,
                view_formats: &[],
            })
            .create_view(&Default::default());
        Ok(Self {
            marker,
            levels,
            queries,
            resolve: buffer(
                16,
                gpu::BufferUsages::QUERY_RESOLVE | gpu::BufferUsages::COPY_SRC,
            ),
            staging: buffer(
                16 + u64::from(levels) * 40,
                gpu::BufferUsages::COPY_DST | gpu::BufferUsages::MAP_READ,
            ),
            period,
        })
    }
    pub fn payload_bytes(&self) -> u64 {
        PAYLOAD_BYTES + u64::from(self.levels - 1) * 40
    }
    pub fn begin(&self, encoder: &mut gpu::CommandEncoder) {
        self.marker(encoder, 0);
    }
    pub fn end(&self, encoder: &mut gpu::CommandEncoder) {
        self.marker(encoder, 1);
    }
    fn marker(&self, encoder: &mut gpu::CommandEncoder, index: u32) {
        let _pass = encoder.begin_render_pass(&gpu::RenderPassDescriptor {
            label: Some("E3DN timestamp marker"),
            color_attachments: &[Some(gpu::RenderPassColorAttachment {
                view: &self.marker,
                resolve_target: None,
                depth_slice: None,
                ops: gpu::Operations {
                    load: gpu::LoadOp::Clear(gpu::Color::BLACK),
                    store: gpu::StoreOp::Store,
                },
            })],
            depth_stencil_attachment: None,
            occlusion_query_set: None,
            timestamp_writes: Some(gpu::RenderPassTimestampWrites {
                query_set: &self.queries,
                beginning_of_pass_write_index: None,
                end_of_pass_write_index: Some(index),
            }),
        });
    }
    pub fn capture(
        &self,
        device: &gpu::Device,
        queue: &gpu::Queue,
        arguments: [&gpu::Buffer; 2],
    ) -> Result<Measurement, RenderError> {
        // Resolve is encoded only after host-observed sampling completion, not merely queue order.
        let (tx, rx) = std::sync::mpsc::sync_channel(1);
        queue.on_submitted_work_done(move || {
            let _ = tx.send(Ok(()));
        });
        self.wait(device, &rx)?;
        let mut encoder = device.create_command_encoder(&Default::default());
        encoder.resolve_query_set(&self.queries, 0..2, &self.resolve, 0);
        encoder.copy_buffer_to_buffer(&self.resolve, 0, &self.staging, 0, 16);
        for (i, source) in arguments.into_iter().enumerate() {
            encoder.copy_buffer_to_buffer(
                source,
                0,
                &self.staging,
                16 + i as u64 * 20 * u64::from(self.levels),
                20 * u64::from(self.levels),
            );
        }
        queue.submit([encoder.finish()]);
        let (tx, rx) = std::sync::mpsc::sync_channel(1);
        self.staging
            .slice(..)
            .map_async(gpu::MapMode::Read, move |result| {
                let _ = tx.send(result);
            });
        if let Err(error) = self.wait(device, &rx) {
            self.staging.unmap();
            return Err(error);
        }
        let mapped = self.staging.slice(..).get_mapped_range();
        let result = decode(&mapped, self.period, self.levels);
        drop(mapped);
        self.staging.unmap();
        result
    }
    fn wait(
        &self,
        device: &gpu::Device,
        rx: &std::sync::mpsc::Receiver<Result<(), gpu::BufferAsyncError>>,
    ) -> Result<(), RenderError> {
        let deadline = Instant::now() + Duration::from_secs(5);
        loop {
            device
                .poll(gpu::PollType::Poll)
                .map_err(|_| RenderError::DeviceUnavailable)?;
            match rx.try_recv() {
                Ok(Ok(())) => return Ok(()),
                Ok(Err(_)) | Err(std::sync::mpsc::TryRecvError::Disconnected) => {
                    return Err(RenderError::CaptureFailed)
                }
                Err(std::sync::mpsc::TryRecvError::Empty) => {}
            }
            if Instant::now() >= deadline {
                return Err(RenderError::CaptureTimeout);
            }
            std::thread::sleep(Duration::from_millis(1));
        }
    }
}
fn decode(bytes: &[u8], period: f64, levels: u32) -> Result<Measurement, RenderError> {
    if !(1..=2).contains(&levels)
        || bytes.len() != 16 + levels as usize * 40
        || !period.is_finite()
        || period <= 0.
    {
        return Err(RenderError::CaptureFailed);
    }
    let read64 = |offset| u64::from_le_bytes(std::array::from_fn(|i| bytes[offset + i]));
    let (start, end) = (read64(0), read64(8));
    if [start, end]
        .iter()
        .any(|value| *value == 0 || *value == u64::MAX)
    {
        eprintln!("GPU timestamp missing or sentinel; sample rejected");
        return Err(RenderError::CaptureFailed);
    }
    let ticks = end
        .checked_sub(start)
        .filter(|v| *v > 0)
        .ok_or(RenderError::CaptureFailed)?;
    let mut counts = [[0u32; 2]; 2];
    for (batch, group) in counts.iter_mut().enumerate() {
        for (level, count) in group.iter_mut().enumerate().take(levels as usize) {
            let offset = 16 + (batch * levels as usize + level) * 20 + 4;
            *count = u32::from_le_bytes(std::array::from_fn(|i| bytes[offset + i]));
            if *count > 1024 {
                return Err(RenderError::CaptureFailed);
            }
        }
    }
    let visible = counts.map(|group| group.iter().sum::<u32>());
    if visible.iter().any(|count| *count > 1024) {
        return Err(RenderError::CaptureFailed);
    }
    Ok(Measurement {
        gpu_ns: ticks as f64 * period,
        visible,
        levels: counts,
    })
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn rejects_missing_reversed_or_zero_timestamps_and_invalid_counts() {
        let mut bytes = [0; 56];
        for (start, end) in [
            (0u64, 0u64),
            (0, 19),
            (9, 0),
            (9, 3),
            (9, 9),
            (9, u64::MAX),
            (u64::MAX, u64::MAX),
        ] {
            bytes[..8].copy_from_slice(&start.to_le_bytes());
            bytes[8..16].copy_from_slice(&end.to_le_bytes());
            assert!(decode(&bytes, 1., 1).is_err());
        }
        bytes[..8].copy_from_slice(&9u64.to_le_bytes());
        bytes[8..16].copy_from_slice(&19u64.to_le_bytes());
        bytes[20..24].copy_from_slice(&1024u32.to_le_bytes());
        bytes[40..44].copy_from_slice(&512u32.to_le_bytes());
        let sample = decode(&bytes, 2., 1).unwrap();
        assert_eq!(sample.gpu_ns, 20.);
        assert_eq!(sample.visible, [1024, 512]);
        assert!(decode(&bytes[..55], 1., 1).is_err());
        for period in [f64::NAN, f64::INFINITY, 0., -1.] {
            assert!(decode(&bytes, period, 1).is_err());
        }
        bytes[20..24].copy_from_slice(&1025u32.to_le_bytes());
        assert!(decode(&bytes, 1., 1).is_err());
    }
    #[test]
    fn packed_lod_records_sum_per_batch_and_reject_duplicate_selection_over_capacity() {
        let mut bytes = [0; 96];
        bytes[..8].copy_from_slice(&9u64.to_le_bytes());
        bytes[8..16].copy_from_slice(&19u64.to_le_bytes());
        for (offset, count) in [(20, 512u32), (40, 512), (60, 0), (80, 1024)] {
            bytes[offset..offset + 4].copy_from_slice(&count.to_le_bytes());
        }
        let value = decode(&bytes, 1., 2).unwrap();
        assert_eq!(value.visible, [1024, 1024]);
        assert_eq!(value.levels, [[512, 512], [0, 1024]]);
        bytes[20..24].copy_from_slice(&513u32.to_le_bytes());
        assert!(decode(&bytes, 1., 2).is_err());
        assert!(decode(&bytes, 1., 3).is_err());
        assert!(decode(&bytes, 1., 0).is_err());
    }
}
