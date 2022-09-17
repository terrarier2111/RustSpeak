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

use crate::render::{Model, Renderer};
use crate::screen_sys::ScreenType::Other;
use std::sync::atomic::{AtomicIsize, Ordering};
use std::sync::{Arc, Mutex, RwLock};
use winit::dpi::{PhysicalPosition, Position};
use winit::event::VirtualKeyCode;
use winit::window::Window;
use crate::ui::Container;

pub trait Screen: Send + Sync {
    // Called once
    fn init(
        &mut self,
        _screen_sys: Arc<ScreenSystem>,
        _renderer: Arc<Renderer>,
    ) {
    }
    fn deinit(
        &mut self,
        _screen_sys: Arc<ScreenSystem>,
        _renderer: Arc<Renderer>,
    ) {
    }

    // May be called multiple times
    fn on_active(
        &mut self,
        screen_sys: Arc<ScreenSystem>,
        renderer: Arc<Renderer>,
    );
    fn on_deactive(
        &mut self,
        screen_sys: Arc<ScreenSystem>,
        renderer: Arc<Renderer>,
    );

    // Called every frame the screen is active
    fn tick(
        &mut self,
        screen_sys: Arc<ScreenSystem>,
        renderer: Arc<Renderer>,
        delta: f64,
    );

    // Events
    fn on_scroll(&mut self, _x: f64, _y: f64) {}

    fn on_resize(
        &mut self,
        _screen_sys: Arc<ScreenSystem>,
        _renderer: Arc<Renderer>,
    ) {
    } // TODO: make non-optional!

    fn on_key_press(&mut self, screen_sys: Arc<ScreenSystem>, key: VirtualKeyCode, down: bool) {
        if key == VirtualKeyCode::Escape && !down && self.is_closable() {
            screen_sys.pop_screen();
        }
    }

    fn on_char_receive(&mut self, _received: char) {}

    fn is_closable(&self) -> bool {
        false
    }

    fn is_tick_always(&self) -> bool {
        false
    }

    fn ty(&self) -> ScreenType {
        Other(String::new())
    }

    fn container(&self) -> &Arc<Container>;

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
    InGame, // FIXME: rework all the variations of this type!
}

#[derive(Clone)]
struct ScreenInfo {
    screen: Arc<Mutex<Box<dyn Screen>>>,
    active: bool,
    last_width: i32,
    last_height: i32,
}

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

    pub fn push_screen(&self, screen: Box<dyn Screen>) {
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
        self.push_screen(screen);
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

    pub fn receive_char(&self, received: char) {
        if let Some(screen) = self.screens.clone().read().unwrap().last() {
            screen
                .screen
                .clone()
                .lock()
                .unwrap()
                .on_char_receive(received);
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
        self: &Arc<Self>,
        delta: f64,
        renderer: &Arc<Renderer>,
        window: &Window,
    ) -> Vec<Model> {
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
                        );
                    }
                    screen.screen.clone().lock().unwrap().deinit(
                        self.clone(),
                        renderer.clone(),
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
                        );
                    }
                }
                let mut current = screens.last_mut().unwrap();
                let curr_screen = current.screen.clone();
                let mut curr_screen = curr_screen.lock().unwrap();
                curr_screen.init(self.clone(), renderer.clone());
                current.active = true;
                curr_screen.on_active(self.clone(), renderer.clone());
            }
            self.lowest_offset.store(-1, Ordering::Release);
            if !was_closable {
                let (width, height) = renderer.dimensions.get();
                window.set_cursor_position(Position::Physical(PhysicalPosition::new(
                    (width / 2) as i32,
                    (height / 2) as i32,
                )));
            }
        }

        let len = self.screens.clone().read().unwrap().len();
        if len == 0 {
            return vec![];
        }
        // Update state for screens
        let tmp = self.screens.clone();
        let mut tmp = tmp.write().unwrap();
        let current = tmp.last_mut().unwrap();
        if !current.active {
            current.active = true;
            current.screen.clone().lock().unwrap().on_active(
                self.clone(),
                renderer.clone(),
            );
        }
        let (width, height) = renderer.dimensions.get();
        if current.last_width != width as i32 || current.last_height != height as i32
        {
            if current.last_width != -1 && current.last_height != -1 {
                for screen in tmp.iter_mut().enumerate() {
                    let inner_screen = screen.1.screen.clone();
                    let mut inner_screen = inner_screen.lock().unwrap();
                    if inner_screen.is_tick_always() || screen.0 == len - 1 {
                        inner_screen.on_resize(self.clone(), renderer.clone());
                        drop(inner_screen);
                        let (width, height) = renderer.dimensions.get();
                        screen.1.last_width = width as i32;
                        screen.1.last_height = height as i32;
                    }
                }
            } else {
                let (width, height) =
                    renderer.dimensions.get();
                current.last_width = width as i32;
                current.last_height = height as i32;
            }
        }
        let mut models = vec![];
        for screen in tmp.iter_mut().enumerate() {
            let inner_screen = screen.1.screen.clone();
            let mut inner_screen = inner_screen.lock().unwrap();
            if inner_screen.is_tick_always() || screen.0 == len - 1 {
                inner_screen.tick(self.clone(), renderer.clone(), delta);
                let mut screen_models = inner_screen.container().build_models();
                models.append(&mut screen_models);
            }
        }
        models
    }

    pub fn on_scroll(&self, x: f64, y: f64) {
        if let Some(screen) = self.screens.clone().read().unwrap().last() {
            screen.screen.clone().lock().unwrap().on_scroll(x, y);
        }
    }
}
