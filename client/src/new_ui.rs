use std::net::{Ipv4Addr, SocketAddr, SocketAddrV4};
use std::pin::Pin;
use std::sync::Arc;
use std::sync::mpsc::Receiver;
use std::task::{Context, Poll};
use flume::r#async::RecvStream;
use futures_util::{Stream, StreamExt, TryStreamExt};
use iced::{executor, Renderer, Theme, time};
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
    data: Data,
}

struct Data {
    config: Arc<Config>,
    profiles: Vec<DbProfile>,
    profiles_texts: Vec<String>,
    error_screens: Vec<String>,
    channel_texts: Vec<(String, bool)>,
    active_profile: Option<UserUuid>,
}

impl Ui {
    
    pub fn new() -> Self {
        let client = CLIENT.get().unwrap();
        let profiles = client.profile_db.iter().map(|profile| DbProfile::from_bytes(profile.unwrap().1).unwrap()).collect::<Vec<_>>();
        let profiles_texts = profiles.iter().map(|profile| format!("{} (\"{}\")", profile.name.as_str(), profile.alias.as_str())).collect();
        Self {
            ty: UiType::Menu,
            data: Data { config: client.config.load_full(), profiles, profiles_texts, error_screens: vec![], channel_texts: vec![], active_profile: None },
            client: client.clone(),
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
        // FIXME: update profiles!
        match message {
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
                    Profile::from_existing(profile.name, profile.alias, profile.priv_key, profile.security_proofs)
                } else {
                    let mut profiles = self.client.profile_db.iter().collect::<Vec<_>>();
                    println!("profiles222: {}", profiles.len());
                    let profile = profiles.remove(rand::thread_rng().gen_range(0..profiles.len())).unwrap().1;
                    let profile = DbProfile::from_bytes(profile).unwrap();
                    let profile = Profile::from_existing(profile.name, profile.alias, profile.priv_key, profile.security_proofs);
                    profile
                };
                let uuid = profile.uuid();
                self.data.active_profile = Some(uuid);
                if let Ok(server) = pollster::block_on(Server::new(self.client.clone(), profile, AddressMode::V4,
                                                                   certificate::insecure_local::config(),
                                                                   SocketAddr::V4(SocketAddrV4::new(Ipv4Addr::LOCALHOST, 20354)),
                                                                   server_name.clone())) {
                    let channels_loaded = server.channels.load();
                    let channel_texts = channels_loaded.iter().map(|channel| (channel.1.name.clone(), channel.1.clients.iter().any(|client| &client.uuid == &uuid))).collect::<Vec<_>>();
                    panic!("channels: {}", channel_texts.len());
                    self.data.channel_texts = channel_texts;
                    drop(channels_loaded);
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
                    let channels = server.channels.load();
                    let channel = channels.get(channel_id).unwrap();
                    // FIXME: join channel!
                }
            }
        }
        Command::none()
    }

    fn view(&self) -> Element<'_, Self::Message, Renderer<Self::Theme>> {
        let frame = container(
            column![
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
                        channels.push(row![
                            vertical_space(Length::Fill),
                            button(text(channel.0.as_str())).on_press(UiMessage::ChannelClicked(channel.0.clone())),
                        ].into());
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
                    accounts.push(button(self.data.profiles_texts[i].as_str()).on_press(UiMessage::SwitchAccount(profile.name.clone())).into());
                }
                if let Some(err_screen) = err_screen {
                    container(column(vec![err_screen.into(), row(accounts).into(), frame])).into()
                } else {
                    container(column(vec![row(accounts).into(), frame])).into()
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
    MakeUiMessage(CLIENT.get().unwrap().err_screen_queue.1.stream())
}

struct MakeUiMessage<'a>(RecvStream<'a, String>);

impl<'a> Stream for MakeUiMessage<'a> {
    type Item = UiMessage;

    #[inline]
    fn poll_next(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        match Stream::poll_next(unsafe { Pin::new_unchecked(&mut self.0) }, cx) {
            Poll::Ready(val) => Poll::Ready(val.map(|val| UiMessage::OpenErr(val))),
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
    OpenErr(String),
}
