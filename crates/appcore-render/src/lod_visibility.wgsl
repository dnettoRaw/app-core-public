struct Object {sphere:vec4<f32>,scale:vec4<f32>}
struct Level {geometry:vec4<u32>,error:vec4<f32>}
struct Draw {index_count:u32,instance_count:atomic<u32>,first_index:u32,base_vertex:i32,first_instance:u32}
struct Controls {planes:array<vec4<f32>,6>,eye:vec4<f32>,forward:vec4<f32>,projection:vec4<f32>,info:vec4<u32>}
@group(0) @binding(0) var<storage,read> objects:array<Object>;
@group(0) @binding(1) var<storage,read_write> visible:array<u32>;
@group(0) @binding(2) var<storage,read_write> draws:array<Draw>;
@group(0) @binding(3) var<storage,read> levels:array<Level>;
@group(0) @binding(4) var<uniform> controls:Controls;
@group(0) @binding(5) var<storage,read> mask:array<u32>;
@group(0) @binding(6) var<storage,read_write> choices:array<u32>;
@compute @workgroup_size(64) fn select_lod(@builtin(global_invocation_id) invocation:vec3<u32>){
    let index=invocation.x; if index>=controls.info.x{return;}
    choices[index]=0xffffffffu;
    if controls.info.w==1u && mask[index]==0u{return;}
    let object=objects[index]; let sphere=object.sphere;
    var clipped=false;
    for(var i=0u;i<6u;i++){
        let plane=controls.planes[i];
        let distance=dot(plane.xyz,sphere.xyz)+plane.w;
        let margin=(dot(abs(plane.xyz),abs(sphere.xyz))+abs(plane.w)+sphere.w+1.0)*0.00001;
        if distance < -sphere.w-margin{return;}
        if distance <= sphere.w+margin{clipped=true;}
    }
    let delta=sphere.xyz-controls.eye.xyz;
    let magnitude=dot(abs(delta),abs(controls.forward.xyz))+sphere.w+1.0;
    let depth=dot(delta,controls.forward.xyz)-sphere.w-magnitude*0.00001;
    var desired=0u;
    if !clipped && depth>controls.eye.w {
        for(var level=1u;level<controls.info.y && level<8u;level++){
            let error=levels[level].error.x*object.scale.x;
            if depth-error>controls.eye.w {
                let projected=(controls.projection.x+controls.projection.y)*error/(depth-error);
                if projected*1.00001<=controls.projection.z{desired=level;}
            }
        }
    }
    var selected=0xffffffffu;
    for(var candidate=desired;candidate<controls.info.y && candidate<8u;candidate++) {
        if levels[candidate].geometry.w!=0u {selected=candidate;break;}
    }
    if selected==0xffffffffu{return;}
    var choice=selected;
    if selected!=desired {choice=choice|0x80000000u;}
    choices[index]=choice;
    let offset=atomicAdd(&draws[selected].instance_count,1u);
    visible[selected*controls.info.z+offset]=index;
}
