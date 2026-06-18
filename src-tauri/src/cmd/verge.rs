use super::CmdResult;
use crate::{cmd::StringifyErr as _, config::IVerge, service};
use clash_verge_draft::SharedDraft;

/// 获取Verge配置
#[tauri::command]
pub async fn get_verge_config() -> CmdResult<SharedDraft<IVerge>> {
    service::verge::get_config().await.stringify_err()
}

/// 修改Verge配置
#[tauri::command]
pub async fn patch_verge_config(payload: IVerge) -> CmdResult {
    service::verge::patch_config(&payload).await.stringify_err()
}
