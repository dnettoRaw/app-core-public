struct VertexOutput {
    @builtin(position) position: vec4<f32>,
    @location(0) color: vec3<f32>,
}
@vertex fn vertex(@builtin(vertex_index) index: u32, @builtin(instance_index) instance: u32) -> VertexOutput {
    let positions = array<vec2<f32>, 3>(vec2(-0.8, -0.8), vec2(0.8, -0.8), vec2(0.0, 0.8));
    var output: VertexOutput;
    output.position = vec4(positions[index], 0.3 + f32(instance) * 0.4, 1.0);
    output.color = select(vec3(1.0, 0.25, 0.0), vec3(0.0, 0.0, 1.0), instance == 1u);
    return output;
}
@fragment fn fragment(input: VertexOutput) -> @location(0) vec4<f32> { return vec4(input.color, 1.0); }
