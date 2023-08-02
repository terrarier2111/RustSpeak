use std::sync::Arc;
use iced::{Application, Settings};
use uuid::Uuid;
use crate::Client;
use crate::packet::RemoteProfile;
use crate::protocol::UserUuid;
use crate::ui::new_ui::Ui;

pub mod new_ui;

#[derive(Copy, Clone, PartialEq, Eq)]
pub enum UiImpl {
    Wgpu,
    Iced,
}

pub fn start_ui(client: Arc<Client>, ui: UiImpl) -> anyhow::Result<()> {
    match ui {
        UiImpl::Wgpu => {
            todo!()
        }
        UiImpl::Iced => {
            new_ui::init_client(client);
            Ok(Ui::run(Settings::default())?)
        }
    }
}

#[derive(Debug, Clone)]
pub enum InterUiMessage {
    ChannelRemoveUser(Uuid, UserUuid),
    ChannelAddUser(Uuid, RemoteProfile),
    UpdateProfiles,
    Error(String),
    ServerConnected,
}
