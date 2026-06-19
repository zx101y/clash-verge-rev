use crate::{
    config::{Config, IProfiles, PrfOption},
    feat,
};
use anyhow::{Result, anyhow};
use clash_verge_draft::SharedDraft;
use smartstring::alias::String;

pub async fn get_config() -> SharedDraft<IProfiles> {
    Config::profiles().await.data_arc()
}

fn resolve_id_in(profiles: &IProfiles, id_or_name: &str) -> Result<String> {
    if profiles.get_item(id_or_name).is_ok() {
        return Ok(id_or_name.into());
    }

    profiles
        .items
        .as_ref()
        .and_then(|items| {
            items
                .iter()
                .find(|item| item.name.as_deref() == Some(id_or_name))
                .and_then(|item| item.uid.clone())
        })
        .ok_or_else(|| anyhow!("profile not found: {id_or_name}"))
}

pub async fn resolve_id(id_or_name: &str) -> Result<String> {
    let profiles = Config::profiles().await.latest_arc();
    resolve_id_in(&profiles, id_or_name)
}

pub async fn update(index: &String, option: Option<&PrfOption>) -> Result<()> {
    feat::update_profile(index, option, true, true, true).await
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;
    use crate::config::PrfItem;

    fn profiles() -> IProfiles {
        IProfiles {
            current: Some("first-id".into()),
            items: Some(vec![
                PrfItem {
                    uid: Some("first-id".into()),
                    name: Some("First".into()),
                    ..Default::default()
                },
                PrfItem {
                    uid: Some("second-id".into()),
                    name: Some("Second".into()),
                    ..Default::default()
                },
            ]),
        }
    }

    #[test]
    fn resolves_profile_by_uid() {
        assert_eq!(resolve_id_in(&profiles(), "second-id").unwrap(), "second-id");
    }

    #[test]
    fn resolves_profile_by_name() {
        assert_eq!(resolve_id_in(&profiles(), "First").unwrap(), "first-id");
    }

    #[test]
    fn rejects_unknown_profile() {
        assert!(resolve_id_in(&profiles(), "Missing").is_err());
    }
}
