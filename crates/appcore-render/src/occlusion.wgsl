struct Controls {matrix:mat4x4<f32>,info:vec4<u32>}
@group(0) @binding(0) var<storage,read> spheres:array<vec4<f32>>;
@group(0) @binding(1) var<storage,read_write> flags:array<u32>;
@group(0) @binding(2) var pyramid:texture_2d<f32>;
@group(0) @binding(3) var<uniform> controls:Controls;
@compute @workgroup_size(64) fn query(@builtin(global_invocation_id) id:vec3<u32>){
    if id.x>=controls.info.x{return;}
    flags[id.x]=1u;
    let sphere=spheres[id.x];var low=vec2(1e30);var high=vec2(-1e30);var nearest=1e30;
    for(var i=0u;i<8u;i++){
        let signs=vec3(select(-1.0,1.0,(i&1u)!=0u),select(-1.0,1.0,(i&2u)!=0u),select(-1.0,1.0,(i&4u)!=0u));
        let clip=controls.matrix*vec4(sphere.xyz+signs*sphere.w,1.0);
        if clip.w<=0.000001 || clip.z<=0.0{return;}
        let ndc=clip.xyz/clip.w;
        if any(abs(ndc.xy)>vec2(1.0)){return;}
        let pixel=vec2((ndc.x*0.5+0.5)*f32(controls.info.y),(0.5-ndc.y*0.5)*f32(controls.info.z));
        low=min(low,pixel);high=max(high,pixel);nearest=min(nearest,ndc.z);
    }
    let limit=controls.info.yz-vec2(1u);
    let first=min(vec2<u32>(max(floor(low)-vec2(1.0),vec2(0.0))),limit);
    let last=min(vec2<u32>(ceil(high)+vec2(1.0)),limit);
    var level=0u;
    loop{
        if all((last>>vec2(level))-(first>>vec2(level))<=vec2(1u)) || level+1u>=controls.info.w{break;}
        level++;
    }
    let a=vec2<i32>(first>>vec2(level));let b=vec2<i32>(last>>vec2(level));
    let depth=max(max(textureLoad(pyramid,a,i32(level)).x,textureLoad(pyramid,vec2(b.x,a.y),i32(level)).x),max(textureLoad(pyramid,vec2(a.x,b.y),i32(level)).x,textureLoad(pyramid,b,i32(level)).x));
    if depth<1.0 && nearest>depth+0.0001{flags[id.x]=0u;}
}
