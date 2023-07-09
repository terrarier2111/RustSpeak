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

pub enum Ui {
    Menu,
    Accounts,
    ServerList,
}

impl Application for Ui {
    type Executor = executor::Default;
    type Message = UiMessage;
    type Theme = Theme;
    type Flags = ();

    fn new(flags: Self::Flags) -> (Self, Command<Self::Message>) {
        (Self::Menu, Command::none())
    }

    fn title(&self) -> String {
        "RustSpeak".to_string()
    }

    fn update(&mut self, message: Self::Message) -> Command<Self::Message> {
        match message {
            UiMessage::TestPressed => {}
            UiMessage::MenuPressed => *self = Ui::Menu,
            UiMessage::ServerListPressed => *self = Ui::ServerList,
            UiMessage::AccountsPressed => *self = Ui::Accounts,
        }
        Command::none()
    }

    fn view(&self) -> Element<'_, Self::Message, Renderer<Self::Theme>> {
        let content = match self {
            Ui::Menu => {
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
            Ui::Accounts => {
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
            Ui::ServerList => {
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
}
