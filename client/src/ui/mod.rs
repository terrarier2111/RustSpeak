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

#[derive(Debug, Clone)]
pub enum InterUiMessage {
    ChannelRemoveUser(Uuid, UserUuid),
    ChannelAddUser(Uuid, RemoteProfile),
    UpdateProfiles,
    Error(String),
    ServerConnected,
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
