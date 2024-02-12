use crate::{AddressMode, certificate, Client, Profile, Server};
use std::sync::{Arc, RwLock};
use rand::Rng;
use crate::ui::wgpu::{ctx, DARK_GRAY_UI};
use crate::ui::wgpu::render::GlyphBuilder;
use crate::ui::wgpu::screen_sys::Screen;
use crate::ui::wgpu::ui::{Button, Coloring, Container, TextBox};

#[derive(Clone)]
pub struct ServerList {
    container: Arc<Container>,
}

impl ServerList {
    pub fn new() -> Self {
        Self {
            container: Arc::new(Container::new()),
        }
    }
}

const ENTRIES_ON_PAGE: usize = 9;

impl Screen for ServerList {
    fn on_active(&mut self, client: &Arc<Client>) {
        let entry_offset = 1.0 / ENTRIES_ON_PAGE as f32;
        for entry in client.config.load().fav_servers.iter().enumerate() {
            let addr = entry.1.addr.clone();
            let server_name = entry.1.name.clone();
            let pos = (0.0, 1.0 - ((entry.0 + 1) as f32 * entry_offset));
            self.container.add(Arc::new(RwLock::new(Box::new(Button {
                inner_box: TextBox {
                    pos,
                    width: 0.2,
                    height: 0.1,
                    coloring: Coloring::Color([
                        DARK_GRAY_UI,
                        DARK_GRAY_UI,
                        DARK_GRAY_UI,
                        DARK_GRAY_UI,
                        DARK_GRAY_UI,
                        DARK_GRAY_UI,
                    ]),
                    texts: vec![GlyphBuilder::new(&entry.1.name, pos, (0.2, 0.1)).in_bounds_off((0.05, 4.0)).build()],
                },
                data: (),
                on_click: Arc::new(Box::new(move |button, client| {
                    println!("test!!");
                    /*match button.inner_box.coloring {
                        Coloring::Color(mut color) => {
                            color[0].r += 0.1;
                            color[1].r += 0.1;
                            color[2].r += 0.1;
                            color[3].r += 0.1;
                            color[4].r += 0.1;
                            color[5].r += 0.1;
                        }
                        Coloring::Tex(_) => {}
                    }
                    button.inner_box.pos.0 += 0.1;*/
                    let mut profiles = client.profile_db.cache_ref().iter().map(|profile| profile.value().clone()).collect::<Vec<_>>();
                    let profile = profiles.remove(rand::thread_rng().gen_range(0..profiles.len()));
                    let profile = Profile::from_existing(profile.name, profile.alias, profile.priv_key, profile.security_proofs);
                    let server = Server::new(client.clone(), profile, AddressMode::V4,
                                                                       certificate::insecure_local::config(),
                                                                       addr,
                                                                       server_name.clone());
                    client.server.store(Some(server));
                }))
            }))));
        }
    }

    fn on_deactive(&mut self, _client: &Arc<Client>) {}

    fn tick(&mut self, _client: &Arc<Client>) {}

    #[inline(always)]
    fn is_closable(&self) -> bool {
        true
    }

    #[inline(always)]
    fn is_tick_always(&self) -> bool {
        false
    }

    #[inline(always)]
    fn is_transparent(&self) -> bool {
        false
    }

    fn container(&self) -> &Arc<Container> {
        &self.container
    }

    fn clone_screen(&self) -> Box<dyn Screen> {
        Box::new(self.clone())
    }

}
