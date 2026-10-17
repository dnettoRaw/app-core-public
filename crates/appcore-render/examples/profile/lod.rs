// =============================================================================
//        #######
//     ###       ###     F: lod.rs
//    ##   ## ##   ##    P: AppCore-Runtime
//         ## ##
//                       C: 2026/10/15 08:11:39 by dnettoRaw
//    ##   ## ##   ##    U: 2026/10/15 10:24:40 by dnettoRaw
//      ###########      S: 0.1.0-alpha.1
// =============================================================================

use super::{timestamps, Result};
use appcore_render::*;
use std::{
    sync::atomic::{AtomicBool, Ordering},
    time::Instant,
};
#[path = "lod_assets.rs"]
mod assets;
pub fn run(device: &gpu::Device, queue: &gpu::Queue, failed: &AtomicBool) -> Result<()> {
    let frame = assets::frame()?;
    let clock = timestamps::Clock::new(device, queue, 2)?;
    eprintln!("LOD profile: perspective 45deg near=0.1 far=100; 2048 instances/2 batches/2 levels; finest=4096 input triangles, coarsest=2048; error=32px; 2 warmups + 7 samples; end marker included; logical availability only, no GPU eviction");
    println!("width,height,workload,sample,gpu_ns,cpu_encode_ns,batch0_fine,batch0_coarse,batch1_fine,batch1_coarse,managed_gpu_payload_bytes");
    for size in [(512, 512), (960, 540), (1920, 1080)] {
        profile_size(device, queue, failed, &clock, &frame, size)?;
    }
    Ok(())
}
fn scene(
    device: &gpu::Device,
    frame: &PreparedScene,
    size: (u32, u32),
    extra: u64,
) -> Result<GpuMeshScene> {
    let levels = |fine, coarse| {
        [
            MeshLodDescriptor {
                seed: &frame.instances()[fine],
                geometric_error: 0.,
            },
            MeshLodDescriptor {
                seed: &frame.instances()[coarse],
                geometric_error: 0.066,
            },
        ]
    };
    let a = levels(0, 2048);
    let b = levels(1024, 2049);
    Ok(GpuMeshScene::new_lod(
        device,
        &[
            LodSceneBatchDescriptor {
                levels: &a,
                capacity: 1024,
            },
            LodSceneBatchDescriptor {
                levels: &b,
                capacity: 1024,
            },
        ],
        gpu::TextureFormat::Rgba8Unorm,
        size.0,
        size.1,
        RenderLimits {
            managed_bytes: RenderLimits::default()
                .managed_bytes
                .checked_sub(extra)
                .ok_or(RenderError::BudgetExceeded)?,
            ..Default::default()
        },
    )?)
}
fn camera(size: (u32, u32)) -> ([f32; 16], LodView) {
    let focal = 1. / (std::f32::consts::PI / 8.).tan();
    let near = 0.1;
    let far = 100.;
    let matrix = [
        focal * size.1 as f32 / size.0 as f32,
        0.,
        0.,
        0.,
        0.,
        focal,
        0.,
        0.,
        0.,
        0.,
        far / (far - near),
        1.,
        0.,
        0.,
        -far * near / (far - near),
        0.,
    ];
    (
        matrix,
        LodView {
            eye: [0.; 3],
            forward: [0., 0., 1.],
            near_distance: near,
            focal_pixels: focal * size.1 as f32 * 0.5,
            viewport_half_diagonal: ((f64::from(size.0).powi(2) + f64::from(size.1).powi(2)).sqrt()
                * 0.5
                * 1.000001) as f32,
            maximum_error_pixels: 32.,
        },
    )
}
fn profile_size(
    device: &gpu::Device,
    queue: &gpu::Queue,
    failed: &AtomicBool,
    clock: &timestamps::Clock,
    frame: &PreparedScene,
    size: (u32, u32),
) -> Result<()> {
    let (width, height) = size;
    let extra = u64::from(width) * u64::from(height) * 4 + clock.payload_bytes();
    let mut scene = scene(device, frame, size, extra)?;
    let target = device
        .create_texture(&gpu::TextureDescriptor {
            label: Some("E3DN LOD profile"),
            size: gpu::Extent3d {
                width,
                height,
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
    let (matrix, view) = camera(size);
    let mut reference = None;
    for workload in ["resident", "fine_missing", "restored", "empty"] {
        let resident = workload != "fine_missing";
        for batch in 0..2 {
            scene.set_lod_resident(batch, 0, resident)?;
        }
        let groups: [&[PreparedInstance]; 2] = if workload == "empty" {
            [&[], &[]]
        } else {
            [&frame.instances()[..1024], &frame.instances()[1024..2048]]
        };
        for sample in 0..9 {
            let started = Instant::now();
            let mut encoder = device.create_command_encoder(&Default::default());
            clock.begin(&mut encoder);
            scene.encode_lod(
                device,
                queue,
                &mut encoder,
                LodSceneFrame {
                    groups: &groups,
                    matrix,
                    view,
                    target: &target,
                },
            )?;
            clock.end(&mut encoder);
            let encoded = encoder.finish();
            let cpu_ns = started.elapsed().as_nanos();
            queue.submit([encoded]);
            let measurement = clock.capture(
                device,
                queue,
                [scene.draw_arguments(0)?, scene.draw_arguments(1)?],
            )?;
            if failed.load(Ordering::Acquire) {
                return Err(RenderError::DeviceUnavailable.into());
            }
            validate_counts(workload, measurement.levels, &mut reference)?;
            if sample >= 2 {
                println!(
                    "{width},{height},{workload},{},{:.0},{cpu_ns},{},{},{},{},{}",
                    sample - 2,
                    measurement.gpu_ns,
                    measurement.levels[0][0],
                    measurement.levels[0][1],
                    measurement.levels[1][0],
                    measurement.levels[1][1],
                    scene.managed_bytes() + extra
                );
            }
        }
    }
    Ok(())
}
fn validate_counts(
    workload: &str,
    counts: [[u32; 2]; 2],
    reference: &mut Option<[[u32; 2]; 2]>,
) -> Result<()> {
    match workload {
        "empty" if counts != [[0; 2]; 2] => return Err("empty LOD arguments not cleared".into()),
        "fine_missing"
            if counts.iter().any(|b| b[0] != 0)
                || counts.iter().map(|b| b[1]).sum::<u32>() == 0 =>
        {
            return Err("missing fine LOD was drawn or scene vanished".into())
        }
        "resident" | "restored" => {
            if counts.iter().map(|b| b[0]).sum::<u32>() == 0
                || counts.iter().map(|b| b[1]).sum::<u32>() == 0
            {
                return Err("profile did not exercise both LODs".into());
            }
            if let Some(previous) = reference {
                if *previous != counts {
                    return Err("restored/resident LOD counts diverged".into());
                }
            } else {
                *reference = Some(counts);
            }
        }
        "empty" | "fine_missing" => {}
        _ => return Err("unknown LOD workload".into()),
    }
    Ok(())
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn profile_requires_residency_recovery_and_matching_perspective() {
        let mut reference = None;
        assert!(validate_counts("resident", [[512, 512]; 2], &mut reference).is_ok());
        assert!(validate_counts("restored", [[511, 512]; 2], &mut reference).is_err());
        assert!(validate_counts("fine_missing", [[1, 1023]; 2], &mut reference).is_err());
        assert!(validate_counts("empty", [[0, 1]; 2], &mut reference).is_err());
        assert!(validate_counts("unknown", [[0; 2]; 2], &mut reference).is_err());
        let levels = [
            LodLevel {
                index_count: 6,
                first_index: 0,
                base_vertex: 0,
                geometric_error: 0.,
                resident: true,
            },
            LodLevel {
                index_count: 3,
                first_index: 6,
                base_vertex: 4,
                geometric_error: 0.066,
                resident: true,
            },
        ];
        for size in [(512, 512), (960, 540), (1920, 1080)] {
            let (matrix, view) = camera(size);
            let frustum = Frustum::from_matrix(matrix).unwrap();
            for (z, level) in [(2., 0), (12., 1)] {
                let object = LodObject {
                    bounds: BoundingSphere {
                        center: [0., 0., z],
                        radius: 0.033,
                    },
                    max_scale: 1.,
                };
                assert_eq!(
                    choose_lod(&levels, object, view, frustum)
                        .unwrap()
                        .unwrap()
                        .level,
                    level
                );
            }
            assert_eq!(matrix[5] * size.1 as f32 * 0.5, view.focal_pixels);
        }
        let (matrix, view) = camera((1920, 1080));
        view.validate().unwrap();
        for z in [0.1, 2., 12., 100.] {
            let depth = (matrix[10] * z + matrix[14]) / z;
            assert!((-0.000001..=1.000001).contains(&depth));
        }
    }
}
