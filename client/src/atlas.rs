use std::collections::HashMap;
use std::ops::Deref;
use std::sync::{Arc, Mutex, RwLock};
use std::sync::atomic::{AtomicU64, Ordering};
use arc_swap::ArcSwap;
use guillotiere::{Allocation, AllocId, AtlasAllocator, size2};
use wgpu::{CommandEncoder, Extent3d, FilterMode, ImageCopyTexture, ImageDataLayout, Origin3d, Sampler, SamplerDescriptor, Texture, TextureAspect, TextureDescriptor, TextureDimension, TextureFormat, TextureUsages, TextureViewDescriptor, TextureViewDimension};
use wgpu_biolerless::{RawTextureBuilder, State};
use crate::render::TexTriple;

#[derive(Copy, Clone)]
pub struct UV(pub u32, pub u32);

pub struct Atlas {
    alloc: Mutex<AtlasAllocator>,
    alloc_map: RwLock<HashMap<String, Arc<AtlasAlloc>>>,
    gpu_buffer: ArcSwap<TexTriple>,
    buffer_size: AtomicU64,
    state: Arc<State>,
    write_queue: Mutex<Vec<QueuedWrite>>,
    texture_format: TextureFormat,
}

impl Atlas {

    pub fn new(state: Arc<State>, size: (u32, u32), texture_format: TextureFormat) -> Self { // FIXME: can we infer the texture_format from the state?
        if size.0 >= (1 << 31) || size.1 >= (1 << 31) {
            panic!("The size passed was too big!");
        }
        let tex = Self::create_tex(&state, size.clone(), texture_format);
        Self {
            alloc: Mutex::new(AtlasAllocator::new(size2(size.0 as i32, size.1 as i32))),
            alloc_map: Default::default(),
            gpu_buffer: ArcSwap::new(Arc::new(tex)),
            buffer_size: ,
            state,
            write_queue: Mutex::new(vec![]),
            texture_format,
        }
    }

    pub fn alloc(self: &Arc<Self>, path: String, size: (u32, u32), content: impl AsRef<[u8]>) -> Arc<AtlasAlloc> {
        if size.0 >= (1 << 31) || size.1 >= (1 << 31) {
            panic!("The size passed was too big!");
        }
        let mut alloc = self.alloc.lock().unwrap();
        let mut realloc = false;
        loop {
            if let Some(alloc) = alloc.allocate(size2(size.0 as i32, size.1 as i32)) {
                let alloc = Arc::new(AtlasAlloc {
                    allocation: alloc,
                    atlas: self.clone(),
                });
                self.alloc_map.write().unwrap().insert(path, alloc.clone());
                // if we don't have to reallocate, just perform the texture write
                // else enqueue the write to happen once the atlas gets updated
                if realloc {
                    self.write_queue.lock().unwrap().push(QueuedWrite {
                        data: Arc::new(Box::new(content)),
                        pos: (alloc.allocation.rectangle.min.x as u32, alloc.allocation.rectangle.min.y as u32),
                        size,
                    });
                } else {
                    self.write_tex(&self.gpu_buffer.load().tex, (alloc.allocation.rectangle.min.x as u32, alloc.allocation.rectangle.min.y as u32), size, content);
                }
                return alloc;
            }
            // grow the allocation inside the allocator until we have enough free space
            alloc.grow(alloc.size() * 2);
            realloc = true;
        }
    }

    pub fn dealloc(&self, path: String) -> bool {
        self.alloc_map.write().unwrap().remove(path.as_str()).is_some()
    }

    pub fn update(&self, cmd_encoder: &mut CommandEncoder) {
        let mut write_queue = self.write_queue.lock().unwrap();
        if !write_queue.is_empty() {
            let new_size = self.alloc.lock().unwrap().size().to_tuple();
            let new_size = (new_size.0 as u32, new_size.1 as u32);
            let new_tex = Arc::new(Self::create_tex(&self.state, new_size, self.texture_format));
            self.gpu_buffer.load()
            cmd_encoder.copy_texture_to_texture(self.gpu_buffer.load().tex.as_image_copy(), new_tex.tex.as_image_copy(), Extent3d {
                width: 0,
                height: 0,
                depth_or_array_layers: 0
            });
            self.gpu_buffer.store(new_tex);
            while let Some(write) = write_queue.pop() {
                self.write_tex(&self.gpu_buffer.load().tex, write.pos, write.size, write.data.deref());
            }
        }
    }

    fn create_tex(state: &Arc<State>, size: (u32, u32), texture_format: TextureFormat) -> TexTriple {
        let tex = state.create_raw_texture(RawTextureBuilder::new()
            .usages(TextureUsages::TEXTURE_BINDING | TextureUsages::COPY_DST).dimensions(size)
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
        let sampler = state.device().create_sampler(&SamplerDescriptor { // FIXME: check values!
            label: None,
            address_mode_u: Default::default(),
            address_mode_v: Default::default(),
            address_mode_w: Default::default(),
            mag_filter: FilterMode::Linear,
            min_filter: FilterMode::Linear,
            mipmap_filter: FilterMode::Linear,
            lod_min_clamp: 0.0,
            lod_max_clamp: 0.0,
            compare: None,
            anisotropy_clamp: None,
            border_color: None,
        });
        TexTriple {
            tex,
            view,
            sampler,
        }
    }

    fn write_tex(&self, tex: &Texture, pos: (u32, u32), size: (u32, u32), content: &[u8]) {
        self.state.queue().write_texture(ImageCopyTexture {
            texture: tex,
            mip_level: 0,
            origin: Origin3d {
                x: pos.0,
                y: pos.1,
                z: 0,
            },
            aspect: TextureAspect::All,
        }, content, ImageDataLayout {
            offset: 1,
            bytes_per_row: None, // FIXME: can we pass the actual values, so we get more optimizations?
            rows_per_image: None, // FIXME: can we pass the actual values, so we get more optimizations?
        }, Extent3d {
            width: size.0,
            height: size.1,
            depth_or_array_layers: 1,
        });
    }

}

pub struct AtlasAlloc {
    allocation: Allocation,
    atlas: Arc<Atlas>,
}

impl AtlasAlloc {
    pub fn uv(&self) -> UV {
        UV(self.allocation.rectangle.min.x as u32, self.allocation.rectangle.min.y as u32)
    }
}

impl Drop for AtlasAlloc {
    fn drop(&mut self) {
        self.atlas.alloc.lock().unwrap().deallocate(self.allocation.id);
    }
}

struct QueuedWrite {
    data: Arc<Box<dyn AsRef<[u8]>>>,
    pos: (u32, u32),
    size: (u32, u32),
}

struct Size(AtomicU64);

impl Size {

    fn new(x: u32, y: u32) -> Self {
        let val = AtomicU64::new((x as u64) | ((y as u64) << 32));
        Self(val)
    }

    fn get(&self) -> (u32, u32) {
        let val = self.0.load(Ordering::Acquire);

        ((val | (u32::MAX)) as u32, (val >> 32) as u32)
    }

    fn set(&self, x: u32, y: u32) {
        self.0.store((x as u64) | ((y as u64) << 32), Ordering::Release);
    }

}
