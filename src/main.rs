fn main() {
    faceit_bypass::platform::prepare_console();
    match faceit_bypass::run() {
        Ok(faceit_bypass::Finish::Exit) => {}
        Ok(faceit_bypass::Finish::Stay) => faceit_bypass::platform::pause(),
        Err(err) => {
            eprintln!("{err}");
            faceit_bypass::platform::pause();
            std::process::exit(1);
        }
    }
}
