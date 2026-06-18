use crate::feat;
use anyhow::Result;
use feat::LocalBackupFile;
use smartstring::alias::String;

pub async fn create() -> Result<()> {
    feat::create_local_backup().await
}

pub async fn list() -> Result<Vec<LocalBackupFile>> {
    feat::list_local_backup().await
}

pub async fn delete(filename: String) -> Result<()> {
    feat::delete_local_backup(filename).await
}

pub async fn restore(filename: String) -> Result<()> {
    feat::restore_local_backup(filename).await
}

pub async fn import(source: String) -> Result<String> {
    feat::import_local_backup(source).await
}

pub async fn export(filename: String, destination: String) -> Result<()> {
    feat::export_local_backup(filename, destination).await
}
