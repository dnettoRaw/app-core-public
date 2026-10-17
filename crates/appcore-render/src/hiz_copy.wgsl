@group(0) @binding(0) var depth:texture_depth_2d;
@group(0) @binding(1) var output_mip:texture_storage_2d<r32float,write>;
@compute @workgroup_size(8,8) fn copy_depth(@builtin(global_invocation_id) id:vec3<u32>){
    if any(id.xy>=textureDimensions(output_mip)){return;}
    var value=1.0;
    if all(id.xy<textureDimensions(depth)){value=textureLoad(depth,vec2<i32>(id.xy),0);}
    textureStore(output_mip,vec2<i32>(id.xy),vec4(value,0.0,0.0,0.0));
}
