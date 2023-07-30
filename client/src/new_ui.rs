use std::net::{Ipv4Addr, SocketAddr, SocketAddrV4};
use std::pin::Pin;
use std::sync::Arc;
use std::sync::mpsc::Receiver;
use std::task::{Context, Poll};
use bytes::BytesMut;
use flume::r#async::RecvStream;
use futures_util::{Stream, StreamExt, TryStreamExt};
use iced::{Color, executor, Renderer, Theme, time};
use iced::keyboard;
use iced::subscription::{self, Subscription};
use iced::theme;
use iced::widget::{
    self, button, column, container, horizontal_space, vertical_space, pick_list, row, text,
    text_input,
};
use iced::{Alignment, Application, Command, Element, Event, Length, Settings};
use iced::alignment::Horizontal;
use iced::futures::channel;
use pollster::FutureExt;
use rand::Rng;
use swap_arc::DataPtrConvert;
use uuid::Uuid;
use crate::{certificate, Client, packet};
use crate::config::Config;
use crate::data_structures::conc_once_cell::ConcurrentOnceCell;
use crate::network::AddressMode;
use crate::packet::RemoteProfile;
use crate::profile::Profile;
use crate::profile_db::{DbProfile, uuid_from_pub_key};
use crate::protocol::{RWBytes, UserUuid};
use crate::server::Server;

pub struct Ui {
    pub ty: UiType,
    client: Arc<Client>,
    data: Data,
}

struct Data {
    config: Arc<Config>,
    profiles: Vec<DbProfile>,
    profiles_texts: Vec<String>,
    error_screens: Vec<String>,
    channel_texts: Vec<ChannelText>,
    active_profile: Option<UserUuid>,
}

struct ChannelText {
    uuid: Uuid,
    slots: usize,
    current: bool,
    text: String,
    name: String,
    users: Vec<UserText>,
}

struct UserText {
    name: String,
    uuid: UserUuid,
    text: String,
}

impl Ui {
    
    pub fn new() -> Self {
        let client = CLIENT.get().unwrap();
        let profiles = client.profile_db.cache_ref().iter().map(|profile| profile.value().clone()).collect::<Vec<_>>();
        let profiles_texts = profiles.iter().map(|profile| format!("{} (\"{}\")", profile.name.as_str(), profile.alias.as_str())).collect();
        Self {
            ty: UiType::Menu,
            data: Data { config: client.config.load_full(), profiles, profiles_texts, error_screens: vec![], channel_texts: vec![], active_profile: None },
            client: client.clone(),
        }
    }
    
}

#[derive(Copy, Clone, PartialEq)]
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
        // FIXME: update profiles!
        match message {
            UiMessage::MenuPressed => self.ty = UiType::Menu,
            UiMessage::ServerListPressed => self.ty = UiType::ServerList,
            UiMessage::AccountsPressed => self.ty = UiType::Accounts,
            UiMessage::ConnectToSrv(server_name) => {
                let profile = if let Some(curr_profile) = &self.client.config.load().get_default_account() {
                    let profile = self.client.profile_db.cache_ref().iter().find_map(|profile| {
                        if &UserUuid::from_u256(profile.value().uuid().unwrap()) == curr_profile {
                            Some(profile.value().clone())
                        } else {
                            None
                        }
                    }).unwrap();
                    Profile::from_existing(profile.name, profile.alias, profile.priv_key, profile.security_proofs)
                } else {
                    let mut profiles = self.client.profile_db.cache_ref().iter().map(|entry| entry.value().clone()).collect::<Vec<_>>();
                    let profile = profiles.remove(rand::thread_rng().gen_range(0..profiles.len()));
                    let profile = Profile::from_existing(profile.name, profile.alias, profile.priv_key, profile.security_proofs);
                    profile
                };
                self.data.active_profile = Some(profile.uuid());
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
                if let Some(account) = self.client.profile_db.cache_ref().get(&account) {
                    let config = old_cfg.set_default_account(UserUuid::from_u256(account.value().uuid().unwrap()));
                    config.save().unwrap();
                    let config = Arc::new(config);
                    self.client.config.store(config.clone());
                    self.data.config = config;
                } else {
                    // FIXME: error - the account isn't present!
                }
            }
            UiMessage::OpenErr(err) => {
                self.data.error_screens.push(err);
            }
            UiMessage::ChannelClicked(channel) => {
                let server = self.client.server.load();
                if let Some(server) = server.as_ref() {
                    let channel_names = server.channels_by_name.load();
                    let channel_id = channel_names.get(channel.as_str()).unwrap();
                    // let channels = server.channels.load();
                    // let channel = channels.get(channel_id).unwrap();
                    let packet = packet::ClientPacket::SwitchChannel { channel: channel_id.clone() }.encode().unwrap();
                    server.connection.get().unwrap().send_reliable(&packet).block_on().unwrap();
                }
            }
            UiMessage::ServerConnected => {
                let server = self.client.server.load();
                let channels_loaded = server.as_ref().unwrap().channels.load();
                let mut channels = channels_loaded.iter().map(|channel| channel.1.clone()).collect::<Vec<_>>();
                channels.sort_by(|channel, channel2| channel.sort_id.cmp(&channel2.sort_id));
                let channel_texts = channels.into_iter().map(|channel| {
                    let current = channel.clients.get(self.data.active_profile.as_ref().unwrap()).is_some();
                    ChannelText {
                        uuid: channel.id,
                        slots: channel.slots as usize,
                        current,
                        text: format!("{} ({}/{})", channel.name.as_str(), channel.clients.len(), channel.slots),
                        name: channel.name,
                        users: channel.clients.into_iter().map(|client| UserText {
                            name: client.1.name.clone(),
                            uuid: client.1.uuid,
                            text: client.1.name,
                        }).collect(),
                    }
                }).collect::<Vec<_>>();
                self.data.channel_texts = channel_texts;
                self.ty = UiType::Menu;
            }
            UiMessage::ChannelRemoveUser(channel, user) => {
                let mut channel = self.data.channel_texts.iter_mut().find(|channel_text| &channel_text.uuid == &channel).unwrap();
                let idx = channel.users.iter().enumerate().find(|user_text| &user_text.1.uuid == &user).unwrap().0;
                channel.users.remove(idx);
                channel.current = channel.users.iter().any(|client| &client.uuid == self.data.active_profile.as_ref().unwrap());
                channel.text = format!("{} ({}/{})", channel.name.as_str(), channel.users.len(), channel.slots);
            }
            UiMessage::ChannelAddUser(channel, user) => {
                let mut channel = self.data.channel_texts.iter_mut().find(|channel_text| &channel_text.uuid == &channel).unwrap();
                channel.users.push(UserText {
                    name: user.name.clone(),
                    uuid: user.uuid.clone(),
                    text: user.name.clone(),
                });
                channel.current = channel.users.iter().any(|client| &client.uuid == self.data.active_profile.as_ref().unwrap());
                channel.text = format!("{} ({}/{})", channel.name.as_str(), channel.users.len(), channel.slots);
            }
            UiMessage::UpdateProfiles => {
                let profiles = self.client.profile_db.cache_ref().iter().map(|profile| profile.value().clone()).collect::<Vec<_>>();
                let profiles_texts = profiles.iter().map(|profile| format!("{} (\"{}\")", profile.name.as_str(), profile.alias.as_str())).collect();
                self.data.profiles_texts = profiles_texts;
                self.data.profiles = profiles;
            }
        }
        Command::none()
    }

    fn view(&self) -> Element<'_, Self::Message, Renderer<Self::Theme>> {
        let frame = container(
            column![
                row![
                    vertical_space(Length::Fill),
                    button(text("Menu")).on_press(UiMessage::MenuPressed).style(if self.ty == UiType::Menu {
                        theme::Button::Secondary
                    } else {
                        theme::Button::Primary
                    }),
                    button(text("Server list")).on_press(UiMessage::ServerListPressed).style(if self.ty == UiType::ServerList {
                        theme::Button::Secondary
                    } else {
                        theme::Button::Primary
                    }),
                    button(text("Accounts")).on_press(UiMessage::AccountsPressed).style(if self.ty == UiType::Accounts {
                        theme::Button::Secondary
                    } else {
                        theme::Button::Primary
                    })
                ].spacing(20).padding(20)
                .align_items(Alignment::End)
                .height(Length::Fill),
            ]
                .height(Length::Fill),
        )
            .width(Length::Fill)
            .height(Length::Fill).center_x().into();
        let err_screen = self.data.error_screens.last().map(|x| row![
                    vertical_space(Length::FillPortion(2)),
                    text(x.as_str())
                ]);
        let content = match self.ty {
            UiType::Menu => {
                let server = self.client.server.load();
                let frame = if let Some(server) = server.as_ref() {
                    let mut channels = vec![];
                    for channel in self.data.channel_texts.iter() {
                        let mut users: Vec<Element<UiMessage, Renderer>> = vec![];
                        for user in channel.users.iter() {
                            users.push(button(text(user.name.as_str())/*.style(theme::Text::Color(Color::from_rgb(1.0, 0.0, 0.0)))*/).style(theme::Button::Destructive).into());
                        }
                        let channel = Into::<Element<UiMessage, Renderer>>::into(
                            button(text(channel.text.as_str())).on_press(UiMessage::ChannelClicked(channel.name.clone())).style(if channel.current {
                                theme::Button::Secondary
                            } else {
                                theme::Button::Primary
                            }),
                        );
                        channels.push(Into::<Element<UiMessage, Renderer>>::into(column![channel, Into::<Element<UiMessage, Renderer>>::into(column(users))])); // FIXME: pad users to the left
                        // channels.extend(users);
                    }
                    container(column(vec![column(channels).into(), frame])).into()
                } else {
                    frame
                };
                if let Some(err_screen) = err_screen {
                    container(column(vec![err_screen.into(), frame])).into()
                } else {
                    frame
                }
            }
            UiType::Accounts => {
                let mut accounts = vec![];
                for (i, profile) in self.data.profiles.iter().enumerate() {
                    accounts.push(button(self.data.profiles_texts[i].as_str()).on_press(UiMessage::SwitchAccount(profile.name.clone())).style(if self.data.active_profile == Some(UserUuid::from_u256(profile.uuid().unwrap())) {
                        theme::Button::Secondary
                    } else {
                        theme::Button::Primary
                    }).into());
                }
                if let Some(err_screen) = err_screen {
                    container(column(vec![err_screen.into(), column(accounts).into(), frame])).into()
                } else {
                    container(column(vec![column(accounts).into(), frame])).into()
                }
            }
            UiType::ServerList => {
                let mut servers = vec![];
                for server in self.data.config.fav_servers.iter() {
                    servers.push(button(server.name.as_str()).on_press(UiMessage::ConnectToSrv(server.name.clone())).into());
                }
                if let Some(err_screen) = err_screen {
                    container(column(vec![err_screen.into(), row(servers).into(), frame])).into()
                } else {
                    container(column(vec![row(servers).into(), frame])).into()
                }
            }
        };
        content
    }

    fn subscription(&self) -> Subscription<Self::Message> {
        subscription::run(stream)
    }
    
}

fn stream<'a>() -> MakeUiMessage<'a> {
    MakeUiMessage(CLIENT.get().unwrap().inter_ui_msg_queue.1.stream())
}

struct MakeUiMessage<'a>(RecvStream<'a, InterUiMessage>);

impl<'a> Stream for MakeUiMessage<'a> {
    type Item = UiMessage;

    #[inline]
    fn poll_next(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        match Stream::poll_next(unsafe { Pin::new_unchecked(&mut self.0) }, cx) {
            Poll::Ready(val) => Poll::Ready(val.map(|val| match val {
                InterUiMessage::Error(err) => UiMessage::OpenErr(err),
                InterUiMessage::ServerConnected => UiMessage::ServerConnected,
                InterUiMessage::ChannelRemoveUser(channel, user) => UiMessage::ChannelRemoveUser(channel, user),
                InterUiMessage::ChannelAddUser(channel, user) => UiMessage::ChannelAddUser(channel, user),
                InterUiMessage::UpdateProfiles => UiMessage::UpdateProfiles,
            })),
            Poll::Pending => Poll::Pending,
        }
    }
}

#[derive(Debug, Clone)]
pub enum UiMessage {
    MenuPressed,
    ServerListPressed,
    AccountsPressed,
    ConnectToSrv(String),
    SwitchAccount(String),
    ChannelClicked(String),
    ChannelRemoveUser(Uuid, UserUuid),
    ChannelAddUser(Uuid, RemoteProfile),
    UpdateProfiles,
    OpenErr(String),
    ServerConnected,
}

#[derive(Debug, Clone)]
pub enum InterUiMessage {
    ChannelRemoveUser(Uuid, UserUuid),
    ChannelAddUser(Uuid, RemoteProfile),
    UpdateProfiles,
    Error(String),
    ServerConnected,
}
