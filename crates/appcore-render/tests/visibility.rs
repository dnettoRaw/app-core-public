// =============================================================================
//        #######
//     ###       ###     F: visibility.rs
//    ##   ## ##   ##    P: AppCore-Runtime
//         ## ##
//                       C: 2026/10/14 17:50:33 by dnettoRaw
//    ##   ## ##   ##    U: 2026/10/15 10:24:40 by dnettoRaw
//      ###########      S: 0.1.0-alpha.1
// =============================================================================

use appcore_render::{BoundingSphere, Frustum, RenderError};
const IDENTITY: [f32; 16] = [
    1.0, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 0.0, 1.0,
];
#[test]
fn frustum_uses_webgpu_near_plane_and_conservative_boundaries() -> Result<(), RenderError> {
    let frustum = Frustum::from_matrix(IDENTITY)?;
    assert!(frustum.intersects(BoundingSphere {
        center: [0.0, 0.0, 0.5],
        radius: 0.1
    })?);
    assert!(!frustum.intersects(BoundingSphere {
        center: [0.0, 0.0, -0.5],
        radius: 0.1
    })?);
    assert!(frustum.intersects(BoundingSphere {
        center: [0.0, 0.0, -0.5],
        radius: 0.5
    })?);
    assert!(!frustum.intersects(BoundingSphere {
        center: [3.0, 0.0, 0.5],
        radius: 0.1
    })?);
    assert!(Frustum::from_matrix([0.0; 16]).is_err());
    assert!(frustum
        .intersects(BoundingSphere {
            center: [f32::NAN, 0.0, 0.0],
            radius: 1.0
        })
        .is_err());
    Ok(())
}
#[cfg(feature = "metal")]
mod metal {
    use super::*;
    use appcore_render::{gpu, GpuVisibility, RenderLimits};
    use std::{
        future::Future,
        task::{Context, Poll, Waker},
        time::{Duration, Instant},
    };
    fn run<F: Future>(future: F) -> F::Output {
        let mut future = std::pin::pin!(future);
        let mut context = Context::from_waker(Waker::noop());
        let deadline = Instant::now() + Duration::from_secs(15);
        loop {
            if let Poll::Ready(result) = future.as_mut().poll(&mut context) {
                return result;
            }
            assert!(Instant::now() < deadline);
            std::thread::sleep(Duration::from_millis(1));
        }
    }
    fn capture(
        device: &gpu::Device,
        queue: &gpu::Queue,
        source: &gpu::Buffer,
        size: u64,
    ) -> Result<Vec<u32>, RenderError> {
        let buffer = device.create_buffer(&gpu::BufferDescriptor {
            label: None,
            size,
            usage: gpu::BufferUsages::MAP_READ | gpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        let mut encoder = device.create_command_encoder(&Default::default());
        encoder.copy_buffer_to_buffer(source, 0, &buffer, 0, size);
        queue.submit([encoder.finish()]);
        let (tx, rx) = std::sync::mpsc::sync_channel(1);
        buffer
            .slice(..)
            .map_async(gpu::MapMode::Read, move |result| {
                let _ = tx.send(result);
            });
        let deadline = Instant::now() + Duration::from_secs(5);
        loop {
            device
                .poll(gpu::PollType::Poll)
                .map_err(|_| RenderError::DeviceUnavailable)?;
            match rx.try_recv() {
                Ok(Ok(())) => break,
                Ok(Err(_)) => return Err(RenderError::CaptureFailed),
                Err(_) => {}
            }
            if Instant::now() >= deadline {
                return Err(RenderError::CaptureTimeout);
            }
            std::thread::sleep(Duration::from_millis(1));
        }
        let mapped = buffer.slice(..).get_mapped_range();
        let values = mapped
            .chunks_exact(4)
            .map(|b| u32::from_le_bytes([b[0], b[1], b[2], b[3]]))
            .collect();
        drop(mapped);
        buffer.unmap();
        Ok(values)
    }
    #[test]
    #[ignore = "requires a real native Metal adapter; invoke explicitly"]
    fn gpu_compaction_matches_reference_and_resets_indirect_arguments() -> Result<(), RenderError> {
        let instance = gpu::Instance::new(&gpu::InstanceDescriptor {
            backends: gpu::Backends::METAL,
            ..Default::default()
        });
        let adapter = run(instance.request_adapter(&Default::default()))
            .map_err(|_| RenderError::DeviceUnavailable)?;
        let (device, queue) = run(adapter.request_device(&Default::default()))
            .map_err(|_| RenderError::DeviceUnavailable)?;
        let visibility = GpuVisibility::new(&device, 8, RenderLimits::default())?;
        let frustum = Frustum::from_matrix(IDENTITY)?;
        let spheres = [
            BoundingSphere {
                center: [0.0, 0.0, 0.5],
                radius: 0.1,
            },
            BoundingSphere {
                center: [3.0, 0.0, 0.5],
                radius: 0.1,
            },
            BoundingSphere {
                center: [0.0, 0.0, -0.5],
                radius: 0.5,
            },
            BoundingSphere {
                center: [0.0, 0.0, 1.6],
                radius: 0.1,
            },
            BoundingSphere {
                center: [-1.1, 0.0, 0.5],
                radius: 0.1,
            },
        ];
        visibility.submit(&device, &queue, &spheres, frustum, 36)?;
        let args = capture(&device, &queue, visibility.draw_arguments(), 20)?;
        let mut expected = Vec::new();
        for (i, sphere) in spheres.iter().enumerate() {
            if frustum.intersects(*sphere)? {
                expected.push(i as u32);
            }
        }
        assert_eq!(args, [36, expected.len() as u32, 0, 0, 0]);
        let mut actual = capture(&device, &queue, visibility.visible_indices(), 32)?;
        actual.truncate(args[1] as usize);
        actual.sort_unstable();
        assert_eq!(actual, expected);
        assert!(visibility
            .submit(
                &device,
                &queue,
                &[BoundingSphere {
                    center: [f32::NAN, 0.0, 0.0],
                    radius: 1.0
                }],
                frustum,
                36
            )
            .is_err());
        assert_eq!(
            capture(&device, &queue, visibility.draw_arguments(), 20)?,
            args
        );
        assert!(visibility
            .submit(&device, &queue, &[spheres[0]; 9], frustum, 36)
            .is_err());
        visibility.submit(&device, &queue, &[], frustum, 3)?;
        assert_eq!(
            capture(&device, &queue, visibility.draw_arguments(), 20)?,
            [3, 0, 0, 0, 0]
        );
        Ok(())
    }
}
