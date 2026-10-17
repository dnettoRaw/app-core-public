struct Controls {planes:array<vec4<f32>,6>,info:vec4<u32>}
struct Draw {index_count:u32,instance_count:atomic<u32>,first_index:u32,base_vertex:i32,first_instance:u32}
@group(0) @binding(0) var<storage,read> spheres:array<vec4<f32>>;
@group(0) @binding(1) var<storage,read_write> visible:array<u32>;
@group(0) @binding(2) var<storage,read_write> arguments:Draw;
@group(0) @binding(3) var<uniform> controls:Controls;
@group(0) @binding(4) var<storage,read> occlusion:array<u32>;
@compute @workgroup_size(64) fn cull(@builtin(global_invocation_id) invocation:vec3<u32>){
    let index=invocation.x;if index>=controls.info.x{return;}
    if controls.info.y==1u && occlusion[index]==0u{return;}
    let sphere=spheres[index];
    for(var i=0u;i<6u;i++){
        let plane=controls.planes[i];
        let distance=dot(plane.xyz,sphere.xyz)+plane.w;
        let margin=(dot(abs(plane.xyz),abs(sphere.xyz))+abs(plane.w)+sphere.w+1.0)*0.00001;
        if distance < -sphere.w-margin{return;}
    }
    let offset=atomicAdd(&arguments.instance_count,1u);
    visible[offset]=index;
}
