struct Controls { matrix: mat4x4<f32>, info: vec4<u32>, viewport: vec4<f32> }
@group(0) @binding(0) var<storage,read> points: array<vec4<f32>>;
@group(0) @binding(1) var<uniform> controls: Controls;
struct Output { @builtin(position) position: vec4<f32>, @location(0) uv: vec2<f32>, @location(1) color: vec3<f32> }
@vertex fn vertex(@builtin(vertex_index) vertex_id:u32,@builtin(instance_index) instance:u32)->Output {
    let corners=array<vec2<f32>,6>(vec2(-1.0,-1.0),vec2(1.0,-1.0),vec2(-1.0,1.0),vec2(-1.0,1.0),vec2(1.0,-1.0),vec2(1.0,1.0));
    let colors=array<vec3<f32>,6>(vec3(0.95,0.78,0.35),vec3(0.24,0.79,0.79),vec3(0.50,0.64,0.98),vec3(0.92,0.41,0.58),vec3(0.58,0.85,0.50),vec3(0.76,0.50,0.92));
    let trail=instance/controls.info.x;let sample=instance%controls.info.x;
    let offset=select(sample,(controls.info.y+sample)%4096u,controls.info.x==4096u);
    var clip=controls.matrix*points[trail*4096u+offset];
    let size=select(1.5,2.8,trail==controls.info.z);
    let uv=corners[vertex_id];clip.x+=uv.x*size*2.0/controls.viewport.x*clip.w;clip.y+=uv.y*size*2.0/controls.viewport.y*clip.w;
    if controls.info.w==1u && trail!=controls.info.z {clip=vec4(0.0,0.0,2.0,1.0);}
    var output:Output;output.position=clip;output.uv=uv;output.color=colors[trail];return output;
}
@fragment fn fragment(input:Output)->@location(0)vec4<f32> {
    if dot(input.uv,input.uv)>1.0 {discard;}
    return vec4(input.color,1.0);
}
