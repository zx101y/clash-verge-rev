use crate::{
    config::Config,
    core::{handle, tray::Tray},
};
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
pub struct NodeSummary {
    pub node: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct NodeDelay {
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

fn node_summaries(proxies: &Proxies) -> Vec<NodeSummary> {
    let mut unique_nodes = BTreeSet::new();
    for group in proxies.proxies.values() {
        let Some(group_nodes) = group.all.as_ref() else {
            continue;
        };
        for node_name in group_nodes {
            let is_leaf = proxies.proxies.get(node_name).is_none_or(|proxy| proxy.all.is_none());
            if is_leaf {
                unique_nodes.insert(node_name.clone());
            }
        }
    }

    unique_nodes.into_iter().map(|node| NodeSummary { node }).collect()
}

fn resolve_leaf_node(proxies: &Proxies, selected: &str) -> String {
    let mut current = selected;
    let mut visited = BTreeSet::new();
    while visited.insert(current) {
        let Some(next) = proxies
            .proxies
            .get(current)
            .filter(|proxy| proxy.all.is_some())
            .and_then(|proxy| proxy.now.as_deref())
        else {
            break;
        };
        current = next;
    }
    current.to_string()
}

fn primary_selection(proxies: &Proxies, mode: &str, group_order: &[String]) -> Option<(String, String)> {
    if mode.eq_ignore_ascii_case("direct") {
        return Some(("DIRECT".into(), "DIRECT".into()));
    }
    if mode.eq_ignore_ascii_case("global") {
        let selected = proxies.proxies.get("GLOBAL")?.now.as_deref()?;
        return Some(("GLOBAL".into(), resolve_leaf_node(proxies, selected)));
    }

    let mut groups = group_order
        .iter()
        .filter(|name| name.as_str() != "GLOBAL")
        .filter(|name| {
            proxies
                .proxies
                .get(name.as_str())
                .is_some_and(|proxy| proxy.all.is_some())
        })
        .cloned()
        .collect::<Vec<_>>();
    if groups.is_empty() {
        groups = proxies
            .proxies
            .iter()
            .filter(|(name, proxy)| name.as_str() != "GLOBAL" && proxy.all.is_some())
            .map(|(name, _)| name.clone())
            .collect();
        groups.sort();
    }
    if groups.is_empty() {
        let selected = proxies.proxies.get("GLOBAL")?.now.as_deref()?;
        return Some(("GLOBAL".into(), resolve_leaf_node(proxies, selected)));
    }

    const PRIMARY_KEYWORDS: [&str; 5] = ["auto", "select", "proxy", "节点选择", "自动选择"];
    let group = groups
        .iter()
        .find(|name| {
            let lower = name.to_lowercase();
            PRIMARY_KEYWORDS.iter().any(|keyword| lower.contains(keyword))
        })
        .or_else(|| groups.first())?;
    let selected = proxies.proxies.get(group)?.now.as_deref()?;
    Some((group.clone(), resolve_leaf_node(proxies, selected)))
}

async fn current_mode_and_group_order() -> (String, Vec<String>) {
    let mode = Config::clash()
        .await
        .latest_arc()
        .0
        .get("mode")
        .and_then(serde_yaml_ng::Value::as_str)
        .unwrap_or("rule")
        .into();
    let order = Config::runtime()
        .await
        .latest_arc()
        .config
        .as_ref()
        .and_then(|config| config.get("proxy-groups"))
        .and_then(serde_yaml_ng::Value::as_sequence)
        .map(|groups| {
            groups
                .iter()
                .filter_map(|group| group.get("name"))
                .filter_map(serde_yaml_ng::Value::as_str)
                .map(String::from)
                .collect()
        })
        .unwrap_or_default();
    (mode, order)
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

pub async fn nodes() -> Result<Vec<NodeSummary>> {
    let proxies = handle::Handle::mihomo().await.get_proxies().await?;
    Ok(node_summaries(&proxies))
}

pub async fn test_nodes() -> Result<Vec<NodeDelay>> {
    let proxies = handle::Handle::mihomo().await.get_proxies().await?;
    let nodes = node_summaries(&proxies);
    let delays = test_unique_nodes(nodes.iter().map(|item| item.node.clone())).await;
    let mut results = nodes
        .into_iter()
        .map(|item| NodeDelay {
            delay: delays.get(&item.node).copied().unwrap_or_default(),
            node: item.node,
        })
        .collect::<Vec<_>>();
    results.sort_by(|left, right| {
        delay_sort_key(left.delay)
            .cmp(&delay_sort_key(right.delay))
            .then_with(|| left.node.cmp(&right.node))
    });
    Ok(results)
}

pub async fn current() -> Result<Option<CurrentNode>> {
    let proxies = handle::Handle::mihomo().await.get_proxies().await?;
    let (mode, group_order) = current_mode_and_group_order().await;
    let Some((group, node)) = primary_selection(&proxies, &mode, &group_order) else {
        return Ok(None);
    };
    let delays = test_unique_nodes([node.clone()]).await;
    Ok(Some(CurrentNode {
        delay: delays.get(&node).copied().unwrap_or_default(),
        group,
        node,
    }))
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
#[allow(clippy::expect_used)]
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
    fn lists_each_leaf_node_once() {
        assert_eq!(
            node_summaries(&proxies()),
            vec![
                NodeSummary { node: "Node 1".into() },
                NodeSummary { node: "Node 2".into() },
            ]
        );
    }

    #[test]
    fn selects_one_primary_rule_group_and_resolves_nested_node() {
        assert_eq!(
            primary_selection(&proxies(), "rule", &["Nested".into(), "Group A".into()]),
            Some(("Nested".into(), "Node 2".into()))
        );
    }

    #[test]
    fn global_mode_resolves_the_selected_group_to_a_leaf_node() {
        let mut proxies = proxies();
        proxies.proxies.insert(
            "GLOBAL".into(),
            serde_json::from_value(proxy("GLOBAL", "Selector", Some(vec!["Group A"]), Some("Group A")))
                .expect("deserialize global"),
        );
        assert_eq!(
            primary_selection(&proxies, "global", &[]),
            Some(("GLOBAL".into(), "Node 1".into()))
        );
    }

    #[test]
    fn rule_mode_falls_back_to_global_when_no_rule_group_exists() {
        let mut proxies = proxies();
        proxies
            .proxies
            .retain(|name, _| matches!(name.as_str(), "Node 1" | "GLOBAL"));
        proxies.proxies.insert(
            "GLOBAL".into(),
            serde_json::from_value(proxy("GLOBAL", "Selector", Some(vec!["Node 1"]), Some("Node 1")))
                .expect("deserialize global"),
        );
        assert_eq!(
            primary_selection(&proxies, "rule", &[]),
            Some(("GLOBAL".into(), "Node 1".into()))
        );
    }

    #[test]
    fn timeout_delays_sort_last() {
        assert!(delay_sort_key(20) < delay_sort_key(0));
    }
}
