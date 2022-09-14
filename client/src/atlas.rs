use std::collections::HashMap;
use std::sync::{Arc, Mutex, RwLock};
use guillotiere::{Allocation, AllocId, AtlasAllocator, size2};
use wgpu::{Sampler, SamplerDescriptor, TextureAspect, TextureDescriptor, TextureDimension, TextureFormat, TextureUsages, TextureViewDescriptor, TextureViewDimension};
use wgpu_biolerless::{RawTextureBuilder, State};
use crate::render::TexTriple;

#[derive(Copy, Clone)]
pub struct UV(pub u32, pub u32);

pub struct Atlas {
    alloc: Mutex<AtlasAllocator>,
    alloc_map: RwLock<HashMap<String, AtlasAlloc>>,
    gpu_buffer: TexTriple,
}

impl Atlas {

    pub fn new(state: &State, size: (u32, u32), texture_format: TextureFormat) -> Self { // FIXME: can we infer the texture_format from the state?
        if size.0 >= (1 << 31) || size.1 >= (1 << 31) {
            panic!("The size passed was too big!");
        }
        let tex = state.create_raw_texture(RawTextureBuilder::new()
            .usages(TextureUsages::TEXTURE_BINDING | TextureUsages::COPY_DST).dimensions(size.clone())
            .texture_dimension(TextureDimension::D2).format(texture_format));
        let view = tex.create_view(&TextureViewDescriptor {
            label: None,
            format: Some(texture_format),
            dimension: Some(TextureViewDimension::D2),
            aspect: TextureAspect::All,
            base_mip_level: 0,
            mip_level_count: None,
            base_array_layer: 0,
            array_layer_count: None,
        });
        let sampler = state.device().create_sampler(&SamplerDescriptor { // FIXME: adjust values!
            label: None,
            address_mode_u: Default::default(),
            address_mode_v: Default::default(),
            address_mode_w: Default::default(),
            mag_filter: Default::default(),
            min_filter: Default::default(),
            mipmap_filter: Default::default(),
            lod_min_clamp: 0.0,
            lod_max_clamp: 0.0,
            compare: None,
            anisotropy_clamp: None,
            border_color: None
        });
        Self {
            alloc: Mutex::new(AtlasAllocator::new(size2(size.0 as i32, size.1 as i32))),
            alloc_map: Default::default(),
            gpu_buffer: TexTriple {
                tex,
                view,
                sampler,
            }
        }
    }

    pub fn alloc(&mut self, size: (i32, i32)) -> Allocation {
        let mut alloc = self.alloc.lock().unwrap();
        loop {
            if let Some(alloc) = alloc.allocate(size2(size.0, size.1)) {
                // self.alloc_map.write().unwrap().insert();
                // FIXME: do insertion into map and take path as parameter
                return alloc;
            }
            // grow the allocation inside the allocator until we have enough free space
            alloc.grow(alloc.size() * 2);
        }
    }

    pub fn dealloc(&mut self, path: String) -> bool {
        self.alloc_map.write().unwrap().remove(path.as_str()).is_some()
    }

}

pub struct AtlasAlloc {
    allocation: Allocation,
    atlas: Arc<Atlas>,
    pub uv: UV,
}

impl Drop for AtlasAlloc {
    fn drop(&mut self) {
        self.atlas.alloc.lock().unwrap().deallocate(self.allocation.id);
    }
}
