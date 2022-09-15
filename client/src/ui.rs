use wgpu::{Sampler, Texture, TextureView};
use crate::atlas::UV;
use crate::render::TexTriple;

pub struct Button {
    pub pos: (f64, f64),
    pub coloring: Coloring,
}

pub struct Color {
    pub r: f32,
    pub g: f32,
    pub b: f32,
    pub a: f32,
}

pub struct Tex {
    pub alpha: f32,
    pub ty: TexTy,
}

pub enum Coloring {
    Color(Color),
    Tex(Tex),
}


