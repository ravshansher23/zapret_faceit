use std::path::Path;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Strategy {
    Fake,
    Alt,
}

const GAME_UDP: &str = "27000-28100";
const STEAM_TCP: &str = "27015-27050";

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
    let game_fake = file("stun.bin");
    let quic_repeats = match strategy {
        Strategy::Fake => "6",
        Strategy::Alt => "11",
    };

    let extra_udp = udp_outside_game(udp);
    let wf_udp = if extra_udp.is_empty() {
        format!("443,{GAME_UDP}")
    } else {
        format!("443,{GAME_UDP},{extra_udp}")
    };
    let mut args = vec![
        format!("--wf-tcp=80,443,{STEAM_TCP}"),
        format!("--wf-udp={wf_udp}"),
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

    args.extend(steam_tcp_profile(&tls, &exclude, strategy));
    args.push("--new".to_string());
    args.extend(game_udp_profile(GAME_UDP, None, &exclude, &game_fake, strategy));
    if !extra_udp.is_empty() {
        args.push("--new".to_string());
        args.extend(game_udp_profile(
            &extra_udp,
            Some(ipset.as_str()),
            &exclude,
            &game_fake,
            strategy,
        ));
    }
    args
}

fn steam_tcp_profile(tls: &str, exclude: &str, strategy: Strategy) -> Vec<String> {
    let mut profile = vec![
        format!("--filter-tcp={STEAM_TCP}"),
        format!("--ipset-exclude={exclude}"),
        "--dpi-desync-any-protocol=1".to_string(),
        "--dpi-desync-cutoff=n4".to_string(),
    ];
    match strategy {
        Strategy::Fake => profile.extend([
            "--dpi-desync=fake,fakedsplit".to_string(),
            "--dpi-desync-repeats=6".to_string(),
            "--dpi-desync-fooling=ts".to_string(),
            "--dpi-desync-fakedsplit-pattern=0x00".to_string(),
            format!("--dpi-desync-fake-tls={tls}"),
            format!("--dpi-desync-fake-http={tls}"),
        ]),
        Strategy::Alt => profile.extend([
            "--dpi-desync=multisplit".to_string(),
            "--dpi-desync-split-seqovl=652".to_string(),
            "--dpi-desync-split-pos=2".to_string(),
            format!("--dpi-desync-split-seqovl-pattern={tls}"),
        ]),
    }
    profile
}

fn game_udp_profile(
    ports: &str,
    ipset: Option<&str>,
    exclude: &str,
    game_fake: &str,
    strategy: Strategy,
) -> Vec<String> {
    let mut profile = vec![format!("--filter-udp={ports}")];
    if let Some(ipset) = ipset {
        profile.push(format!("--ipset={ipset}"));
    }
    profile.push(format!("--ipset-exclude={exclude}"));
    profile.push("--dpi-desync-any-protocol=1".to_string());
    match strategy {
        Strategy::Fake => profile.extend([
            "--dpi-desync=ipfrag2".to_string(),
            "--dpi-desync-ipfrag-pos-udp=8".to_string(),
        ]),
        Strategy::Alt => profile.extend([
            "--dpi-desync=fake".to_string(),
            "--dpi-desync-repeats=1".to_string(),
            format!("--dpi-desync-fake-unknown-udp={game_fake}"),
            "--dpi-desync-ttl=8".to_string(),
            "--dpi-desync-autottl=1:3-16".to_string(),
        ]),
    }
    profile
}

fn udp_outside_game(spec: &str) -> String {
    spec.split(',')
        .map(str::trim)
        .filter(|part| !part.is_empty() && !udp_part_inside_game(part))
        .collect::<Vec<_>>()
        .join(",")
}

fn udp_part_inside_game(part: &str) -> bool {
    let (start, end) = match part.split_once('-') {
        Some((start, end)) => (start, end),
        None => (part, part),
    };
    let (Ok(start), Ok(end)) = (start.parse::<u16>(), end.parse::<u16>()) else {
        return false;
    };
    start <= end && (27000..=28100).contains(&start) && (27000..=28100).contains(&end)
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
        "stun.bin",
    ] {
        if !bin.join(name).is_file() {
            missing.push(format!("bin/{name}"));
        }
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

    #[test]
    fn fake_strategy_targets_saved_port_and_ipset() {
        let root = Path::new("/tmp/faceit-root");
        let args = winws_arguments(root, "27015", Strategy::Fake);
        let joined = args.join("\n");
        assert!(joined.contains("--wf-udp=443,27000-28100"));
        assert!(joined.contains("--filter-udp=27000-28100"));
        assert!(joined.contains("--filter-tcp=27015-27050"));
        assert!(joined.contains("--dpi-desync=fake,fakedsplit"));
        assert!(!joined.contains(&root.join("lists/ipset-faceit-user.txt").display().to_string()));
        assert!(joined.contains(&root.join("lists/list-faceit.txt").display().to_string()));
        assert!(!joined.contains("0.0.0.0/0"));
        assert!(joined.contains("--dpi-desync=ipfrag2"));
        assert!(joined.contains("--dpi-desync-ipfrag-pos-udp=8"));
        assert!(!joined.contains("badsum"));
    }

    #[test]
    fn unusual_port_stays_limited_to_saved_ip() {
        let root = Path::new("/tmp/faceit-root");
        let joined = winws_arguments(root, "7777", Strategy::Fake).join("\n");
        assert!(joined.contains("--filter-udp=27000-28100"));
        assert!(joined.contains("--filter-udp=7777"));
        assert!(joined.contains(&root.join("lists/ipset-faceit-user.txt").display().to_string()));
    }

    #[test]
    fn alt_strategy_uses_multisplit() {
        let root = Path::new("/tmp/faceit-root");
        let joined = winws_arguments(root, "27016,27030", Strategy::Alt).join("\n");
        assert!(joined.contains("--dpi-desync=multisplit"));
        assert!(joined.contains("--filter-udp=27000-28100"));
        assert!(!joined.contains("ipset-faceit-user.txt"));
        assert!(joined.contains("--dpi-desync-repeats=11"));
        assert!(!joined.contains("fakedsplit"));
        assert!(joined.contains("--dpi-desync-repeats=1"));
        assert!(joined.contains("stun.bin"));
        assert!(joined.contains("--dpi-desync-autottl=1:3-16"));
        assert!(!joined.contains("ipfrag2"));
    }
}
