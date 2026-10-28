use std::fs;
use std::net::Ipv4Addr;
use std::path::{Path, PathBuf};

pub const WIDE_UDP: &str = "1024-65535";
const MAX_SERVERS: usize = 20;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Server {
    pub ip: Ipv4Addr,
    pub port: Option<u16>,
}

impl Server {
    pub fn label(&self) -> String {
        match self.port {
            Some(port) => format!("{}:{port}", self.ip),
            None => format!("{} (UDP {WIDE_UDP})", self.ip),
        }
    }
}

pub fn parse_server(text: &str) -> Result<Server, String> {
    let text = text.trim().trim_start_matches('\u{feff}');
    let line = text.lines().next().unwrap_or("").trim();
    let line = line.trim_matches(|ch| ch == '"' || ch == '\'');
    let body = strip_wrapper(line);
    let (ip, port, rest) = split_endpoint(body)?;
    if !rest.is_empty() && !rest.starts_with(';') && !rest.starts_with(|ch: char| ch.is_whitespace()) {
        return Err("ожидается IP:PORT или connect IP:PORT".into());
    }
    public_server(ip, port)
}

/// Последний `connect IP:PORT` в логе или командной строке.
pub fn parse_loose(text: &str) -> Option<Server> {
    let mut found = None;
    for (index, _) in text.char_indices() {
        let tail = &text[index..];
        if tail.len() < 7 || !tail.as_bytes()[..7].eq_ignore_ascii_case(b"connect") {
            continue;
        }
        if !boundary_before(text, index) {
            continue;
        }
        let after = tail[7..].trim_start();
        let Ok((ip, port, _)) = split_endpoint(after) else {
            continue;
        };
        let Ok(server) = public_server(ip, port) else {
            continue;
        };
        found = Some(server);
    }
    found
}

pub fn port_spec(servers: &[Server]) -> Result<String, String> {
    if servers.is_empty() {
        return Err("нет сохранённых серверов".into());
    }
    if servers.iter().any(|server| server.port.is_none()) {
        return Ok(WIDE_UDP.to_string());
    }
    let mut ports: Vec<u16> = servers.iter().filter_map(|server| server.port).collect();
    ports.sort_unstable();
    ports.dedup();
    if !valid_port_spec_ports(&ports) {
        return Err("некорректный список портов".into());
    }
    let mut parts = Vec::new();
    let covers_cs = ports.iter().any(|port| (27000..=27200).contains(port));
    if covers_cs {
        parts.push("27000-27200".to_string());
    }
    for port in ports {
        if !(27000..=27200).contains(&port) {
            parts.push(port.to_string());
        }
    }
    Ok(parts.join(","))
}

pub fn valid_port_spec(spec: &str) -> bool {
    !spec.is_empty()
        && spec.bytes().all(|byte| byte.is_ascii_digit() || byte == b',' || byte == b'-')
        && !spec.contains(",,")
        && !spec.starts_with('-')
        && !spec.ends_with('-')
}

#[cfg_attr(not(windows), allow(dead_code))]
pub fn steam_library_paths(vdf: &str) -> Vec<String> {
    let mut paths = Vec::new();
    let bytes = vdf.as_bytes();
    let needle = b"\"path\"";
    let mut index = 0;
    while index + needle.len() < bytes.len() {
        if bytes[index..].starts_with(needle) {
            let rest = vdf[index + needle.len()..].trim_start();
            if let Some(rest) = rest.strip_prefix('"') {
                if let Some(end) = rest.find('"') {
                    paths.push(rest[..end].replace("\\\\", "\\"));
                    index += needle.len() + end;
                    continue;
                }
            }
        }
        index += 1;
    }
    paths
}

pub struct Store {
    pub root: PathBuf,
}

impl Store {
    pub fn open() -> Result<Self, String> {
        let root = find_root();
        if !root.join("lists").join("list-faceit.txt").is_file() {
            return Err("рядом с программой нет lists/list-faceit.txt".into());
        }
        Ok(Self { root })
    }

    #[cfg_attr(not(test), allow(dead_code))]
    pub fn with_root(root: impl Into<PathBuf>) -> Self {
        Self { root: root.into() }
    }

    pub fn load(&self) -> Result<Vec<Server>, String> {
        let path = self.servers_path();
        if !path.exists() {
            return Ok(Vec::new());
        }
        let text = fs::read_to_string(&path).map_err(|err| err.to_string())?;
        let mut servers = Vec::new();
        for raw in text.lines() {
            let line = raw.trim();
            if line.is_empty() || line.starts_with('#') {
                continue;
            }
            servers.push(parse_stored(line)?);
        }
        Ok(servers)
    }

    pub fn add(&self, server: Server) -> Result<Vec<Server>, String> {
        let mut servers: Vec<Server> = self
            .load()?
            .into_iter()
            .filter(|item| item.ip != server.ip)
            .collect();
        servers.push(server);
        if servers.len() > MAX_SERVERS {
            servers = servers.split_off(servers.len() - MAX_SERVERS);
        }
        self.write(&servers)?;
        Ok(servers)
    }

    pub fn remove(&self, ip: Ipv4Addr) -> Result<Vec<Server>, String> {
        let servers: Vec<Server> = self.load()?.into_iter().filter(|item| item.ip != ip).collect();
        self.write(&servers)?;
        Ok(servers)
    }

    pub fn clear(&self) -> Result<(), String> {
        self.write(&[])
    }

    pub fn write(&self, servers: &[Server]) -> Result<(), String> {
        let lists = self.root.join("lists");
        fs::create_dir_all(&lists).map_err(|err| err.to_string())?;
        let body = servers
            .iter()
            .map(|server| {
                let port = server.port.map(|port| port.to_string()).unwrap_or_else(|| "*".into());
                format!("{} {port}", server.ip)
            })
            .collect::<Vec<_>>()
            .join("\n");
        let servers_body = if body.is_empty() { String::new() } else { format!("{body}\n") };
        let ipset = servers
            .iter()
            .map(|server| format!("{}/32", server.ip))
            .collect::<Vec<_>>()
            .join("\n");
        let ipset_body = if ipset.is_empty() { String::new() } else { format!("{ipset}\n") };
        fs::write(self.servers_path(), servers_body).map_err(|err| err.to_string())?;
        fs::write(self.ipset_path(), ipset_body).map_err(|err| err.to_string())?;
        Ok(())
    }

    pub fn servers_path(&self) -> PathBuf {
        self.root.join("lists").join("servers.txt")
    }

    pub fn ipset_path(&self) -> PathBuf {
        self.root.join("lists").join("ipset-faceit-user.txt")
    }
}

pub fn find_root() -> PathBuf {
    let mut candidates = Vec::new();
    if let Ok(exe) = std::env::current_exe() {
        let mut dir = exe.parent().map(Path::to_path_buf);
        for _ in 0..4 {
            if let Some(current) = dir {
                candidates.push(current.clone());
                dir = current.parent().map(Path::to_path_buf);
            }
        }
    }
    if let Ok(cwd) = std::env::current_dir() {
        candidates.push(cwd);
    }
    for candidate in candidates {
        if candidate.join("lists").join("list-faceit.txt").is_file() {
            return candidate;
        }
    }
    PathBuf::from(".")
}

fn parse_stored(line: &str) -> Result<Server, String> {
    let mut parts = line.split_whitespace();
    let ip_text = parts.next().ok_or_else(|| format!("битая строка: {line}"))?;
    let port_text = parts.next().unwrap_or("*");
    if parts.next().is_some() {
        return Err(format!("битая строка: {line}"));
    }
    let ip = parse_ipv4(ip_text).ok_or_else(|| format!("битый IP: {line}"))?;
    let port = if port_text == "*" {
        None
    } else {
        let port: u16 = port_text
            .parse()
            .map_err(|_| format!("битый порт: {line}"))?;
        if port == 0 {
            return Err(format!("битый порт: {line}"));
        }
        Some(port)
    };
    public_server(ip, port)
}

fn strip_wrapper(line: &str) -> &str {
    if let Some(rest) = strip_prefix_ignore_ascii_case(line, "steam://connect/") {
        return rest.trim_start();
    }
    if let Some(rest) = strip_prefix_ignore_ascii_case(line, "connect") {
        if rest.is_empty() || rest.starts_with(|ch: char| ch.is_whitespace()) {
            return rest.trim_start();
        }
    }
    line
}

fn split_endpoint(text: &str) -> Result<(Ipv4Addr, Option<u16>, &str), String> {
    let text = text.trim_start();
    let (ip, after_ip) = take_ipv4(text).ok_or_else(|| "ожидается IP:PORT или connect IP:PORT".to_string())?;
    let after_ip = after_ip.trim_start();
    if let Some(rest) = after_ip.strip_prefix(':') {
        let rest = rest.trim_start();
        let port_len = rest.bytes().take_while(|byte| byte.is_ascii_digit()).count();
        if port_len == 0 {
            return Err("после двоеточия нужен порт".into());
        }
        let port: u32 = rest[..port_len].parse().map_err(|_| "некорректный порт".to_string())?;
        if !(1..=65535).contains(&port) {
            return Err("порт должен быть от 1 до 65535".into());
        }
        Ok((ip, Some(port as u16), &rest[port_len..]))
    } else {
        Ok((ip, None, after_ip))
    }
}

fn take_ipv4(text: &str) -> Option<(Ipv4Addr, &str)> {
    let end = text
        .char_indices()
        .find(|(_, ch)| *ch != '.' && !ch.is_ascii_digit())
        .map(|(index, _)| index)
        .unwrap_or(text.len());
    let ip = parse_ipv4(&text[..end])?;
    Some((ip, &text[end..]))
}

fn parse_ipv4(text: &str) -> Option<Ipv4Addr> {
    let parts: Vec<&str> = text.split('.').collect();
    if parts.len() != 4 {
        return None;
    }
    let mut octets = [0u8; 4];
    for (index, part) in parts.iter().enumerate() {
        if part.is_empty() || part.len() > 3 || !part.bytes().all(|byte| byte.is_ascii_digit()) {
            return None;
        }
        let value: u16 = part.parse().ok()?;
        if value > 255 {
            return None;
        }
        octets[index] = value as u8;
    }
    Some(Ipv4Addr::new(octets[0], octets[1], octets[2], octets[3]))
}

fn public_server(ip: Ipv4Addr, port: Option<u16>) -> Result<Server, String> {
    if !is_public_ipv4(ip) {
        return Err(format!("нужен публичный IPv4 игрового сервера, получен {ip}"));
    }
    Ok(Server { ip, port })
}

fn is_public_ipv4(ip: Ipv4Addr) -> bool {
    let [a, b, c, _] = ip.octets();
    match a {
        0 | 10 | 127 => false,
        100 if (64..128).contains(&b) => false,
        169 if b == 254 => false,
        172 if (16..=31).contains(&b) => false,
        192 if b == 168 || (b == 0 && c == 2) => false,
        198 if (b == 18) || (b == 19) || (b == 51 && c == 100) => false,
        203 if b == 0 && c == 113 => false,
        value if value >= 224 => false,
        _ => true,
    }
}

fn boundary_before(text: &str, index: usize) -> bool {
    match text[..index].chars().next_back() {
        None => true,
        Some(ch) => !ch.is_ascii_alphanumeric(),
    }
}

fn strip_prefix_ignore_ascii_case<'a>(text: &'a str, prefix: &str) -> Option<&'a str> {
    let size = prefix.len();
    if text.len() >= size && text.as_bytes()[..size].eq_ignore_ascii_case(prefix.as_bytes()) {
        Some(&text[size..])
    } else {
        None
    }
}

fn valid_port_spec_ports(ports: &[u16]) -> bool {
    ports.iter().all(|port| *port >= 1)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::{SystemTime, UNIX_EPOCH};

    #[test]
    fn parses_match_room_strings() {
        assert_eq!(
            parse_server("1.2.3.4:27015").unwrap(),
            Server { ip: Ipv4Addr::new(1, 2, 3, 4), port: Some(27015) }
        );
        assert_eq!(
            parse_server("connect 1.2.3.4:27015").unwrap().port,
            Some(27015)
        );
        assert_eq!(
            parse_server("steam://connect/1.2.3.4:27015").unwrap(),
            Server { ip: Ipv4Addr::new(1, 2, 3, 4), port: Some(27015) }
        );
        assert_eq!(
            parse_server("  connect   001.002.003.004 : 27015 \nхвост").unwrap(),
            Server { ip: Ipv4Addr::new(1, 2, 3, 4), port: Some(27015) }
        );
        assert_eq!(parse_server("1.2.3.4").unwrap().port, None);
        assert_eq!(
            parse_server("1.2.3.4:27015; password").unwrap().port,
            Some(27015)
        );
    }

    #[test]
    fn rejects_private_and_garbage() {
        assert!(parse_server("192.168.0.10:27015").is_err());
        assert!(parse_server("10.1.2.3:27015").is_err());
        assert!(parse_server("100.64.1.1:27015").is_err());
        assert!(parse_server("203.0.113.5:27015").is_err());
        assert!(parse_server("127.0.0.1:27015").is_err());
        assert!(parse_server("сервер ещё не готов").is_err());
        assert!(parse_server("1.2.3.4:0").is_err());
        assert!(parse_server("1.2.3.4:70000").is_err());
    }

    #[test]
    fn loose_parse_skips_disconnect_and_keeps_last() {
        let text = "disconnect 8.8.8.8:27015\n] connect 1.2.3.4:27030\nconnect 9.9.9.9:27015\n";
        assert_eq!(
            parse_loose(text),
            Some(Server { ip: Ipv4Addr::new(9, 9, 9, 9), port: Some(27015) })
        );
        assert_eq!(
            parse_loose("+connect 1.2.3.4:27015").unwrap().port,
            Some(27015)
        );
    }

    #[test]
    fn store_rewrites_ipset_for_same_ip() {
        let root = temp_root("store");
        let store = Store::with_root(&root);
        store.add(Server { ip: Ipv4Addr::new(1, 2, 3, 4), port: Some(27015) }).unwrap();
        store.add(Server { ip: Ipv4Addr::new(8, 8, 4, 4), port: Some(27030) }).unwrap();
        store.add(Server { ip: Ipv4Addr::new(1, 2, 3, 4), port: Some(27016) }).unwrap();
        let text = fs::read_to_string(store.ipset_path()).unwrap();
        assert_eq!(text, "8.8.4.4/32\n1.2.3.4/32\n");
        assert_eq!(port_spec(&store.load().unwrap()).unwrap(), "27000-27200");
        store.add(Server { ip: Ipv4Addr::new(1, 1, 1, 1), port: None }).unwrap();
        assert_eq!(port_spec(&store.load().unwrap()).unwrap(), WIDE_UDP);
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn reads_libraryfolders_paths() {
        let vdf = "\"libraryfolders\"\n{\n\t\"0\"\n\t{\n\t\t\"path\"\t\t\"C:\\\\Program Files (x86)\\\\Steam\"\n\t}\n}\n";
        assert_eq!(
            steam_library_paths(vdf),
            vec!["C:\\Program Files (x86)\\Steam".to_string()]
        );
    }

    fn temp_root(name: &str) -> PathBuf {
        let nanos = SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_nanos();
        let root = std::env::temp_dir().join(format!("faceit-bypass-{name}-{nanos}"));
        fs::create_dir_all(root.join("lists")).unwrap();
        root
    }
}
