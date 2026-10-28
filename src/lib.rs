pub mod platform;
mod server;
mod strategy;

use std::io::{self, Write};

use server::{parse_loose, parse_server, port_spec, valid_port_spec, Server, Store};
use strategy::{missing_files, Strategy};
#[cfg(windows)]
use strategy::winws_arguments;

pub enum Finish {
    Stay,
    Exit,
}

pub fn run() -> Result<Finish, String> {
    let args: Vec<String> = std::env::args().skip(1).collect();
    if args.is_empty() {
        return default_flow(Strategy::Fake);
    }
    match args[0].to_ascii_lowercase().as_str() {
        "help" | "-h" | "--help" => {
            print_help();
            Ok(Finish::Stay)
        }
        "list" => {
            print_servers()?;
            Ok(Finish::Stay)
        }
        "clear" => {
            Store::open()?.clear()?;
            println!("список очищен");
            Ok(Finish::Stay)
        }
        "remove" => {
            let ip = args.get(1).ok_or("укажите IP")?;
            let server = parse_server(ip)?;
            let left = Store::open()?.remove(server.ip)?;
            println!("серверов: {}", left.len());
            Ok(Finish::Stay)
        }
        "stop" => stop(),
        "start" => start(strategy_of(args.get(1).map(String::as_str))),
        "clip" => {
            let text = platform::read_clipboard()?;
            add_and_start(&text, Strategy::Fake)
        }
        "discover" => {
            let (server, source) = discover_live()?;
            Store::open()?.add(server.clone())?;
            println!("{source}: {}", server.label());
            start(Strategy::Fake)
        }
        "add" => {
            let text = join_rest(&args);
            if text.is_empty() {
                return Err("вставьте строку сервера".into());
            }
            add_and_start(&text, Strategy::Fake)
        }
        "alt" => {
            let text = join_rest(&args);
            if text.is_empty() {
                default_flow(Strategy::Alt)
            } else {
                add_and_start(&text, Strategy::Alt)
            }
        }
        _ => add_and_start(&args.join(" "), Strategy::Fake),
    }
}

fn default_flow(strategy: Strategy) -> Result<Finish, String> {
    if let Ok(text) = platform::read_clipboard() {
        if let Ok(server) = parse_server(&text) {
            println!("буфер обмена: {}", server.label());
            Store::open()?.add(server)?;
            return start(strategy);
        }
    }
    println!("Скопируйте IP в комнате матча FACEIT и вставьте сюда.");
    println!("Пример: 185.1.2.3:27015");
    print!("> ");
    io::stdout().flush().ok();
    let mut line = String::new();
    io::stdin().read_line(&mut line).map_err(|err| err.to_string())?;
    if line.trim().is_empty() {
        let (server, source) = discover_live()?;
        println!("{source}: {}", server.label());
        Store::open()?.add(server)?;
        return start(strategy);
    }
    add_and_start(&line, strategy)
}

fn add_and_start(text: &str, strategy: Strategy) -> Result<Finish, String> {
    let server = parse_server(text)?;
    Store::open()?.add(server.clone())?;
    println!("{}", server.label());
    start(strategy)
}

fn stop() -> Result<Finish, String> {
    #[cfg(windows)]
    if !platform::is_elevated() {
        platform::relaunch_elevated(&["stop".to_string()])?;
        println!("Подтвердите запрос администратора.");
        return Ok(Finish::Exit);
    }
    let stopped = platform::stop_winws()?;
    println!("остановлено: {stopped}");
    Ok(Finish::Stay)
}

fn start(strategy: Strategy) -> Result<Finish, String> {
    let store = Store::open()?;
    let servers = store.load()?;
    if servers.is_empty() {
        return Err("нет IP. Вставьте строку сервера из комнаты матча".into());
    }
    store.write(&servers)?;
    let udp = port_spec(&servers)?;
    if !valid_port_spec(&udp) {
        return Err("некорректный порт сервера".into());
    }
    let missing = missing_files(&store.root);
    if !missing.is_empty() {
        return Err(format!(
            "не хватает файлов zapret:\n{}\nСкопируйте папку bin из релиза https://github.com/Flowseal/zapret-discord-youtube/releases",
            missing.join("\n")
        ));
    }

    #[cfg(not(windows))]
    {
        let _ = strategy;
        let labels = servers.iter().map(Server::label).collect::<Vec<_>>().join(", ");
        return Err(format!(
            "IP записан ({labels}). Обход запускается сборкой для Windows рядом с bin\\winws.exe"
        ));
    }

    #[cfg(windows)]
    {
        if !platform::is_elevated() {
            let mut args = vec!["start".to_string()];
            if strategy == Strategy::Alt {
                args.push("alt".to_string());
            }
            platform::relaunch_elevated(&args)?;
            println!("Подтвердите запрос администратора.");
            return Ok(Finish::Exit);
        }
        platform::exclude_from_defender(&store.root);
        let stopped = platform::stop_winws()?;
        if stopped > 0 {
            println!("предыдущий обход остановлен");
        }
        let args = winws_arguments(&store.root, &udp, strategy);
        let exe = store.root.join("bin").join("winws.exe");
        platform::spawn_detached(&exe, &args, &store.root.join("bin"))?;
        println!("обход запущен");
        for server in &servers {
            println!("{}", server.label());
        }
        println!("udp {udp}");
        let mode = match strategy {
            Strategy::Fake => "фрагменты игровых пакетов",
            Strategy::Alt => "короткая маскировка",
        };
        println!("режим: {mode}");
        println!("VPN выключите: пакеты матча должны идти напрямую, не в туннель.");
        println!("Дальше подключайтесь к матчу. Остановить обход: Стоп.bat");
        Ok(Finish::Stay)
    }
}

fn discover_live() -> Result<(Server, &'static str), String> {
    let clipboard = platform::read_clipboard().unwrap_or_default();
    let commands = platform::cs2_command_lines();
    let log = platform::console_log_text();
    discover_from(&clipboard, &commands, log.as_deref())
}

fn discover_from(
    clipboard: &str,
    commands: &[String],
    log: Option<&str>,
) -> Result<(Server, &'static str), String> {
    if let Ok(server) = parse_server(clipboard) {
        return Ok((server, "буфер обмена"));
    }
    let mut from_game = None;
    for command in commands {
        if let Some(server) = parse_loose(command) {
            from_game = Some(server);
        }
    }
    if let Some(server) = from_game {
        return Ok((server, "командная строка cs2.exe"));
    }
    if let Some(text) = log {
        if let Some(server) = parse_loose(text) {
            return Ok((server, "console.log CS2"));
        }
    }
    Err("строка сервера не найдена. Скопируйте IP в комнате матча или вставьте её вручную".into())
}

fn print_servers() -> Result<(), String> {
    let servers = Store::open()?.load()?;
    if servers.is_empty() {
        println!("список пуст");
        return Ok(());
    }
    for server in &servers {
        println!("{}", server.label());
    }
    println!("udp {}", port_spec(servers.as_slice())?);
    Ok(())
}

fn strategy_of(value: Option<&str>) -> Strategy {
    match value.map(|item| item.to_ascii_lowercase()).as_deref() {
        Some("alt") => Strategy::Alt,
        _ => Strategy::Fake,
    }
}

fn join_rest(args: &[String]) -> String {
    args.iter().skip(1).cloned().collect::<Vec<_>>().join(" ")
}

fn print_help() {
    println!(
        r#"faceit-bypass — обход для IP сервера FACEIT

  faceit-bypass 185.1.2.3:27015
  faceit-bypass connect 185.1.2.3:27015
  faceit-bypass alt 185.1.2.3:27015
  faceit-bypass clip
  faceit-bypass discover
  faceit-bypass list
  faceit-bypass remove 185.1.2.3
  faceit-bypass clear
  faceit-bypass start
  faceit-bypass start alt
  faceit-bypass stop

Без аргументов программа берёт строку из буфера обмена.
Пустой ввод ищет connect в запущенном cs2.exe и в console.log.
Для лога у CS2 в параметрах запуска нужен -condebug.
Подключаться к матчу нужно после запуска обхода."#
    );
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn discover_prefers_clipboard_over_log() {
        let (server, source) = discover_from(
            "connect 1.2.3.4:27015",
            &["+connect 8.8.8.8:27015".to_string()],
            Some("connect 9.9.9.9:27015"),
        )
        .unwrap();
        assert_eq!(source, "буфер обмена");
        assert_eq!(server.port, Some(27015));
        assert_eq!(server.ip, std::net::Ipv4Addr::new(1, 2, 3, 4));
    }

    #[test]
    fn discover_reads_cs2_then_log() {
        let (server, source) = discover_from(
            "просто текст",
            &["cs2.exe +connect 1.2.3.4:27115".to_string()],
            Some("connect 9.9.9.9:27015"),
        )
        .unwrap();
        assert_eq!(source, "командная строка cs2.exe");
        assert_eq!(server.port, Some(27115));

        let (server, source) = discover_from("нет", &[], Some("connect 9.9.9.9:27015")).unwrap();
        assert_eq!(source, "console.log CS2");
        assert_eq!(server.ip, std::net::Ipv4Addr::new(9, 9, 9, 9));
    }
}
