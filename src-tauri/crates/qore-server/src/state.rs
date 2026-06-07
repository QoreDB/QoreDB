// SPDX-License-Identifier: BUSL-1.1

use std::sync::Arc;

use qore_service::ServiceContext;

use crate::config::ServerConfig;
use crate::controlplane::ControlStore;

#[derive(Clone)]
pub struct AppState {
    pub ctx: Arc<ServiceContext>,
    pub config: Arc<ServerConfig>,
    pub control: ControlStore,
}
