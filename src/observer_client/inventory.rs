//! Observer `GET /api/v1/inventory` (Ansible `--list` JSON).

use std::collections::HashMap;

use serde::{Deserialize, Serialize};

/// One Ansible inventory group after flattening `_meta`.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct InventoryGroup {
    pub name: String,
    #[serde(default)]
    pub hosts: Vec<String>,
    #[serde(default)]
    pub children: Vec<String>,
}

#[derive(Debug, Clone, Deserialize)]
struct AnsibleGroup {
    #[serde(default)]
    hosts: Option<Vec<String>>,
    #[serde(default)]
    children: Option<Vec<String>>,
}

#[derive(Debug, Clone, Deserialize)]
pub(super) struct AnsibleInventory {
    #[serde(rename = "_meta", default)]
    _meta: Option<serde_json::Value>,
    #[serde(flatten)]
    groups: HashMap<String, AnsibleGroup>,
}

impl AnsibleInventory {
    pub(super) fn into_groups(self) -> Vec<InventoryGroup> {
        let mut groups: Vec<InventoryGroup> = self
            .groups
            .into_iter()
            .map(|(name, group)| InventoryGroup {
                name,
                hosts: group.hosts.unwrap_or_default(),
                children: group.children.unwrap_or_default(),
            })
            .collect();
        groups.sort_by(|left, right| left.name.cmp(&right.name));
        groups
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_ansible_list_and_sorts_groups() {
        let body = br#"{
            "_meta": {"hostvars": {"idx1": {"host_id": "11111111-1111-4111-8111-111111111111"}}},
            "forwarders": {"hosts": ["idx1"]},
            "all": {"children": ["forwarders", "ungrouped"]}
        }"#;
        let inventory: AnsibleInventory = serde_json::from_slice(body).expect("inventory");
        let groups = inventory.into_groups();
        assert_eq!(groups.len(), 2);
        assert_eq!(groups[0].name, "all");
        assert_eq!(groups[0].children, vec!["forwarders", "ungrouped"]);
        assert_eq!(groups[1].name, "forwarders");
        assert_eq!(groups[1].hosts, vec!["idx1"]);
    }
}
