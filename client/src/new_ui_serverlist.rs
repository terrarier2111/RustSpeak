use std::sync::Arc;
use iced::widget::{button, column, text, Column};
use crate::Client;

pub fn start() {

}

struct ServerList {
    name: String,
}

impl ServerList {

    pub fn view(&mut self, client: &Arc<Client>) -> Column<ServerListMessage> {
        // We use a column: a simple vertical layout
        column![
            // The increment button. We tell it to produce an
            // `IncrementPressed` message when pressed
            button("+").on_press(ServerListMessage::ServerPressed(0)),

            // We show the value of the counter here
            // text(self.value).size(50),

            // The decrement button. We tell it to produce a
            // `DecrementPressed` message when pressed
            button("-").on_press(ServerListMessage::ServerPressed(0)),
        ]
    }

}

#[derive(Debug, Clone, Copy)]
pub enum ServerListMessage {
    ServerPressed(usize),
    Scroll(f64),
}
