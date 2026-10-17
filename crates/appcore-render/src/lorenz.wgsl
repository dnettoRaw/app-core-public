struct Parameters { cursor: u32, steps: u32, dt: f32, padding: u32, coefficients: vec4<f32> }
@group(0) @binding(0) var<storage, read> input_heads: array<vec4<f32>>;
@group(0) @binding(1) var<storage, read_write> output_heads: array<vec4<f32>>;
@group(0) @binding(2) var<storage, read_write> trails: array<vec4<f32>>;
@group(0) @binding(3) var<uniform> parameters: Parameters;
fn derivative(p: vec3<f32>) -> vec3<f32> {
    return vec3(parameters.coefficients.x * (p.y-p.x), p.x*(parameters.coefficients.y-p.z)-p.y, p.x*p.y-parameters.coefficients.z*p.z);
}
fn rk4(p: vec3<f32>, dt: f32) -> vec3<f32> {
    let a=derivative(p); let b=derivative(p+a*dt*0.5);
    let c=derivative(p+b*dt*0.5); let d=derivative(p+c*dt);
    return p+dt*(a+2.0*b+2.0*c+d)/6.0;
}
@compute @workgroup_size(64) fn integrate(@builtin(global_invocation_id) invocation: vec3<u32>) {
    let trail=invocation.x;
    if trail>=6u { return; }
    var head=input_heads[trail].xyz;
    for(var step=0u; step<parameters.steps; step++) {
        head=rk4(head,parameters.dt);
        let index=trail*4096u+(parameters.cursor+step)%4096u;
        trails[index]=vec4(head.x*0.1,(head.z-25.0)*0.1,head.y*0.1,1.0);
    }
    output_heads[trail]=vec4(head,1.0);
}
