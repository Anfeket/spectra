struct VertexIn {
    @location(0) corner: vec2<f32>,
    @location(1) inst_x: f32,
    @location(2) inst_width: f32,
    @location(3) inst_height: f32,
    @location(4) inst_color: vec3<f32>,
}

struct VertexOut {
    @builtin(position) clip_pos: vec4<f32>,
    @location(0) local_uv: vec2<f32>,
    @location(1) color: vec3<f32>,
}

@vertex
fn vs_main(in: VertexIn) -> VertexOut {
    var out: VertexOut;
    let x = in.inst_x + in.corner.x * in.inst_width;
    let y = in.corner.y * in.inst_height;
    let ndc_x = x * 2.0 - 1.0;
    let ndc_y = y * 2.0 - 1.0;
    out.clip_pos = vec4<f32>(ndc_x, ndc_y, 0.0, 1.0);
    out.local_uv = in.corner;
    out.color = in.inst_color;
    return out;
}

@fragment
fn fs_main(in: VertexOut) -> @location(0) vec4<f32> {
    let d = min(in.local_uv.x, min(1.0 - in.local_uv.x,
              min(in.local_uv.y, 1.0 - in.local_uv.y)));
    let aa = fwidth(d) * 1.5;
    let coverage = smoothstep(0.0, aa, d);
    return vec4<f32>(in.color, coverage);
}
