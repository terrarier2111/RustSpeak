use std::net::{Ipv4Addr, SocketAddr, SocketAddrV4};
use std::sync::Arc;
use iced::{executor, Renderer, Theme};
use iced::keyboard;
use iced::subscription::{self, Subscription};
use iced::theme;
use iced::widget::{
    self, button, column, container, horizontal_space, vertical_space, pick_list, row, text,
    text_input,
};
use iced::{Alignment, Application, Command, Element, Event, Length, Settings};
use iced::alignment::Horizontal;
use rand::Rng;
use crate::{certificate, Client};
use crate::config::Config;
use crate::data_structures::conc_once_cell::ConcurrentOnceCell;
use crate::network::AddressMode;
use crate::profile::Profile;
use crate::profile_db::{DbProfile, uuid_from_pub_key};
use crate::protocol::UserUuid;
use crate::server::Server;

pub struct Ui {
    pub ty: UiType,
    client: Arc<Client>,
}

impl Ui {
    
    pub fn new() -> Self {
        Self {
            ty: UiType::Menu,
            client: CLIENT.get().unwrap().clone(),
        }
    }
    
}

pub enum UiType {
    Menu,
    Accounts,
    ServerList,
}

static CLIENT: ConcurrentOnceCell<Arc<Client>> = ConcurrentOnceCell::new();

pub fn init_client(client: Arc<Client>) {
    CLIENT.try_init_silent(client).unwrap();
}

impl Application for Ui {
    type Executor = executor::Default;
    type Message = UiMessage;
    type Theme = Theme;
    type Flags = ();

    fn new(flags: Self::Flags) -> (Self, Command<Self::Message>) {
        (Self::new(), Command::none())
    }

    fn title(&self) -> String {
        "RustSpeak".to_string()
    }

    fn update(&mut self, message: Self::Message) -> Command<Self::Message> {
        match message {
            UiMessage::TestPressed => {}
            UiMessage::MenuPressed => self.ty = UiType::Menu,
            UiMessage::ServerListPressed => self.ty = UiType::ServerList,
            UiMessage::AccountsPressed => self.ty = UiType::Accounts,
            UiMessage::ConnectToSrv(server_name) => {
                let profile = if let Some(curr_profile) = &self.client.config.load().get_default_account() {
                    let profile = self.client.profile_db.iter().find_map(|raw| {
                        let profile = DbProfile::from_bytes(raw.unwrap().1).unwrap();
                        if &UserUuid::from_u256(profile.uuid().unwrap()) == curr_profile {
                            Some(profile)
                        } else {
                            None
                        }
                    }).unwrap();
                    Profile::from_existing(profile.name, profile.priv_key, profile.security_proofs)
                } else {
                    let mut profiles = self.client.profile_db.iter().collect::<Vec<_>>();
                    let profile = profiles.remove(rand::thread_rng().gen_range(0..profiles.len())).unwrap().1;
                    let profile = DbProfile::from_bytes(profile).unwrap();
                    let profile = Profile::from_existing(profile.name, profile.priv_key, profile.security_proofs);
                    profile
                };
                if let Ok(server) = pollster::block_on(Server::new(self.client.clone(), profile, AddressMode::V4,
                                                                   certificate::insecure_local::config(),
                                                                   SocketAddr::V4(SocketAddrV4::new(Ipv4Addr::LOCALHOST, 20354)),
                                                                   server_name.clone())) {
                    self.client.server.store(Some(server));
                } else {
                    // FIXME: connection failure!
                }
            }
            UiMessage::SwitchAccount(account) => {
                let old_cfg = self.client.config.load();
                if let Some(account) = self.client.profile_db.get(&account).ok().flatten() {
                    let config = old_cfg.set_default_account(UserUuid::from_u256(account.uuid().unwrap()));
                    config.save().unwrap();
                    self.client.config.store(Arc::new(config));

                } else {
                    // FIXME: error - the account isn't present!
                }
            }
        }
        Command::none()
    }

    fn view(&self) -> Element<'_, Self::Message, Renderer<Self::Theme>> {
        let content = match self.ty {
            UiType::Menu => {
                container(
                    column![
                /*row![
                    text("Top Left"),
                    horizontal_space(Length::Fill),
                    text("Top Right")
                ]
                .align_items(Alignment::Start)
                .height(Length::Fill),*/
                /*container(
                    button(text("Test")).on_press(UiMessage::TestPressed)
                )
                .center_x()
                .center_y()
                .width(Length::Fill)
                .height(Length::Fill),*/
                row![
                    vertical_space(Length::Fill),
                    button(text("Menu")).on_press(UiMessage::MenuPressed),
                    button(text("Server list")).on_press(UiMessage::ServerListPressed),
                    button(text("Accounts")).on_press(UiMessage::AccountsPressed)
                ].spacing(20).padding(20)
                .align_items(Alignment::End)
                .height(Length::Fill),
            ]
                        .height(Length::Fill),
                )
                    .width(Length::Fill)
                    .height(Length::Fill).center_x().into()
            }
            UiType::Accounts => {
                let frame = container(
                    column![
                /*row![
                    text("Top Left"),
                    horizontal_space(Length::Fill),
                    text("Top Right")
                ]
                .align_items(Alignment::Start)
                .height(Length::Fill),*/
                /*container(
                    button(text("Test")).on_press(UiMessage::TestPressed)
                )
                .center_x()
                .center_y()
                .width(Length::Fill)
                .height(Length::Fill),*/
                row![
                    vertical_space(Length::Fill),
                    button(text("Menu")).on_press(UiMessage::MenuPressed),
                    button(text("Server list")).on_press(UiMessage::ServerListPressed),
                    button(text("Accounts")).on_press(UiMessage::AccountsPressed)
                ].spacing(20).padding(20)
                .align_items(Alignment::End)
                .height(Length::Fill),
            ]
                        .height(Length::Fill),
                )
                    .width(Length::Fill)
                    .height(Length::Fill).center_x().into();

                let profiles = self.client.profile_db.iter().map(|profile| DbProfile::from_bytes(profile.unwrap().1).unwrap()).collect::<Vec<_>>();
                let mut accounts = vec![];
                for profile in profiles.iter() {
                    accounts.push(button(profile.name.as_str()).on_press(UiMessage::SwitchAccount(profile.name.clone())).into());
                }

                container(column(vec![column(accounts).into(), frame])).into()
            }
            UiType::ServerList => {
                let frame = container(
                    column![
                /*row![
                    text("Top Left"),
                    horizontal_space(Length::Fill),
                    text("Top Right")
                ]
                .align_items(Alignment::Start)
                .height(Length::Fill),*/
                /*container(
                    button(text("Test")).on_press(UiMessage::TestPressed)
                )
                .center_x()
                .center_y()
                .width(Length::Fill)
                .height(Length::Fill),*/
                row![
                    vertical_space(Length::Fill),
                    button(text("Menu")).on_press(UiMessage::MenuPressed),
                    button(text("Server list")).on_press(UiMessage::ServerListPressed),
                    button(text("Accounts")).on_press(UiMessage::AccountsPressed)
                ].spacing(20).padding(20)
                .align_items(Alignment::End)
                .height(Length::Fill),
            ]
                        .height(Length::Fill),
                )
                    .width(Length::Fill)
                    .height(Length::Fill).center_x().into();
                let mut servers = vec![];
                let cfg = self.client.config.load_full();
                for server in cfg.fav_servers.iter() {
                    servers.push(button(server.name.as_str()).on_press(UiMessage::ConnectToSrv(server.name.clone())).into());
                }
                container(column(vec![row(servers).into(), frame])).into()

            }
        };
        content
    }
}

#[derive(Debug, Clone)]
pub enum UiMessage {
    TestPressed,
    MenuPressed,
    ServerListPressed,
    AccountsPressed,
    ConnectToSrv(String),
    SwitchAccount(String),
}
