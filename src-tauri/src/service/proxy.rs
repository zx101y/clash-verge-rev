use crate::core::{handle, tray::Tray};
use anyhow::Result;
use serde_json::Value;
use tauri::Emitter as _;

pub async fn groups() -> Result<Value> {
    Ok(serde_json::to_value(
        handle::Handle::mihomo().await.get_proxies().await?,
    )?)
}

pub async fn select(group: &str, node: &str) -> Result<()> {
    handle::Handle::mihomo()
        .await
        .select_node_for_group(group, node)
        .await?;
    let _ = handle::Handle::app_handle().emit("verge://refresh-proxy-config", ());
    let _ = Tray::global().update_menu().await;
    Ok(())
}

pub async fn connections() -> Result<Value> {
    Ok(serde_json::to_value(
        handle::Handle::mihomo().await.get_connections().await?,
    )?)
}

pub async fn close_connection(id: Option<&str>) -> Result<()> {
    let mihomo = handle::Handle::mihomo().await;
    if let Some(id) = id {
        mihomo.close_connection(id).await?;
    } else {
        mihomo.close_all_connections().await?;
    }
    drop(mihomo);
    Ok(())
}
