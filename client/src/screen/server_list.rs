use std::net::{Ipv4Addr, SocketAddr, SocketAddrV4};
use crate::render::Renderer;
use crate::screen_sys::Screen;
use crate::ui::{Button, Color, ColorBox, Coloring, Container, TextBox, TextSection};
use crate::{AddressMode, certificate, Client, DbProfile, Profile, ScreenSystem, Server};
use std::sync::{Arc, RwLock};
use openssl::pkey::PKey;
use rand::Rng;
use wgpu_glyph::{HorizontalAlign, Layout, Text, VerticalAlign};
use crate::utils::DARK_GRAY_UI;

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

const ENTRIES_ON_PAGE: usize = 12;

impl Screen for ServerList {
    fn on_active(&mut self, client: &Arc<Client>) {
        let entry_offset = 1.0 / ENTRIES_ON_PAGE as f32;
        for entry in client.config.fav_servers.iter().enumerate() {
            self.container.add(Arc::new(RwLock::new(Box::new(Button {
                inner_box: TextBox {
                    pos: (0.0, 1.0 - ((entry.0 + 1) as f32 * entry_offset)),
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
                    text: TextSection {
                        layout: Layout::default_single_line().v_align(VerticalAlign::Bottom/*Bottom*//*VerticalAlign::Center*/).h_align(HorizontalAlign::Left),
                        text: vec![Text::default().with_scale(30.0)],
                        texts: vec![entry.1.name.clone()],
                    }
                },
                data: None,
                on_click: Arc::new(Box::new(|button, client| {
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
                    let mut profiles = client.profile_db.iter().collect::<Vec<_>>();
                    let profile = profiles.remove(rand::thread_rng().gen_range(0..profiles.len())).unwrap().1;
                    let profile = DbProfile::from_bytes(profile).unwrap();
                    let profile = Profile::from_existing(profile.name, profile.priv_key, profile.security_proofs);
                    client.server.store(Some(
                        pollster::block_on(Server::new(client.clone(), profile, AddressMode::V4,
                                    certificate::insecure_local::config(),
                                    SocketAddr::V4(SocketAddrV4::new(Ipv4Addr::LOCALHOST, 20354)),
                                    "local_test_srv")).unwrap() // FIXME: handle the error gracefully!
                    ));
                }))
            }))));
        }
        /*self.container.add(Arc::new(RwLock::new(Box::new(ColorBox {
            pos: (0.25, 0.25),
            width: 0.5,
            height: 0.5,
            coloring: Coloring::Color([
                Color {
                    r: 1.0,
                    g: 1.0,
                    b: 0.0,
                    a: 1.0,
                },
                Color {
                    r: 1.0,
                    g: 0.0,
                    b: 0.0,
                    a: 1.0,
                },
                Color {
                    r: 1.0,
                    g: 0.0,
                    b: 1.0,
                    a: 1.0,
                },
                Color {
                    r: 0.0,
                    g: 1.0,
                    b: 0.0,
                    a: 1.0,
                },
                Color {
                    r: 0.0,
                    g: 1.0,
                    b: 0.0,
                    a: 1.0,
                },
                Color {
                    r: 0.0,
                    g: 1.0,
                    b: 0.0,
                    a: 1.0,
                },
            ]),
        }))));
        self.container.add(Arc::new(RwLock::new(Box::new(TextBox {
            pos: (0.0, 0.0),
            width: 0.5,
            height: 0.5,
            coloring: Coloring::Color([
                Color {
                    r: 1.0,
                    g: 1.0,
                    b: 0.0,
                    a: 0.2,
                },
                Color {
                    r: 1.0,
                    g: 0.0,
                    b: 0.0,
                    a: 0.2,
                },
                Color {
                    r: 1.0,
                    g: 0.0,
                    b: 1.0,
                    a: 0.2,
                },
                Color {
                    r: 0.0,
                    g: 1.0,
                    b: 0.0,
                    a: 0.2,
                },
                Color {
                    r: 0.0,
                    g: 1.0,
                    b: 0.0,
                    a: 0.2,
                },
                Color {
                    r: 0.0,
                    g: 1.0,
                    b: 0.0,
                    a: 0.2,
                },
            ]),
            text: TextSection { layout: Default::default(), text: vec![Text::new("Teste").with_color([1.0, 1.0, 1.0, 1.0])
                .with_scale(500.0)] }
        }))));*/
    }

    fn on_deactive(&mut self, _client: &Arc<Client>) {}

    fn tick(&mut self, _client: &Arc<Client>, _delta: f64) {}

    fn is_closable(&self) -> bool {
        false
    }

    fn is_tick_always(&self) -> bool {
        true
    }

    fn container(&self) -> &Arc<Container> {
        &self.container
    }

    fn clone_screen(&self) -> Box<dyn Screen> {
        Box::new(self.clone())
    }

}
