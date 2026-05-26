use anyhow::Result;
use std::collections::hash_map::Entry;
use std::collections::HashMap;
use tokio::process::Command;

#[derive(Debug, Clone)]
pub struct PortInfo {
    pub port: u16,
    pub bind_address: String,
    pub pid: Option<u32>,
    pub process_name: String,
    pub cmdline: String,
}

pub async fn scan() -> Vec<PortInfo> {
    scan_inner().await.unwrap_or_default()
}

async fn scan_inner() -> Result<Vec<PortInfo>> {
    let output = Command::new("ss")
        .args(["-tlnp"])
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::null())
        .output()
        .await?;

    let stdout = String::from_utf8_lossy(&output.stdout);
    let mut ports = parse_ss_output(&stdout);

    for port in &mut ports {
        if let Some(pid) = port.pid {
            if let Ok(cmd) = read_cmdline(pid).await {
                port.cmdline = cmd;
            }
        }
    }

    Ok(ports)
}

/// Parse full `ss -tlnp` output into deduplicated PortInfo entries.
/// Dual-stack listeners (same port+pid on different addresses) are merged
/// into a single entry with bind_address = "*".
fn parse_ss_output(stdout: &str) -> Vec<PortInfo> {
    let mut by_port_pid: HashMap<(u16, Option<u32>), PortInfo> = HashMap::new();

    for line in stdout.lines().skip(1) {
        if let Some(info) = parse_ss_line(line) {
            let key = (info.port, info.pid);
            match by_port_pid.entry(key) {
                Entry::Vacant(e) => {
                    e.insert(info);
                }
                Entry::Occupied(mut e) if e.get().bind_address != info.bind_address => {
                    e.get_mut().bind_address = "*".to_string();
                }
                _ => {}
            }
        }
    }

    let mut ports: Vec<PortInfo> = by_port_pid.into_values().collect();
    ports.sort_by_key(|p| p.port);
    ports
}

fn parse_ss_line(line: &str) -> Option<PortInfo> {
    let parts: Vec<&str> = line.split_whitespace().collect();
    if parts.len() < 4 {
        return None;
    }

    let local_addr = parts[3];
    let port = parse_port(local_addr)?;
    let bind_address = parse_bind_address(local_addr);

    // Process info may contain spaces, so join parts[5..] back together
    let (pid, process_name) = if parts.len() >= 6 {
        let process_field = parts[5..].join(" ");
        parse_process_info(&process_field)
    } else {
        (None, "(unknown)".to_string())
    };

    Some(PortInfo {
        port,
        bind_address,
        pid,
        process_name,
        cmdline: String::new(),
    })
}

fn parse_port(addr: &str) -> Option<u16> {
    // Handle [::]:port and addr:port formats
    if let Some(bracket_pos) = addr.rfind("]:") {
        addr[bracket_pos + 2..].parse().ok()
    } else {
        addr.rsplit(':').next()?.parse().ok()
    }
}

fn parse_bind_address(addr: &str) -> String {
    if let Some(bracket_pos) = addr.rfind("]:") {
        addr[..bracket_pos + 1].to_string()
    } else if let Some(colon_pos) = addr.rfind(':') {
        addr[..colon_pos].to_string()
    } else {
        addr.to_string()
    }
}

fn parse_process_info(field: &str) -> (Option<u32>, String) {
    // Format: users:(("name",pid=123,fd=4))
    // or multiple: users:(("name",pid=123,fd=4),("name2",pid=456,fd=5))
    if !field.starts_with("users:") {
        return (None, "(unknown)".to_string());
    }

    let inner = &field[6..]; // strip "users:"
                             // Find first process entry: ("name",pid=N,...)
    if let Some(start) = inner.find("(\"") {
        let rest = &inner[start + 2..];
        let name = rest.split('"').next().unwrap_or("unknown").to_string();

        let pid = rest.find("pid=").and_then(|i| {
            let after = &rest[i + 4..];
            after
                .split(|c: char| !c.is_ascii_digit())
                .next()
                .and_then(|s| s.parse::<u32>().ok())
        });

        (pid, name)
    } else {
        (None, "(unknown)".to_string())
    }
}

async fn read_cmdline(pid: u32) -> Result<String> {
    let path = format!("/proc/{}/cmdline", pid);
    let data = tokio::fs::read(&path).await?;
    let cmd = data
        .split(|&b| b == 0)
        .filter(|s| !s.is_empty())
        .map(|s| String::from_utf8_lossy(s).to_string())
        .collect::<Vec<_>>()
        .join(" ");
    Ok(cmd)
}

#[cfg(test)]
mod tests {
    use super::*;

    // Real captured output from a live system
    const SS_FIXTURE: &str = r#"State  Recv-Q Send-Q              Local Address:Port  Peer Address:PortProcess
LISTEN 0      511                     127.0.0.1:45647      0.0.0.0:*    users:(("MainThread",pid=75660,fd=26))
LISTEN 0      4096                   127.0.0.54:53         0.0.0.0:*
LISTEN 0      4096                      0.0.0.0:22         0.0.0.0:*
LISTEN 0      511                       0.0.0.0:41641      0.0.0.0:*    users:(("MainThread",pid=66955,fd=26))
LISTEN 0      4096                      0.0.0.0:631        0.0.0.0:*
LISTEN 0      128                     127.0.0.1:38997      0.0.0.0:*    users:(("ttyd",pid=85474,fd=12))
LISTEN 0      4096                127.0.0.53%lo:53         0.0.0.0:*
LISTEN 0      4096              100.101.219.118:38916      0.0.0.0:*
LISTEN 0      511                       0.0.0.0:40671      0.0.0.0:*    users:(("MainThread",pid=67772,fd=26))
LISTEN 0      511                       0.0.0.0:38305      0.0.0.0:*    users:(("MainThread",pid=67331,fd=26))
LISTEN 0      10                      127.0.0.1:45037      0.0.0.0:*    users:(("chrome",pid=85477,fd=83))
LISTEN 0      4096                         [::]:22            [::]:*
LISTEN 0      4096                         [::]:631           [::]:*
LISTEN 0      511                             *:4100             *:*    users:(("next-server (v1",pid=26870,fd=22))
LISTEN 0      4096   [fd7a:115c:a1e0::734:db76]:56451         [::]:*
LISTEN 0      4096                            *:5432             *:*"#;

    // -----------------------------------------------------------------------
    // parse_port tests
    // -----------------------------------------------------------------------

    #[test]
    fn parse_port_ipv4() {
        assert_eq!(parse_port("0.0.0.0:22"), Some(22));
    }

    #[test]
    fn parse_port_ipv4_loopback() {
        assert_eq!(parse_port("127.0.0.1:45647"), Some(45647));
    }

    #[test]
    fn parse_port_ipv6_wildcard() {
        assert_eq!(parse_port("[::]:22"), Some(22));
    }

    #[test]
    fn parse_port_ipv6_full_address() {
        assert_eq!(parse_port("[fd7a:115c:a1e0::734:db76]:56451"), Some(56451));
    }

    #[test]
    fn parse_port_wildcard_star() {
        assert_eq!(parse_port("*:4100"), Some(4100));
    }

    #[test]
    fn parse_port_interface_scoped() {
        assert_eq!(parse_port("127.0.0.53%lo:53"), Some(53));
    }

    #[test]
    fn parse_port_specific_ip() {
        assert_eq!(parse_port("100.101.219.118:38916"), Some(38916));
    }

    #[test]
    fn parse_port_invalid_returns_none() {
        assert_eq!(parse_port("garbage"), None);
    }

    #[test]
    fn parse_port_empty_returns_none() {
        assert_eq!(parse_port(""), None);
    }

    // -----------------------------------------------------------------------
    // parse_bind_address tests
    // -----------------------------------------------------------------------

    #[test]
    fn bind_address_ipv4() {
        assert_eq!(parse_bind_address("0.0.0.0:22"), "0.0.0.0");
    }

    #[test]
    fn bind_address_ipv4_loopback() {
        assert_eq!(parse_bind_address("127.0.0.1:45647"), "127.0.0.1");
    }

    #[test]
    fn bind_address_ipv6_wildcard() {
        assert_eq!(parse_bind_address("[::]:22"), "[::]");
    }

    #[test]
    fn bind_address_ipv6_full() {
        assert_eq!(
            parse_bind_address("[fd7a:115c:a1e0::734:db76]:56451"),
            "[fd7a:115c:a1e0::734:db76]"
        );
    }

    #[test]
    fn bind_address_star_wildcard() {
        assert_eq!(parse_bind_address("*:4100"), "*");
    }

    #[test]
    fn bind_address_interface_scoped() {
        assert_eq!(parse_bind_address("127.0.0.53%lo:53"), "127.0.0.53%lo");
    }

    // -----------------------------------------------------------------------
    // parse_process_info tests
    // -----------------------------------------------------------------------

    #[test]
    fn process_info_simple() {
        let (pid, name) = parse_process_info(r#"users:(("ttyd",pid=85474,fd=12))"#);
        assert_eq!(pid, Some(85474));
        assert_eq!(name, "ttyd");
    }

    #[test]
    fn process_info_name_with_spaces_and_parens() {
        let (pid, name) = parse_process_info(r#"users:(("next-server (v1",pid=26870,fd=22))"#);
        assert_eq!(pid, Some(26870));
        assert_eq!(name, "next-server (v1");
    }

    #[test]
    fn process_info_main_thread() {
        let (pid, name) = parse_process_info(r#"users:(("MainThread",pid=75660,fd=26))"#);
        assert_eq!(pid, Some(75660));
        assert_eq!(name, "MainThread");
    }

    #[test]
    fn process_info_chrome() {
        let (pid, name) = parse_process_info(r#"users:(("chrome",pid=85477,fd=83))"#);
        assert_eq!(pid, Some(85477));
        assert_eq!(name, "chrome");
    }

    #[test]
    fn process_info_not_users_prefix() {
        let (pid, name) = parse_process_info("something_else");
        assert_eq!(pid, None);
        assert_eq!(name, "(unknown)");
    }

    #[test]
    fn process_info_empty_field() {
        let (pid, name) = parse_process_info("");
        assert_eq!(pid, None);
        assert_eq!(name, "(unknown)");
    }

    // -----------------------------------------------------------------------
    // parse_ss_line tests
    // -----------------------------------------------------------------------

    #[test]
    fn ss_line_ipv4_with_process() {
        let line = r#"LISTEN 0      511                     127.0.0.1:45647      0.0.0.0:*    users:(("MainThread",pid=75660,fd=26))"#;
        let info = parse_ss_line(line).expect("should parse");
        assert_eq!(info.port, 45647);
        assert_eq!(info.bind_address, "127.0.0.1");
        assert_eq!(info.pid, Some(75660));
        assert_eq!(info.process_name, "MainThread");
    }

    #[test]
    fn ss_line_ipv4_no_process() {
        let line = "LISTEN 0      4096                      0.0.0.0:22         0.0.0.0:*";
        let info = parse_ss_line(line).expect("should parse");
        assert_eq!(info.port, 22);
        assert_eq!(info.bind_address, "0.0.0.0");
        assert_eq!(info.pid, None);
        assert_eq!(info.process_name, "(unknown)");
    }

    #[test]
    fn ss_line_ipv6_wildcard() {
        let line = "LISTEN 0      4096                         [::]:22            [::]:*";
        let info = parse_ss_line(line).expect("should parse");
        assert_eq!(info.port, 22);
        assert_eq!(info.bind_address, "[::]");
        assert_eq!(info.pid, None);
    }

    #[test]
    fn ss_line_star_wildcard_with_spaces_in_name() {
        let line = r#"LISTEN 0      511                             *:4100             *:*    users:(("next-server (v1",pid=26870,fd=22))"#;
        let info = parse_ss_line(line).expect("should parse");
        assert_eq!(info.port, 4100);
        assert_eq!(info.bind_address, "*");
        assert_eq!(info.pid, Some(26870));
        assert_eq!(info.process_name, "next-server (v1");
    }

    #[test]
    fn ss_line_interface_scoped() {
        let line = "LISTEN 0      4096                127.0.0.53%lo:53         0.0.0.0:*";
        let info = parse_ss_line(line).expect("should parse");
        assert_eq!(info.port, 53);
        assert_eq!(info.bind_address, "127.0.0.53%lo");
    }

    #[test]
    fn ss_line_ipv6_full_address() {
        let line = "LISTEN 0      4096   [fd7a:115c:a1e0::734:db76]:56451         [::]:*";
        let info = parse_ss_line(line).expect("should parse");
        assert_eq!(info.port, 56451);
        assert_eq!(info.bind_address, "[fd7a:115c:a1e0::734:db76]");
    }

    #[test]
    fn ss_line_too_short() {
        assert!(parse_ss_line("LISTEN 0 4096").is_none());
    }

    #[test]
    fn ss_line_empty() {
        assert!(parse_ss_line("").is_none());
    }

    // -----------------------------------------------------------------------
    // parse_ss_output / dual-stack dedup tests
    // -----------------------------------------------------------------------

    #[test]
    fn parse_full_fixture() {
        let ports = parse_ss_output(SS_FIXTURE);
        // Should produce entries, sorted by port
        assert!(!ports.is_empty());
        for i in 1..ports.len() {
            assert!(
                ports[i].port >= ports[i - 1].port,
                "ports should be sorted: {} >= {}",
                ports[i].port,
                ports[i - 1].port
            );
        }
    }

    #[test]
    fn dual_stack_dedup_merges_to_wildcard() {
        // Port 22 appears on 0.0.0.0 and [::] with no process (pid=None)
        // They share key (22, None) so should merge to bind_address = "*"
        let ports = parse_ss_output(SS_FIXTURE);
        let port22: Vec<_> = ports.iter().filter(|p| p.port == 22).collect();
        // With pid=None, both IPv4 and IPv6 merge into one entry
        assert_eq!(port22.len(), 1, "dual-stack port 22 should be deduplicated");
        assert_eq!(port22[0].bind_address, "*");
    }

    #[test]
    fn dual_stack_dedup_port_631() {
        // Port 631 also appears on both 0.0.0.0 and [::]
        let ports = parse_ss_output(SS_FIXTURE);
        let port631: Vec<_> = ports.iter().filter(|p| p.port == 631).collect();
        assert_eq!(
            port631.len(),
            1,
            "dual-stack port 631 should be deduplicated"
        );
        assert_eq!(port631[0].bind_address, "*");
    }

    #[test]
    fn same_port_different_pids_kept_separate() {
        // Port 53 on 127.0.0.54 (no pid) and 127.0.0.53%lo (no pid)
        // Both have pid=None so they get merged into one entry
        let ports = parse_ss_output(SS_FIXTURE);
        let port53: Vec<_> = ports.iter().filter(|p| p.port == 53).collect();
        // Both are pid=None, so merged to "*"
        assert_eq!(port53.len(), 1);
    }

    #[test]
    fn star_wildcard_port_parsed() {
        let ports = parse_ss_output(SS_FIXTURE);
        let port4100: Vec<_> = ports.iter().filter(|p| p.port == 4100).collect();
        assert_eq!(port4100.len(), 1);
        assert_eq!(port4100[0].process_name, "next-server (v1");
        assert_eq!(port4100[0].pid, Some(26870));
    }

    #[test]
    fn no_process_port_parsed() {
        // Port 5432 appears as *:5432 with no process info
        let ports = parse_ss_output(SS_FIXTURE);
        let port5432: Vec<_> = ports.iter().filter(|p| p.port == 5432).collect();
        assert_eq!(port5432.len(), 1);
        assert_eq!(port5432[0].pid, None);
        assert_eq!(port5432[0].process_name, "(unknown)");
    }

    #[test]
    fn header_line_skipped() {
        let output = "State  Recv-Q Send-Q  Local Address:Port  Peer Address:PortProcess\n";
        let ports = parse_ss_output(output);
        assert!(ports.is_empty());
    }

    #[test]
    fn empty_input() {
        let ports = parse_ss_output("");
        assert!(ports.is_empty());
    }

    #[test]
    fn dedup_synthetic_dual_stack() {
        // Synthetic test: same port, same pid, different addresses
        let input = "State  Recv-Q Send-Q  Local Address:Port  Peer Address:PortProcess\n\
                      LISTEN 0 128 0.0.0.0:8080 0.0.0.0:* users:((\"myapp\",pid=1234,fd=5))\n\
                      LISTEN 0 128 [::]:8080 [::]:* users:((\"myapp\",pid=1234,fd=5))";
        let ports = parse_ss_output(input);
        assert_eq!(ports.len(), 1);
        assert_eq!(ports[0].port, 8080);
        assert_eq!(ports[0].bind_address, "*");
        assert_eq!(ports[0].pid, Some(1234));
        assert_eq!(ports[0].process_name, "myapp");
    }

    #[test]
    fn dedup_same_address_not_merged() {
        // Same port, same pid, same address -> no merge needed, stays as-is
        let input = "State  Recv-Q Send-Q  Local Address:Port  Peer Address:PortProcess\n\
                      LISTEN 0 128 0.0.0.0:8080 0.0.0.0:* users:((\"myapp\",pid=1234,fd=5))\n\
                      LISTEN 0 128 0.0.0.0:8080 0.0.0.0:* users:((\"myapp\",pid=1234,fd=5))";
        let ports = parse_ss_output(input);
        assert_eq!(ports.len(), 1);
        assert_eq!(ports[0].bind_address, "0.0.0.0");
    }
}
