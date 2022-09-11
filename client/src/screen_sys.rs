// Copyright 2016 Matthew Collins
//
// Licensed under the Apache License, Version 2.0 (the "License");
// you may not use this file except in compliance with the License.
// You may obtain a copy of the License at
//
//     http://www.apache.org/licenses/LICENSE-2.0
//
// Unless required by applicable law or agreed to in writing, software
// distributed under the License is distributed on an "AS IS" BASIS,
// WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
// See the License for the specific language governing permissions and
// limitations under the License.

/*
use crate::render::Renderer;
use crate::screen::ScreenType::Other;
use crate::screen_sys::ScreenType::Other;
use crate::ui;
use crate::ui::Container;
use crate::{render, Game};
use std::sync::atomic::{AtomicIsize, Ordering};
use std::sync::{Arc, Mutex, RwLock};
use winit::dpi::{PhysicalPosition, Position};
use winit::event::VirtualKeyCode;
use winit::window::Window;

pub trait Screen: Send + Sync {
    // Called once
    fn init(
        &mut self,
        _screen_sys: Arc<ScreenSystem>,
        _renderer: Arc<Renderer>,
        _ui_container: &mut Container,
    ) {
    }
    fn deinit(
        &mut self,
        _screen_sys: Arc<ScreenSystem>,
        _renderer: Arc<Renderer>,
        _ui_container: &mut Container,
    ) {
    }

    // May be called multiple times
    fn on_active(
        &mut self,
        screen_sys: Arc<ScreenSystem>,
        renderer: Arc<Renderer>,
        ui_container: &mut Container,
    );
    fn on_deactive(
        &mut self,
        screen_sys: Arc<ScreenSystem>,
        renderer: Arc<Renderer>,
        ui_container: &mut Container,
    );

    // Called every frame the screen is active
    fn tick(
        &mut self,
        screen_sys: Arc<ScreenSystem>,
        renderer: Arc<Renderer>,
        ui_container: &mut Container,
        delta: f64,
    );

    // Events
    fn on_scroll(&mut self, _x: f64, _y: f64) {}

    fn on_resize(
        &mut self,
        _screen_sys: Arc<ScreenSystem>,
        _renderer: Arc<Renderer>,
        _ui_container: &mut Container,
    ) {
    } // TODO: make non-optional!

    fn on_key_press(&mut self, screen_sys: Arc<ScreenSystem>, key: VirtualKeyCode, down: bool) {
        if key == VirtualKeyCode::Escape && !down && self.is_closable() {
            screen_sys.pop_screen();
        }
    }

    fn on_char_receive(&mut self, _received: char, _game: &mut Game) {}

    fn is_closable(&self) -> bool {
        false
    }

    fn is_tick_always(&self) -> bool {
        false
    }

    fn ty(&self) -> ScreenType {
        Other(String::new())
    }

    fn clone_screen(&self) -> Box<dyn Screen>;
}

impl Clone for Box<dyn Screen> {
    fn clone(&self) -> Box<dyn Screen> {
        self.clone_screen()
    }
}

#[derive(Eq, PartialEq)]
pub enum ScreenType {
    Other(String), // FIXME: maybe convert this into a "&'a str" or maybe even into a "&'static str"
    Chat,
    InGame,
}

#[derive(Clone)]
struct ScreenInfo {
    screen: Arc<Mutex<Box<dyn Screen>>>,
    active: bool,
    last_width: i32,
    last_height: i32,
}

// SAFETY:
// This is safe because the only non-Send, non-Sync types in ScreenSystem
// are boxed Screen types in pre_computed_screens which won't ever be modified
// they will only be cloned once and then put inside an Arc<Mutex<>>
// which means the Screen won't ever race
// FIXME: this SHOULD be safe but we can get rid of it by
// FIXME: making all the UI elements thread safe.
/*
unsafe impl Send for ScreenSystem {}
unsafe impl Sync for ScreenSystem {}*/

#[derive(Default)]
pub struct ScreenSystem {
    screens: Arc<RwLock<Vec<ScreenInfo>>>,
    pre_computed_screens: Arc<RwLock<Vec<Box<dyn Screen>>>>,
    lowest_offset: AtomicIsize,
}

impl ScreenSystem {
    pub fn new() -> Self {
        Default::default()
    }

    pub fn add_screen(&self, screen: Box<dyn Screen>) {
        let new_offset = self.pre_computed_screens.clone().read().unwrap().len() as isize;
        self.pre_computed_screens
            .clone()
            .write()
            .unwrap()
            .push(screen);
        let _ = self.lowest_offset.compare_exchange(
            -1,
            new_offset,
            Ordering::Acquire,
            Ordering::Relaxed,
        );
    }

    pub fn close_closable_screens(&self) {
        while self.is_current_closable() {
            self.pop_screen();
        }
    }

    pub fn pop_screen(&self) {
        let pre_computed_screens = self.pre_computed_screens.clone();
        let mut pre_computed_screens = pre_computed_screens.write().unwrap();
        if pre_computed_screens.last().is_some() {
            pre_computed_screens.pop();
            let new_offset = pre_computed_screens.len() as isize;
            let _ = self.lowest_offset.fetch_update(
                Ordering::Release,
                Ordering::Acquire,
                |curr_offset| {
                    if curr_offset == -1 || new_offset < curr_offset {
                        Some(new_offset)
                    } else {
                        None
                    }
                },
            );
        }
    }

    pub fn replace_screen(&self, screen: Box<dyn Screen>) {
        self.pop_screen();
        self.add_screen(screen);
    }

    pub fn is_current_closable(&self) -> bool {
        if let Some(last) = self.pre_computed_screens.clone().read().unwrap().last() {
            return last.is_closable();
        }
        false
    }

    pub fn is_current_ingame(&self) -> bool {
        if let Some(last) = self.pre_computed_screens.clone().read().unwrap().last() {
            return last.ty() == ScreenType::InGame;
        }
        false
    }

    pub fn is_any_ingame(&self) -> bool {
        for screen in self
            .pre_computed_screens
            .clone()
            .read()
            .unwrap()
            .iter()
            .rev()
        {
            if screen.ty() == ScreenType::InGame {
                return true;
            }
        }
        false
    }

    pub fn current_screen_ty(&self) -> ScreenType {
        if let Some(last) = self.pre_computed_screens.clone().read().unwrap().last() {
            return last.ty();
        }
        Other(String::new())
    }

    pub fn receive_char(&self, received: char, game: &mut Game) {
        if let Some(screen) = self.screens.clone().read().unwrap().last() {
            screen
                .screen
                .clone()
                .lock()
                .unwrap()
                .on_char_receive(received, game);
        }
    }

    pub fn press_key(self: Arc<Self>, key: VirtualKeyCode, down: bool) {
        if let Some(screen) = self.screens.clone().read().unwrap().last() {
            screen
                .screen
                .clone()
                .lock()
                .unwrap()
                .on_key_press(self, key, down);
        }
    }

    #[allow(unused_must_use)]
    pub fn tick(
        self: Arc<Self>,
        delta: f64,
        renderer: Arc<Renderer>,
        ui_container: &mut Container,
        window: &Window,
    ) -> bool {
        let lowest = self.lowest_offset.load(Ordering::Acquire);
        if lowest != -1 {
            let screens_len = self.screens.read().unwrap().len();
            let was_closable = if screens_len > 0 {
                self.screens
                    .read()
                    .unwrap()
                    .last()
                    .as_ref()
                    .unwrap()
                    .screen
                    .lock()
                    .unwrap()
                    .is_closable()
            } else {
                false
            };
            if lowest <= screens_len as isize {
                for _ in 0..(screens_len as isize - lowest) {
                    let screen = self.screens.clone().write().unwrap().pop().unwrap();
                    if screen.active {
                        screen.screen.clone().lock().unwrap().on_deactive(
                            self.clone(),
                            renderer.clone(),
                            ui_container,
                        );
                    }
                    screen.screen.clone().lock().unwrap().deinit(
                        self.clone(),
                        renderer.clone(),
                        ui_container,
                    );
                }
            }
            for screen in self
                .pre_computed_screens
                .read()
                .unwrap()
                .iter()
                .skip(lowest as usize)
            {
                let mut screens = self.screens.write().unwrap();
                let idx = (screens.len() as isize - 1).max(0) as usize;
                screens.push(ScreenInfo {
                    screen: Arc::new(Mutex::new(screen.clone())),
                    active: false,
                    last_width: -1,
                    last_height: -1,
                });
                let last = screens.get_mut(idx);
                if let Some(last) = last {
                    if last.active {
                        last.active = false;
                        last.screen.clone().lock().unwrap().on_deactive(
                            self.clone(),
                            renderer.clone(),
                            ui_container,
                        );
                    }
                }
                let mut current = screens.last_mut().unwrap();
                let curr_screen = current.screen.clone();
                let mut curr_screen = curr_screen.lock().unwrap();
                curr_screen.init(self.clone(), renderer.clone(), ui_container);
                current.active = true;
                curr_screen.on_active(self.clone(), renderer.clone(), ui_container);
            }
            self.lowest_offset.store(-1, Ordering::Release);
            if !was_closable {
                let (safe_width, safe_height) = renderer.screen_data.read().unwrap().safe_dims();
                window.set_cursor_position(Position::Physical(PhysicalPosition::new(
                    (safe_width / 2) as i32,
                    (safe_height / 2) as i32,
                )));
            }
        }

        let len = self.screens.clone().read().unwrap().len();
        if len == 0 {
            return true;
        }
        // Update state for screens
        {
            let tmp = self.screens.clone();
            let mut tmp = tmp.write().unwrap();
            let current = tmp.last_mut().unwrap();
            if !current.active {
                current.active = true;
                current.screen.clone().lock().unwrap().on_active(
                    self,
                    renderer.clone(),
                    ui_container,
                );
            }
            let (safe_width, safe_height) = renderer.screen_data.read().unwrap().safe_dims();
            if current.last_width != safe_width as i32 || current.last_height != safe_height as i32
            {
                if current.last_width != -1 && current.last_height != -1 {
                    for screen in tmp.iter_mut().enumerate() {
                        let inner_screen = screen.1.screen.clone();
                        let mut inner_screen = inner_screen.lock().unwrap();
                        if inner_screen.is_tick_always() || screen.0 == len - 1 {
                            inner_screen.on_resize(self.clone(), renderer.clone(), ui_container);
                            drop(inner_screen);
                            let (safe_width, safe_height) =
                                renderer.screen_data.read().unwrap().safe_dims();
                            screen.1.last_width = safe_width as i32;
                            screen.1.last_height = safe_height as i32;
                        }
                    }
                } else {
                    let (safe_width, safe_height) =
                        renderer.screen_data.read().unwrap().safe_dims();
                    current.last_width = safe_width as i32;
                    current.last_height = safe_height as i32;
                }
            }
            for screen in tmp.iter_mut().enumerate() {
                let inner_screen = screen.1.screen.clone();
                let mut inner_screen = inner_screen.lock().unwrap();
                if inner_screen.is_tick_always() || screen.0 == len - 1 {
                    inner_screen.tick(self.clone(), renderer.clone(), ui_container, delta);
                }
            }
        }
        // Handle current
        return self.screens.clone().read().unwrap()[len - 1]
            .screen
            .clone()
            .lock()
            .unwrap()
            .ty()
            != ScreenType::InGame;
    }

    pub fn on_scroll(&self, x: f64, y: f64) {
        if let Some(screen) = self.screens.clone().read().unwrap().last() {
            screen.screen.clone().lock().unwrap().on_scroll(x, y);
        }
    }
}
*/