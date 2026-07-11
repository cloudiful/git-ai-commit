use crossterm::event::{self, Event, KeyCode, KeyEventKind, KeyModifiers};
use crossterm::terminal::{disable_raw_mode, enable_raw_mode};
use std::io::{self, IsTerminal, Write};

pub(super) fn read_confirmation_input() -> Result<String, String> {
    if !io::stdin().is_terminal() {
        return read_line();
    }

    read_key().map_err(|err| err.to_string())
}

fn read_line() -> Result<String, String> {
    let mut line = String::new();
    io::stdin()
        .read_line(&mut line)
        .map_err(|err| err.to_string())?;
    Ok(line)
}

fn read_key() -> io::Result<String> {
    let _guard = RawModeGuard::enable()?;

    loop {
        let Event::Key(key) = event::read()? else {
            continue;
        };
        if matches!(key.kind, KeyEventKind::Release) {
            continue;
        }
        if key.modifiers.contains(KeyModifiers::CONTROL)
            && matches!(key.code, KeyCode::Char('c' | 'C'))
        {
            write_key_echo("^C")?;
            return Err(io::Error::new(
                io::ErrorKind::Interrupted,
                "confirmation interrupted",
            ));
        }

        let input = match key.code {
            KeyCode::Char(value) => value.to_string(),
            KeyCode::Enter => String::new(),
            KeyCode::Esc => "n".to_string(),
            _ => continue,
        };
        write_key_echo(if input.is_empty() { "" } else { &input })?;
        return Ok(input);
    }
}

fn write_key_echo(value: &str) -> io::Result<()> {
    let mut stderr = io::stderr().lock();
    write!(stderr, "{value}\r\n")?;
    stderr.flush()
}

struct RawModeGuard;

impl RawModeGuard {
    fn enable() -> io::Result<Self> {
        enable_raw_mode()?;
        Ok(Self)
    }
}

impl Drop for RawModeGuard {
    fn drop(&mut self) {
        let _ = disable_raw_mode();
    }
}
