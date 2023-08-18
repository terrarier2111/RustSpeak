struct VertIn {
    @location(0) position: vec2<f32>,
    @location(1) color: vec4<f32>,
    @location(2) radius: f32,
    @location(3) border_thickness: f32,
};

struct VertOut {
    @builtin(position) clip_position: vec4<f32>,
    @location(0) color: vec4<f32>,
    @location(1) radius: f32,
    @location(2) border_thickness: f32,
};

@vertex
fn main_vert(in: VertIn) -> VertOut {
    var out: VertOut;
    out.clip_position = vec4<f32>(in.position, 0.0, 1.0); // FIXME: should these two values actually be 0.0 and 1.0?
    out.color = in.color;
    out.radius = in.radius;
    out.border_thickness = in.border_thickness;

    return out;
}

@fragment
fn main_frag(in: VertOut) -> @location(0) vec4<f32> {
     var uv: vec2<f32> = in.clip_position.xy;
     var d: f32 = sqrt(dot(uv, uv));
     var t: f32 = 1.0 - smoothstep(in.radius-in.border_thickness, in.radius, d);

     return vec4(in.color.rgb, in.color.a*t);
}