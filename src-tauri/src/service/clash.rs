use crate::{
    config::{ClashInfo, Config},
    core::{CoreManager, handle},
    feat,
};
use anyhow::Result;
use clash_verge_logging::{Type, logging_error};
use serde_yaml_ng::Mapping;
use smartstring::alias::String;

pub async fn get_info() -> ClashInfo {
    Config::clash().await.data_arc().get_client_info()
}

pub async fn patch_config(payload: &Mapping) -> Result<()> {
    feat::patch_clash(payload).await
}

pub async fn patch_mode(mode: String) {
    feat::change_clash_mode(mode).await;
}

pub async fn start() -> Result<()> {
    let result = CoreManager::global().start_core().await;
    if result.is_ok() {
        handle::Handle::refresh_clash();
    }
    result
}

pub async fn stop() -> Result<()> {
    logging_error!(Type::Core, Config::profiles().await.data_arc().save_file().await);
    let result = CoreManager::global().stop_core().await;
    if result.is_ok() {
        handle::Handle::refresh_clash();
    }
    result
}

pub async fn restart() -> Result<()> {
    logging_error!(Type::Core, Config::profiles().await.data_arc().save_file().await);
    let result = CoreManager::global().restart_core().await;
    if result.is_ok() {
        handle::Handle::refresh_clash();
    }
    result
}
