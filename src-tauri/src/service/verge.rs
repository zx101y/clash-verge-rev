use crate::{config::IVerge, feat};
use anyhow::Result;
use clash_verge_draft::SharedDraft;

pub async fn get_config() -> Result<SharedDraft<IVerge>> {
    feat::fetch_verge_config().await
}

pub async fn patch_config(payload: &IVerge) -> Result<()> {
    feat::patch_verge(payload, false).await
}
