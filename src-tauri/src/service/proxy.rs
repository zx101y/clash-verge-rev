use crate::core::{handle, tray::Tray};
use anyhow::Result;
use futures::{StreamExt as _, stream};
use serde::Serialize;
use serde_json::Value;
use std::collections::{BTreeMap, BTreeSet};
use tauri::Emitter as _;
use tauri_plugin_mihomo::models::Proxies;

const DEFAULT_TEST_URL: &str = "https://www.gstatic.com/generate_204";
const DEFAULT_TIMEOUT_MS: u32 = 5_000;
const MAX_CONCURRENT_TESTS: usize = 16;

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct NodeMembership {
    pub group: String,
    pub node: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct NodeDelay {
    pub group: String,
    pub node: String,
    pub delay: u32,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct CurrentNode {
    pub group: String,
    pub node: String,
    pub delay: u32,
}

pub async fn groups() -> Result<Value> {
    Ok(serde_json::to_value(
        handle::Handle::mihomo().await.get_proxies().await?,
    )?)
}

fn node_memberships(proxies: &Proxies) -> Vec<NodeMembership> {
    let mut memberships = proxies
        .proxies
        .iter()
        .filter_map(|(group_name, group)| {
            group.all.as_ref().map(|nodes| {
                nodes.iter().filter_map(|node_name| {
                    let is_leaf = proxies.proxies.get(node_name).is_none_or(|proxy| proxy.all.is_none());
                    is_leaf.then(|| NodeMembership {
                        group: group_name.clone(),
                        node: node_name.clone(),
                    })
                })
            })
        })
        .flatten()
        .collect::<Vec<_>>();
    memberships.sort_by(|left, right| left.group.cmp(&right.group).then_with(|| left.node.cmp(&right.node)));
    memberships.dedup();
    memberships
}

fn current_nodes(proxies: &Proxies) -> Vec<(String, String)> {
    let mut current = proxies
        .proxies
        .iter()
        .filter_map(|(group, proxy)| proxy.now.as_ref().map(|node| (group.clone(), node.clone())))
        .collect::<Vec<_>>();
    current.sort();
    current
}

async fn test_unique_nodes(nodes: impl IntoIterator<Item = String>) -> BTreeMap<String, u32> {
    let unique = nodes.into_iter().collect::<BTreeSet<_>>();
    let mihomo = handle::Handle::mihomo().await;
    let delays = stream::iter(unique)
        .map(|node| async {
            let delay = mihomo
                .delay_proxy_by_name(&node, DEFAULT_TEST_URL, DEFAULT_TIMEOUT_MS)
                .await
                .map_or(0, |result| result.delay);
            (node, delay)
        })
        .buffer_unordered(MAX_CONCURRENT_TESTS)
        .collect::<BTreeMap<_, _>>()
        .await;
    drop(mihomo);
    delays
}

pub async fn nodes() -> Result<Vec<NodeMembership>> {
    let proxies = handle::Handle::mihomo().await.get_proxies().await?;
    Ok(node_memberships(&proxies))
}

pub async fn test_nodes() -> Result<Vec<NodeDelay>> {
    let proxies = handle::Handle::mihomo().await.get_proxies().await?;
    let memberships = node_memberships(&proxies);
    let delays = test_unique_nodes(memberships.iter().map(|item| item.node.clone())).await;
    let mut results = memberships
        .into_iter()
        .map(|item| NodeDelay {
            delay: delays.get(&item.node).copied().unwrap_or_default(),
            group: item.group,
            node: item.node,
        })
        .collect::<Vec<_>>();
    results.sort_by(|left, right| {
        delay_sort_key(left.delay)
            .cmp(&delay_sort_key(right.delay))
            .then_with(|| left.node.cmp(&right.node))
            .then_with(|| left.group.cmp(&right.group))
    });
    Ok(results)
}

pub async fn current() -> Result<Vec<CurrentNode>> {
    let proxies = handle::Handle::mihomo().await.get_proxies().await?;
    let current = current_nodes(&proxies);
    let delays = test_unique_nodes(current.iter().map(|(_, node)| node.clone())).await;
    Ok(current
        .into_iter()
        .map(|(group, node)| CurrentNode {
            delay: delays.get(&node).copied().unwrap_or_default(),
            group,
            node,
        })
        .collect())
}

const fn delay_sort_key(delay: u32) -> u32 {
    if delay == 0 { u32::MAX } else { delay }
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

#[cfg(test)]
mod tests {
    use super::*;

    fn proxy(name: &str, proxy_type: &str, all: Option<Vec<&str>>, now: Option<&str>) -> Value {
        serde_json::json!({
            "alive": true,
            "history": [],
            "extra": {},
            "name": name,
            "udp": true,
            "uot": false,
            "type": proxy_type,
            "xudp": false,
            "tfo": false,
            "mptcp": false,
            "smux": false,
            "interface": "",
            "dialer-proxy": "",
            "routing-mark": 0,
            "provider-name": "",
            "all": all,
            "now": now,
        })
    }

    fn proxies() -> Proxies {
        let mut values = serde_json::Map::new();
        values.insert(
            "Group A".into(),
            proxy(
                "Group A",
                "Selector",
                Some(vec!["Node 2", "Nested", "Node 1"]),
                Some("Node 1"),
            ),
        );
        values.insert(
            "Group B".into(),
            proxy("Group B", "Selector", Some(vec!["Node 1"]), Some("Node 1")),
        );
        values.insert(
            "Nested".into(),
            proxy("Nested", "URLTest", Some(vec!["Node 2"]), Some("Node 2")),
        );
        values.insert("Node 1".into(), proxy("Node 1", "Vmess", None, None));
        values.insert("Node 2".into(), proxy("Node 2", "Trojan", None, None));
        serde_json::from_value(serde_json::json!({ "proxies": values })).expect("deserialize proxies")
    }

    #[test]
    fn lists_leaf_nodes_with_each_group_membership() {
        assert_eq!(
            node_memberships(&proxies()),
            vec![
                NodeMembership {
                    group: "Group A".into(),
                    node: "Node 1".into(),
                },
                NodeMembership {
                    group: "Group A".into(),
                    node: "Node 2".into(),
                },
                NodeMembership {
                    group: "Group B".into(),
                    node: "Node 1".into(),
                },
                NodeMembership {
                    group: "Nested".into(),
                    node: "Node 2".into(),
                },
            ]
        );
    }

    #[test]
    fn lists_current_choice_for_each_group() {
        assert_eq!(
            current_nodes(&proxies()),
            vec![
                ("Group A".into(), "Node 1".into()),
                ("Group B".into(), "Node 1".into()),
                ("Nested".into(), "Node 2".into()),
            ]
        );
    }

    #[test]
    fn timeout_delays_sort_last() {
        assert!(delay_sort_key(20) < delay_sort_key(0));
    }
}
