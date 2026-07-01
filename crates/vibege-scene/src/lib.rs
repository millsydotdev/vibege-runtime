#![allow(
    clippy::too_many_arguments,
    clippy::new_without_default,
    clippy::collapsible_if,
    clippy::collapsible_match,
    clippy::unnecessary_cast
)]

pub mod input_helper;
pub mod library;
pub mod runtime;
pub mod scene;
pub mod scenes;
pub mod store;

pub use scene::{
    Scene, SceneAction, SceneContext, SceneId, SceneResult, kind::SceneKind, manager::SceneManager,
    message::SceneMessage,
};
