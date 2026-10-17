@group(0) @binding(0) var input_mip:texture_2d<f32>;
@group(0) @binding(1) var output_mip:texture_storage_2d<r32float,write>;
@compute @workgroup_size(8,8) fn reduce(@builtin(global_invocation_id) id:vec3<u32>){
    if any(id.xy>=textureDimensions(output_mip)){return;}
    let limit=vec2<i32>(textureDimensions(input_mip))-vec2(1);
    let p=vec2<i32>(id.xy)*2;
    let a=textureLoad(input_mip,min(p,limit),0).x;
    let b=textureLoad(input_mip,min(p+vec2(1,0),limit),0).x;
    let c=textureLoad(input_mip,min(p+vec2(0,1),limit),0).x;
    let d=textureLoad(input_mip,min(p+vec2(1,1),limit),0).x;
    textureStore(output_mip,vec2<i32>(id.xy),vec4(max(max(a,b),max(c,d)),0.0,0.0,0.0));
}
