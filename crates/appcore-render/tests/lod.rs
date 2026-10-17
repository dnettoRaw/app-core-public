// =============================================================================
//        #######
//     ###       ###     F: lod.rs
//    ##   ## ##   ##    P: AppCore-Runtime
//         ## ##
//                       C: 2026/10/15 08:11:39 by dnettoRaw
//    ##   ## ##   ##    U: 2026/10/15 10:24:40 by dnettoRaw
//      ###########      S: 0.1.0-alpha.1
// =============================================================================

use appcore_render::*;
fn levels() -> [LodLevel; 3] {
    [
        LodLevel {
            index_count: 36,
            first_index: 0,
            base_vertex: 0,
            geometric_error: 0.,
            resident: true,
        },
        LodLevel {
            index_count: 18,
            first_index: 36,
            base_vertex: 8,
            geometric_error: 0.05,
            resident: true,
        },
        LodLevel {
            index_count: 6,
            first_index: 54,
            base_vertex: 16,
            geometric_error: 0.25,
            resident: true,
        },
    ]
}
fn view() -> LodView {
    LodView {
        eye: [0.; 3],
        forward: [0., 0., -1.],
        near_distance: 0.1,
        focal_pixels: 100.,
        viewport_half_diagonal: 150.,
        maximum_error_pixels: 2.,
    }
}
fn frustum() -> Result<Frustum, RenderError> {
    let (near, far) = (0.1, 100.);
    Frustum::from_matrix([
        1.,
        0.,
        0.,
        0.,
        0.,
        1.,
        0.,
        0.,
        0.,
        0.,
        far / (near - far),
        -1.,
        0.,
        0.,
        near * far / (near - far),
        0.,
    ])
}
fn objects() -> Vec<LodObject> {
    [
        [0., 0., -0.12],
        [0., 0., -10.],
        [0., 0., -50.],
        [0., 0., -10.],
        [500., 0., -10.],
        [9.98, 0., -10.],
        [0., 0., -99.95],
    ]
    .into_iter()
    .enumerate()
    .map(|(i, center)| LodObject {
        bounds: BoundingSphere {
            center,
            radius: 0.1,
        },
        max_scale: if i == 3 { 2. } else { 1. },
    })
    .collect()
}
#[test]
fn reference_respects_projection_scale_clipping_and_residency() -> Result<(), RenderError> {
    let mut levels = levels();
    let choices = objects()
        .into_iter()
        .map(|o| choose_lod(&levels, o, view(), frustum()?))
        .collect::<Result<Vec<_>, _>>()?;
    assert_eq!(
        choices
            .iter()
            .map(|c| c.map(|c| c.level))
            .collect::<Vec<_>>(),
        vec![Some(0), Some(1), Some(2), Some(0), None, Some(0), Some(0)]
    );
    let off_axis = LodObject {
        bounds: BoundingSphere {
            center: [8., 0., -10.],
            radius: 0.1,
        },
        max_scale: 1.,
    };
    assert_eq!(
        choose_lod(&levels, off_axis, view(), frustum()?)?.map(|c| c.level),
        Some(1)
    );
    levels[1].resident = false;
    assert_eq!(
        choose_lod(&levels, objects()[1], view(), frustum()?)?,
        Some(LodChoice {
            level: 2,
            degraded: true
        })
    );
    Ok(())
}
#[test]
fn invalid_metadata_and_views_fail_before_selection() -> Result<(), RenderError> {
    let object = objects()[1];
    let base = levels();
    let test = |levels: &[LodLevel], view| choose_lod(levels, object, view, frustum()?);
    assert_eq!(test(&[], view()), Err(RenderError::InvalidLimits));
    assert!(test(&[base[0]; 9], view()).is_err());
    for index in 0..3 {
        let mut bad = base;
        bad[index].geometric_error = f32::NAN;
        assert!(test(&bad, view()).is_err());
    }
    let mut bad = base;
    bad[2].resident = false;
    assert!(test(&bad, view()).is_err());
    bad = base;
    bad[1].first_index = 0;
    assert!(test(&bad, view()).is_err());
    bad = base;
    bad[0].index_count = 1;
    assert!(test(&bad, view()).is_err());
    bad = base;
    bad[0].geometric_error = 0.1;
    assert!(test(&bad, view()).is_err());
    bad = base;
    bad[2].geometric_error = 0.01;
    assert!(test(&bad, view()).is_err());
    for invalid in [
        LodView {
            forward: [0.; 3],
            ..view()
        },
        LodView {
            focal_pixels: f32::INFINITY,
            ..view()
        },
        LodView {
            viewport_half_diagonal: f32::NAN,
            ..view()
        },
    ] {
        assert!(test(&base, invalid).is_err());
    }
    assert!(choose_lod(
        &base,
        LodObject {
            max_scale: 0.,
            ..object
        },
        view(),
        frustum()?
    )
    .is_err());
    Ok(())
}
#[cfg(feature = "metal")]
mod metal {
    use super::*;
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
    #[test]
    #[ignore = "requires real native Metal; invoke explicitly"]
    fn gpu_lod_compaction_matches_reference_and_recovers_residency(
    ) -> Result<(), Box<dyn std::error::Error>> {
        let instance = gpu::Instance::new(&gpu::InstanceDescriptor {
            backends: gpu::Backends::METAL,
            ..Default::default()
        });
        let adapter = run(instance.request_adapter(&Default::default()))?;
        let (device, queue) = run(adapter.request_device(&Default::default()))?;
        let mut levels = levels();
        admission(&device, &levels);
        let mut selection = GpuLodVisibility::new(&device, 65, &levels, RenderLimits::default())?;
        assert_eq!(selection.visible_binding(1)?.offset, 512);
        assert!(selection.visible_binding(3).is_err());
        assert_eq!(
            selection.set_resident(2, false),
            Err(RenderError::InvalidResource)
        );
        let mask = device.create_buffer(&gpu::BufferDescriptor {
            label: None,
            size: 7 * 4,
            usage: gpu::BufferUsages::STORAGE | gpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        for (resident, masked) in [(true, false), (false, false), (false, true), (true, false)] {
            selection.set_resident(1, resident)?;
            levels[1].resident = resident;
            let flags: Vec<_> = (0..7)
                .flat_map(|i| u32::from(!masked || i != 1).to_le_bytes())
                .collect();
            queue.write_buffer(&mask, 0, &flags);
            let data = objects();
            let mut encoder = device.create_command_encoder(&Default::default());
            selection.encode(
                &device,
                &queue,
                &mut encoder,
                LodVisibilityInput {
                    objects: &data,
                    frustum: frustum()?,
                    view: view(),
                    occlusion: masked.then_some(&mask),
                },
            )?;
            queue.submit([encoder.finish()]);
            let choices = read_words(&device, &queue, selection.choices(), 0, 7)?;
            let mut counts = [0u32; 3];
            for (i, object) in data.into_iter().enumerate() {
                let expected = if masked && i == 1 {
                    None
                } else {
                    choose_lod(&levels, object, view(), frustum()?)?
                };
                let encoded =
                    expected.map_or(u32::MAX, |c| c.level | if c.degraded { 1 << 31 } else { 0 });
                assert_eq!(choices[i], encoded, "object {i}");
                if let Some(choice) = expected {
                    counts[choice.level as usize] += 1;
                }
            }
            let arguments = read_words(&device, &queue, selection.draw_arguments(), 0, 15)?;
            for (level, count) in counts.into_iter().enumerate() {
                assert_eq!(
                    &arguments[level * 5..level * 5 + 5],
                    &[
                        levels[level].index_count,
                        count,
                        levels[level].first_index,
                        levels[level].base_vertex,
                        0
                    ]
                );
                let partition = selection.visible_binding(level)?;
                let indices = read_words(
                    &device,
                    &queue,
                    partition.buffer,
                    partition.offset,
                    count.max(1) as usize,
                )?;
                let mut got = indices[..count as usize].to_vec();
                got.sort_unstable();
                let expected: Vec<_> = choices
                    .iter()
                    .enumerate()
                    .filter(|(_, choice)| {
                        **choice != u32::MAX && (**choice & 0x7fffffff) == level as u32
                    })
                    .map(|(i, _)| i as u32)
                    .collect();
                assert_eq!(got, expected);
            }
        }
        reject_and_clear(&device, &queue, &selection)?;
        Ok(())
    }
    fn admission(device: &gpu::Device, levels: &[LodLevel]) {
        assert_eq!(GpuLodVisibility::buffer_bytes(65, 3), Ok(4196));
        assert!(matches!(
            GpuLodVisibility::new(
                device,
                65,
                levels,
                RenderLimits {
                    managed_bytes: 4195,
                    ..Default::default()
                }
            ),
            Err(RenderError::BudgetExceeded)
        ));
        assert!(matches!(
            GpuLodVisibility::new(device, 0, levels, RenderLimits::default()),
            Err(RenderError::InvalidLimits)
        ));
    }
    fn reject_and_clear(
        device: &gpu::Device,
        queue: &gpu::Queue,
        selection: &GpuLodVisibility,
    ) -> Result<(), RenderError> {
        let before = read_words(device, queue, selection.draw_arguments(), 0, 15)?;
        let mut invalid = objects();
        invalid[0].max_scale = f32::NAN;
        let mut encoder = device.create_command_encoder(&Default::default());
        assert_eq!(
            selection.encode(
                device,
                queue,
                &mut encoder,
                LodVisibilityInput {
                    objects: &invalid,
                    frustum: frustum()?,
                    view: view(),
                    occlusion: None
                }
            ),
            Err(RenderError::InvalidLimits)
        );
        assert_eq!(
            read_words(device, queue, selection.draw_arguments(), 0, 15)?,
            before
        );
        selection.encode(
            device,
            queue,
            &mut encoder,
            LodVisibilityInput {
                objects: &[],
                frustum: frustum()?,
                view: view(),
                occlusion: None,
            },
        )?;
        queue.submit([encoder.finish()]);
        let after = read_words(device, queue, selection.draw_arguments(), 0, 15)?;
        assert_eq!([after[1], after[6], after[11]], [0; 3]);
        Ok(())
    }
    fn read_words(
        device: &gpu::Device,
        queue: &gpu::Queue,
        source: &gpu::Buffer,
        offset: u64,
        count: usize,
    ) -> Result<Vec<u32>, RenderError> {
        let size = count as u64 * 4;
        let buffer = device.create_buffer(&gpu::BufferDescriptor {
            label: None,
            size,
            usage: gpu::BufferUsages::MAP_READ | gpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        let mut encoder = device.create_command_encoder(&Default::default());
        encoder.copy_buffer_to_buffer(source, offset, &buffer, 0, size);
        queue.submit([encoder.finish()]);
        let (tx, rx) = std::sync::mpsc::sync_channel(1);
        buffer.slice(..).map_async(gpu::MapMode::Read, move |r| {
            let _ = tx.send(r);
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
        let words = mapped
            .chunks_exact(4)
            .map(|b| u32::from_le_bytes([b[0], b[1], b[2], b[3]]))
            .collect();
        drop(mapped);
        buffer.unmap();
        Ok(words)
    }
}
