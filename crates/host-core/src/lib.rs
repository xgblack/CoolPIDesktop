pub mod attachments;
pub mod agent_profiles;
mod auto_title;
pub mod broker;
pub mod ecosystem;
pub mod files;
pub mod git;
pub mod git_write;
mod isolation;
pub mod model_config;
mod runtime;
pub mod session;
pub mod store;
mod task;
pub mod terminal;
pub mod trajectory_history;
pub mod trajectory_live;
pub mod workspace;
pub use auto_title::TitlePromptSettings;
pub use broker::ObserverInfo;
pub use runtime::{
    HostError, MIN_VERSION, RuntimeInfo, discover_login_shell, probe, resolve_executable,
};
pub use session::Workbench;
pub use task::{HostEvent, LaunchOptions, RpcQuery, RpcRequest, TaskManager, TaskSnapshot};

pub mod lifecycle;
pub mod terminal_handoff;
pub mod writer_lock;

mod session_export;

mod session_fork;

pub mod session_catalog;

pub mod open_in_app;
