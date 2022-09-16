struct VertIn {
    @location(0) position: vec2<f32>,
    @location(1) color: vec4<f32>,
};

struct VertOut {
    @builtin(position) clip_position: vec4<f32>,
    @location(0) color: vec4<f32>,
};

@vertex
fn main_vert(in: VertIn) -> VertOut {
    var out: VertOut;
    out.clip_position = vec4<f32>(in.position, 0.0, 1.0); // FIXME: should these two values actually be 0.0 and 1.0?
    out.color = in.color;

    return out;
}

@fragment
fn main_frag(in: VertOut) -> @location(0) vec4<f32> {
    return in.color;
}
