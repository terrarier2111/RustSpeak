use std::sync::Arc;
use uuid::Uuid;
use crate::Client;
use crate::packet::RemoteProfile;
use crate::protocol::UserUuid;

// mod iced; // FIXME: once iced updated to the most recent wgpu, add this again!
mod wgpu;

#[derive(Copy, Clone, PartialEq, Eq)]
pub enum UiImpl {
    Wgpu,
    Iced,
}

pub fn start_ui(client: Arc<Client>, ui: UiImpl) -> anyhow::Result<()> {
    match ui {
        UiImpl::Wgpu => {
            wgpu::run(client)
        }
        UiImpl::Iced => {
            // iced::run(client)
            todo!()
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
