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
use crate::Client;
use crate::data_structures::conc_once_cell::ConcurrentOnceCell;
use crate::new_ui::UiMessage::ConnectToSrv;

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
            ConnectToSrv(server) => {

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
                    .height(Length::Fill).center_x()
            }
            UiType::Accounts => {
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
                    .height(Length::Fill).center_x()
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
                for server in self.client.config.fav_servers.iter() {
                    servers.push(button(server.name.as_str()).on_press(ConnectToSrv(server.name.clone())).into());
                }
                container(column(vec![row(servers).into(), frame]))

            }
        };
        content.into()
    }
}

#[derive(Debug, Clone)]
pub enum UiMessage {
    TestPressed,
    MenuPressed,
    ServerListPressed,
    AccountsPressed,
    ConnectToSrv(String),
}
