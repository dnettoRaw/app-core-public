// =============================================================================
//        #######
//     ###       ###     F: lib.rs
//    ##   ## ##   ##    P: AppCore-Runtime
//         ## ##
//                       C: 2026/10/14 16:20:57 by dnettoRaw
//    ##   ## ##   ##    U: 2026/10/15 10:24:40 by dnettoRaw
//      ###########      S: 0.1.0-alpha.1
// =============================================================================

#![doc = include_str!("../README.en.md")]
#![forbid(unsafe_code)]

mod limits;
mod resources;

pub use limits::{RenderError, RenderLimits};
pub use resources::{ResourceArena, ResourceId};

#[cfg(feature = "metal")]
mod offscreen;
#[cfg(feature = "metal")]
pub use offscreen::{OffscreenRenderer, RenderCapabilities};

#[cfg(feature = "metal")]
mod presentation;
#[cfg(feature = "metal")]
pub use presentation::{NativeRenderer, PresentationState};
/// Explicit low-level GPU interop for deployment-owned native surface adapters.
#[cfg(feature = "metal")]
pub use wgpu as gpu;

#[cfg(feature = "metal")]
mod frame_admission;

#[cfg(feature = "metal")]
mod lorenz;
#[cfg(feature = "metal")]
pub use lorenz::LorenzCompute;

mod camera;
pub use camera::LorenzView;
#[cfg(feature = "metal")]
mod lorenz_scene;

mod scene;
mod scene_components;
mod scene_storage;
pub use scene::{SceneLimits, SceneWorld};
pub use scene_components::{RenderBinding, Transform, Velocity};
pub use scene_storage::{EntityId, MovingColumns, SceneInstance, SceneSnapshot};

mod scene_commands;
pub use scene_commands::{SceneCommand, SceneCommands};

mod asset_data;
mod asset_frame;
mod assets;
pub use asset_data::{BoundingSphere, MeshAsset, MeshVertex};
pub use asset_frame::{PreparedInstance, PreparedScene};
pub use assets::{AssetLimits, AssetRegistry};

mod frustum;
pub use frustum::Frustum;
#[cfg(feature = "metal")]
mod visibility;
#[cfg(feature = "metal")]
pub use visibility::{GpuVisibility, VisibilityInput};

#[cfg(feature = "metal")]
mod mesh_batch;
#[cfg(feature = "metal")]
mod mesh_pipeline;
#[cfg(feature = "metal")]
pub use mesh_batch::GpuMeshBatch;

#[cfg(feature = "metal")]
mod hiz;
#[cfg(feature = "metal")]
pub use hiz::HiZPyramid;

#[cfg(feature = "metal")]
mod occlusion;
#[cfg(feature = "metal")]
pub use occlusion::{GpuOcclusion, OcclusionInput};

#[cfg(feature = "metal")]
mod mesh_scene;
#[cfg(feature = "metal")]
pub use mesh_scene::{GpuMeshScene, LodSceneBatchDescriptor, LodSceneFrame, MeshBatchDescriptor};

#[cfg(feature = "metal")]
mod native_mesh;

mod lorenz_parameters;
pub use lorenz_parameters::LorenzParameters;

#[cfg(feature = "metal")]
mod frame_progress;
#[cfg(feature = "metal")]
pub use frame_progress::NativeFrameProgress;

mod lod;
pub use lod::{choose_lod, LodChoice, LodLevel, LodObject, LodView};

#[cfg(feature = "metal")]
mod lod_visibility;
#[cfg(feature = "metal")]
pub use lod_visibility::{GpuLodVisibility, LodVisibilityInput};

#[cfg(feature = "metal")]
mod lod_geometry;
#[cfg(feature = "metal")]
mod lod_mesh_batch;
#[cfg(feature = "metal")]
pub use lod_geometry::MeshLodDescriptor;
#[cfg(feature = "metal")]
pub use lod_mesh_batch::{GpuLodMeshBatch, LodMeshFrame};

#[cfg(feature = "metal")]
mod scene_mesh;
