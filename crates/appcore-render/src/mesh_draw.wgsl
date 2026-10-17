struct Instance {position:vec4<f32>,rotation:vec4<f32>,scale:vec4<f32>,color:vec4<f32>}
@group(0) @binding(0) var<storage,read> instances:array<Instance>;
@group(0) @binding(1) var<storage,read> visible:array<u32>;
@group(0) @binding(2) var<uniform> camera:mat4x4<f32>;
struct Output {@builtin(position) position:vec4<f32>,@location(0) color:vec4<f32>}
fn rotate(q:vec4<f32>,v:vec3<f32>)->vec3<f32>{return v+2.0*(q.w*cross(q.xyz,v)+cross(q.xyz,cross(q.xyz,v)));}
@vertex fn vertex(@location(0) position:vec3<f32>,@location(1) normal:vec3<f32>,@builtin(instance_index) instance:u32)->Output{
    let item=instances[visible[instance]];
    let world=rotate(item.rotation,position*item.scale.xyz)+item.position.xyz;
    let n=normalize(rotate(item.rotation,normal/item.scale.xyz));
    let lighting=0.3+0.7*max(dot(n,normalize(vec3(-0.5,0.8,0.4))),0.0);
    var output:Output;output.position=camera*vec4(world,1.0);output.color=vec4(item.color.xyz*lighting,item.color.w);return output;
}
@fragment fn fragment(input:Output)->@location(0)vec4<f32>{return input.color;}
