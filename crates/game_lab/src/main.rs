use std::{
    error::Error,
    io::{self, BufRead, Write},
};

use ultimate_fate_lab::{
    DEFAULT_SEED, LabCommand, LabSession, error_json, help_json, parse_command,
};

fn main() -> Result<(), Box<dyn Error>> {
    let arguments = std::env::args().collect::<Vec<_>>();
    if arguments
        .get(1)
        .is_some_and(|argument| argument == "--connect")
    {
        let path = arguments
            .get(2)
            .ok_or("--connect requires a Unix socket path")?;
        return connect(path);
    }
    let mut session = LabSession::new(DEFAULT_SEED)?;
    println!("{}", help_json());
    io::stdout().flush()?;
    for line in io::stdin().lock().lines() {
        let line = line?;
        let command = match parse_command(&line) {
            Ok(command) => command,
            Err(error) => {
                println!("{}", error_json(&error));
                io::stdout().flush()?;
                continue;
            }
        };
        let quit = command == LabCommand::Quit;
        println!("{}", session.execute(command));
        io::stdout().flush()?;
        if quit {
            break;
        }
    }
    Ok(())
}

#[cfg(unix)]
fn connect(path: &str) -> Result<(), Box<dyn Error>> {
    use std::os::unix::net::UnixStream;

    let mut stream = UnixStream::connect(path)?;
    let reader_stream = stream.try_clone()?;
    let mut responses = io::BufReader::new(reader_stream);
    for line in io::stdin().lock().lines() {
        let line = line?;
        writeln!(stream, "{line}")?;
        stream.flush()?;
        let mut response = String::new();
        if responses.read_line(&mut response)? == 0 {
            break;
        }
        print!("{response}");
        io::stdout().flush()?;
        if matches!(parse_command(&line), Ok(LabCommand::Quit)) {
            break;
        }
    }
    Ok(())
}

#[cfg(not(unix))]
fn connect(_path: &str) -> Result<(), Box<dyn Error>> {
    Err("live desktop bridge requires a Unix platform".into())
}
