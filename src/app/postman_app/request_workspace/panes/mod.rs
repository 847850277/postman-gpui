mod authorization;
mod body;
mod cookies;
mod key_value;
mod options;
mod script;

pub(in crate::app::postman_app::request_workspace) use authorization::AuthorizationPane;
pub(in crate::app::postman_app::request_workspace) use body::BodyPane;
pub(in crate::app::postman_app::request_workspace) use cookies::{CookiePane, CookiePaneEvent};
pub(in crate::app::postman_app::request_workspace) use key_value::{
    KeyValueRowsKind, KeyValueRowsPane, KeyValueRowsPaneEvent,
};
pub(in crate::app::postman_app::request_workspace) use options::OptionsPane;
pub(in crate::app::postman_app::request_workspace) use script::{ScriptPane, ScriptPaneKind};
