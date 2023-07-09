use std::sync::Arc;
use iced::widget::{button, column, text, Column};
use crate::Client;
use crate::protocol::UserUuid;

pub fn start() {

}

struct Menu {
    active_account: UserUuid,
}

impl Menu {

    pub fn view(&mut self, client: &Arc<Client>) -> Column<MenuMessage> {
        // We use a column: a simple vertical layout
        column![
            // The increment button. We tell it to produce an
            // `IncrementPressed` message when pressed
            button("ServerList").on_press(MenuMessage::ServerListPressed),

            // We show the value of the counter here
            // text(self.value).size(50),

            // The decrement button. We tell it to produce a
            // `DecrementPressed` message when pressed
            button("Accounts").on_press(MenuMessage::AccountsPressed),

            // The decrement button. We tell it to produce a
            // `DecrementPressed` message when pressed
            button("Settings").on_press(MenuMessage::SettingsPressed),
        ]
    }

    pub fn update(&mut self, message: MenuMessage) {
        match message {
            MenuMessage::ServerListPressed => {}
            MenuMessage::AccountsPressed => {}
            MenuMessage::SettingsPressed => {}
        }
    }

}

#[derive(Debug, Clone, Copy)]
pub enum MenuMessage {
    ServerListPressed,
    AccountsPressed,
    SettingsPressed,
}
