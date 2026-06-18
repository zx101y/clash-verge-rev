use super::CmdResult;
use crate::{cmd::StringifyErr as _, feat, service};
use feat::LocalBackupFile;
use smartstring::alias::String;

/// Create a local backup
#[tauri::command]
pub async fn create_local_backup() -> CmdResult<()> {
    service::backup::create().await.stringify_err()
}

/// List local backups
#[tauri::command]
pub async fn list_local_backup() -> CmdResult<Vec<LocalBackupFile>> {
    service::backup::list().await.stringify_err()
}

/// Delete local backup
#[tauri::command]
pub async fn delete_local_backup(filename: String) -> CmdResult<()> {
    service::backup::delete(filename).await.stringify_err()
}

/// Restore local backup
#[tauri::command]
pub async fn restore_local_backup(filename: String) -> CmdResult<()> {
    service::backup::restore(filename).await.stringify_err()
}

/// Import local backup into the app's backup directory
#[tauri::command]
pub async fn import_local_backup(source: String) -> CmdResult<String> {
    service::backup::import(source).await.stringify_err()
}

/// Export local backup to a user selected destination
#[tauri::command]
pub async fn export_local_backup(filename: String, destination: String) -> CmdResult<()> {
    service::backup::export(filename, destination).await.stringify_err()
}
