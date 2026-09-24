//! Parse lab Ansible inventory seed YAML into declared-host rows.
//!
//! Only `hostname`, `ansible_host`, and `intended_inventory_role` are kept.
//! SSH secrets and other operator OS fields are ignored (never posted).

use serde_yml::Value;

use crate::errors::CliError;

/// One host extracted from an Ansible inventory seed.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SeedHost {
    pub hostname: String,
    pub ansible_host: Option<String>,
    pub intended_role: Option<String>,
}

/// Roles that Observer assigns only with `cluster_number` set.
const CLUSTERED_ROLES: &[&str] = &[
    "cluster_manager",
    "cluster_peer",
    "searchhead_deployer",
    "searchhead_member",
];

pub fn role_requires_cluster_number(role: &str) -> bool {
    CLUSTERED_ROLES.contains(&role)
}

/// Parse an Ansible inventory YAML document (`all.hosts` map).
pub fn parse_seed_yaml(content: &str) -> Result<Vec<SeedHost>, CliError> {
    let root: Value = serde_yml::from_str(content)
        .map_err(|err| CliError::Other(format!("invalid inventory YAML: {err}")))?;
    let hosts_map = root
        .get("all")
        .and_then(|all| all.get("hosts"))
        .and_then(Value::as_mapping)
        .ok_or_else(|| {
            CliError::Other("inventory seed must contain an `all.hosts` mapping".into())
        })?;

    let mut hosts = Vec::with_capacity(hosts_map.len());
    for (key, value) in hosts_map {
        let hostname = key.trim().to_string();
        if hostname.is_empty() {
            return Err(CliError::Other(
                "inventory host key must be a non-empty string".into(),
            ));
        }
        let vars = value.as_mapping();
        let ansible_host = vars
            .and_then(|map| map.get("ansible_host").and_then(|v| scalar_string(Some(v))))
            .filter(|s| !s.is_empty());
        let intended_role = vars
            .and_then(|map| {
                map.get("intended_inventory_role")
                    .and_then(|v| scalar_string(Some(v)))
            })
            .filter(|s| !s.is_empty());
        hosts.push(SeedHost {
            hostname,
            ansible_host,
            intended_role,
        });
    }
    hosts.sort_by(|left, right| left.hostname.cmp(&right.hostname));
    Ok(hosts)
}

fn scalar_string(value: Option<&Value>) -> Option<String> {
    match value? {
        Value::String(text) => Some(text.clone()),
        Value::Bool(flag) => Some(flag.to_string()),
        Value::Number(num) => Some(num.to_string()),
        Value::Tagged(tagged) => scalar_string(Some(tagged.value())),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const SAMPLE: &str = r#"
all:
  children:
    splunk_vlan85:
      hosts:
        splunk-shd-1101:
  hosts:
    splunk-shd-1101:
      ansible_host: "2001:2042:2ee4:4e85::1000"
      ansible_user: deslicer-ops
      ansible_ssh_private_key_file: ~/secrets/deslicer-ops_ed25519
      intended_inventory_role: searchhead_deployer
    splunk-clm-1101:
      ansible_host: "2001:2042:2ee4:4e85::1004"
      intended_inventory_role: cluster_manager
"#;

    #[test]
    fn parses_hosts_and_strips_secrets() {
        let hosts = parse_seed_yaml(SAMPLE).expect("parse");
        assert_eq!(hosts.len(), 2);
        assert_eq!(hosts[0].hostname, "splunk-clm-1101");
        assert_eq!(
            hosts[0].ansible_host.as_deref(),
            Some("2001:2042:2ee4:4e85::1004")
        );
        assert_eq!(hosts[0].intended_role.as_deref(), Some("cluster_manager"));
        assert_eq!(hosts[1].hostname, "splunk-shd-1101");
        assert_eq!(
            hosts[1].intended_role.as_deref(),
            Some("searchhead_deployer")
        );
    }

    #[test]
    fn rejects_missing_all_hosts() {
        let err = parse_seed_yaml("all:\n  children: {}\n").unwrap_err();
        assert!(err.to_string().contains("all.hosts"));
    }

    #[test]
    fn clustered_roles_need_cluster_number() {
        assert!(role_requires_cluster_number("searchhead_deployer"));
        assert!(role_requires_cluster_number("cluster_peer"));
        assert!(!role_requires_cluster_number("forwarder"));
    }
}
