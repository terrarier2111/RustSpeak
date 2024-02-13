use std::fmt::Debug;
use std::sync::Arc;
use uuid::Uuid;
use crate::server::Server;
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

pub fn ui_queue(ui: UiImpl) -> Box<dyn UiQueue> {
    match ui {
        UiImpl::Wgpu => wgpu::queue(),
        UiImpl::Iced => todo!(),
    }
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

#[derive(Clone)]
pub enum InterUiMessage {
    ChannelRemoveUser(Arc<Server>, Uuid, UserUuid),
    ChannelAddUser(Arc<Server>, Uuid, RemoteProfile),
    UpdateProfiles,
    Error(Arc<Server>, String),
    ServerConnected(Arc<Server>),
}

impl Debug for InterUiMessage {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::ChannelRemoveUser(_, channel_uuid, user_uuid) => f.debug_tuple("ChannelRemoveUser").field(channel_uuid).field(user_uuid).finish(),
            Self::ChannelAddUser(_, channel_uuid, profile) => f.debug_tuple("ChannelAddUser").field(channel_uuid).field(profile).finish(),
            Self::UpdateProfiles => write!(f, "UpdateProfiles"),
            Self::Error(_, err) => f.debug_tuple("Error").field(err).finish(),
            Self::ServerConnected(_) => write!(f, "ServerConnected"),
        }
    }
}

pub(crate) type UiQueueSender = Box<dyn Fn(InterUiMessage) + Send + Sync + 'static>;

pub trait UiQueue: Send + Sync + 'static {

    fn send(&self, msg: InterUiMessage);

}

impl<U: Fn(InterUiMessage) + Send + Sync + 'static> UiQueue for U {
    fn send(&self, msg: InterUiMessage) {
        self(msg)
    }
}

impl UiQueue for dyn Fn(InterUiMessage) + Send + Sync + 'static {
    fn send(&self, msg: InterUiMessage) {
        self(msg)
    }
}
