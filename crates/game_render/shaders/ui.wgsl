struct UiViewport {
    size: vec2<f32>,
    _padding: vec2<f32>,
};

@group(0) @binding(0)
var<uniform> viewport: UiViewport;

struct VertexInput {
    @builtin(vertex_index) vertex_index: u32,
    @location(0) pixel_center: vec2<f32>,
    @location(1) pixel_size: vec2<f32>,
    @location(2) color: vec4<f32>,
};

struct VertexOutput {
    @builtin(position) clip_position: vec4<f32>,
    @location(0) color: vec4<f32>,
};

@vertex
fn vs_main(input: VertexInput) -> VertexOutput {
    let corners = array<vec2<f32>, 6>(
        vec2<f32>(-0.5, -0.5),
        vec2<f32>( 0.5, -0.5),
        vec2<f32>( 0.5,  0.5),
        vec2<f32>(-0.5, -0.5),
        vec2<f32>( 0.5,  0.5),
        vec2<f32>(-0.5,  0.5),
    );
    let pixel_position =
        input.pixel_center + corners[input.vertex_index] * input.pixel_size;
    let normalized = vec2<f32>(
        pixel_position.x / viewport.size.x * 2.0 - 1.0,
        1.0 - pixel_position.y / viewport.size.y * 2.0,
    );

    var output: VertexOutput;
    output.clip_position = vec4<f32>(normalized, 0.0, 1.0);
    output.color = input.color;
    return output;
}

@fragment
fn fs_main(input: VertexOutput) -> @location(0) vec4<f32> {
    return input.color;
}
