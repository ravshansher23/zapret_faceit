use std::path::Path;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Strategy {
    Fake,
    Alt,
}

#[cfg_attr(not(windows), allow(dead_code))]
pub fn winws_arguments(root: &Path, udp: &str, strategy: Strategy) -> Vec<String> {
    let bin = root.join("bin");
    let lists = root.join("lists");
    let file = |name: &str| display(&bin.join(name));
    let host = display(&lists.join("list-faceit.txt"));
    let ipset = display(&lists.join("ipset-faceit-user.txt"));
    let exclude = display(&lists.join("ipset-exclude.txt"));
    let quic = file("quic_initial_www_google_com.bin");
    let tls = file("tls_clienthello_www_google_com.bin");
    let game_fake = if bin.join("ACTIVE_GAME_UDP.bin").is_file() {
        file("ACTIVE_GAME_UDP.bin")
    } else {
        file("stun.bin")
    };
    let quic_repeats = match strategy {
        Strategy::Fake => "6",
        Strategy::Alt => "11",
    };

    let mut args = vec![
        "--wf-tcp=80,443".to_string(),
        format!("--wf-udp=443,{udp}"),
        "--filter-udp=443".to_string(),
        format!("--hostlist={host}"),
        "--dpi-desync=fake".to_string(),
        format!("--dpi-desync-repeats={quic_repeats}"),
        format!("--dpi-desync-fake-quic={quic}"),
        "--new".to_string(),
    ];

    match strategy {
        Strategy::Fake => args.extend([
            "--filter-tcp=80,443".to_string(),
            format!("--hostlist={host}"),
            "--dpi-desync=fake,fakedsplit".to_string(),
            "--dpi-desync-repeats=6".to_string(),
            "--dpi-desync-fooling=ts".to_string(),
            "--dpi-desync-fakedsplit-pattern=0x00".to_string(),
            format!("--dpi-desync-fake-tls={tls}"),
            format!("--dpi-desync-fake-http={tls}"),
            "--new".to_string(),
        ]),
        Strategy::Alt => args.extend([
            "--filter-tcp=80,443".to_string(),
            format!("--hostlist={host}"),
            "--dpi-desync=multisplit".to_string(),
            "--dpi-desync-split-seqovl=652".to_string(),
            "--dpi-desync-split-pos=2".to_string(),
            format!("--dpi-desync-split-seqovl-pattern={tls}"),
            "--new".to_string(),
        ]),
    }

    let mut game = vec![
        format!("--filter-udp={udp}"),
        format!("--ipset={ipset}"),
        format!("--ipset-exclude={exclude}"),
        "--dpi-desync=fake".to_string(),
        "--dpi-desync-repeats=6".to_string(),
        "--dpi-desync-any-protocol=1".to_string(),
        format!("--dpi-desync-fake-unknown-udp={game_fake}"),
        "--dpi-desync-cutoff=n2".to_string(),
    ];
    match strategy {
        Strategy::Fake => game.push("--dpi-desync-fooling=badsum".to_string()),
        Strategy::Alt => game.push("--dpi-desync-ttl=3".to_string()),
    }
    args.extend(game);
    args
}

pub fn missing_files(root: &Path) -> Vec<String> {
    let bin = root.join("bin");
    let lists = root.join("lists");
    let mut missing = Vec::new();
    for name in [
        "winws.exe",
        "WinDivert.dll",
        "WinDivert64.sys",
        "cygwin1.dll",
        "quic_initial_www_google_com.bin",
        "tls_clienthello_www_google_com.bin",
    ] {
        if !bin.join(name).is_file() {
            missing.push(format!("bin/{name}"));
        }
    }
    if !bin.join("ACTIVE_GAME_UDP.bin").is_file() && !bin.join("stun.bin").is_file() {
        missing.push("bin/ACTIVE_GAME_UDP.bin".to_string());
    }
    for name in ["list-faceit.txt", "ipset-exclude.txt"] {
        if !lists.join(name).is_file() {
            missing.push(format!("lists/{name}"));
        }
    }
    missing
}

#[cfg_attr(not(windows), allow(dead_code))]
fn display(path: &Path) -> String {
    path.display().to_string()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    #[test]
    fn fake_strategy_targets_saved_port_and_ipset() {
        let root = Path::new("/tmp/faceit-root");
        let args = winws_arguments(root, "27015", Strategy::Fake);
        let joined = args.join("\n");
        assert!(joined.contains("--wf-udp=443,27015"));
        assert!(joined.contains("--filter-udp=27015"));
        assert!(joined.contains("--dpi-desync=fake,fakedsplit"));
        assert!(joined.contains(&root.join("lists/ipset-faceit-user.txt").display().to_string()));
        assert!(joined.contains(&root.join("lists/list-faceit.txt").display().to_string()));
        assert!(!joined.contains("0.0.0.0/0"));
        assert!(joined.contains("stun.bin"));
        assert!(joined.contains("--dpi-desync-fooling=badsum"));
    }

    #[test]
    fn alt_strategy_uses_multisplit() {
        let root = Path::new("/tmp/faceit-root");
        let joined = winws_arguments(root, "27016,27030", Strategy::Alt).join("\n");
        assert!(joined.contains("--dpi-desync=multisplit"));
        assert!(joined.contains("--filter-udp=27016,27030"));
        assert!(joined.contains("--dpi-desync-repeats=11"));
        assert!(!joined.contains("fakedsplit"));
        assert!(joined.contains("--dpi-desync-ttl=3"));
    }

    #[test]
    fn prefers_active_game_fake_when_present() {
        let root = std::env::temp_dir().join("faceit-bypass-fake-bin");
        let _ = fs::remove_dir_all(&root);
        fs::create_dir_all(root.join("bin")).unwrap();
        fs::write(root.join("bin").join("ACTIVE_GAME_UDP.bin"), b"x").unwrap();
        let joined = winws_arguments(&root, "27015", Strategy::Fake).join("\n");
        assert!(joined.contains("ACTIVE_GAME_UDP.bin"));
        let _ = fs::remove_dir_all(root);
    }
}
