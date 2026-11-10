#![deny(warnings, clippy::pedantic, clippy::unwrap_used)]

use std::{
    fs, io,
    path::PathBuf,
    process::ExitCode,
    time::{Duration, Instant},
};

use clap::Parser;
use crossterm::event::{self, Event, KeyCode, KeyEventKind, KeyModifiers};
use ratatui::{
    DefaultTerminal, Frame,
    layout::{Constraint, Layout, Rect},
    style::{Color, Stylize},
    text::Span,
    widgets::Paragraph,
};

use unicode_segmentation::UnicodeSegmentation;

#[derive(Parser)]
#[command(version, about = "Read a Markdown file one word at a time")]
struct Args {
    /// UTF-8 Markdown file to read (Markdown syntax is retained)
    path: PathBuf,
}

struct Reader {
    words: Vec<String>,
    index: usize,
    delay_ms: u32,
    paused: bool,
    finished: bool,
}

impl Reader {
    fn new(text: &str) -> io::Result<Self> {
        let words = split_to_words(text);
        if words.is_empty() {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "file contains no words",
            ));
        }
        Ok(Self {
            words,
            index: 0,
            delay_ms: 1000,
            paused: false,
            finished: false,
        })
    }

    fn key(&mut self, key: KeyCode) -> bool {
        match key {
            KeyCode::Char('l' | 'L') => self.delay_ms = self.delay_ms.saturating_sub(10).max(10),
            KeyCode::Char('h' | 'H') => self.delay_ms = (self.delay_ms + 10).min(2000),
            KeyCode::Char('j' | 'J') if !self.finished => self.paused = !self.paused,
            KeyCode::Char('r' | 'R') => {
                self.index = 0;
                self.paused = false;
                self.finished = false;
            }
            KeyCode::Char('q' | 'Q') => return true,
            _ => {}
        }
        false
    }

    // Complete after the final word's interval, retaining it until replay or quit.
    fn tick(&mut self) -> bool {
        if self.paused {
            return false;
        }
        if self.index + 1 == self.words.len() {
            self.finished = true;
            return true;
        }
        self.index += 1;
        false
    }

    fn interval(&self) -> Duration {
        Duration::from_millis(u64::from(self.delay_ms))
    }
}

fn split_to_words(text: &str) -> Vec<String> {
    text.split_whitespace().map(String::from).collect()
}

fn wpm(delay_ms: u32) -> u32 {
    60_000 / delay_ms
}

fn draw(frame: &mut Frame, app: &Reader) {
    let [help, status, body] = Layout::vertical([
        Constraint::Length(1),
        Constraint::Length(1),
        Constraint::Min(0),
    ])
    .areas(frame.area());
    frame.render_widget(
        Paragraph::new("L: faster | H: slower | J: pause/resume | R: replay | Q: quit")
            .fg(Color::Cyan),
        help,
    );
    frame.render_widget(
        Paragraph::new(format!(
            "Words / min: {} | {} | {}/{}",
            wpm(app.delay_ms),
            if app.finished {
                "Finished - R to replay"
            } else if app.paused {
                "Paused"
            } else {
                "Playing"
            },
            app.index + 1,
            app.words.len(),
        ))
        .fg(Color::Yellow),
        status,
    );
    let [_, word, _] = Layout::vertical([
        Constraint::Fill(1),
        Constraint::Length(1),
        Constraint::Fill(1),
    ])
    .areas(body);
    draw_word(frame, app.words[app.index].as_str(), word);
}

fn draw_word(frame: &mut Frame, word: &str, area: Rect) {
    if area.is_empty() {
        return;
    }
    // Graphemes keep accents and emoji together; widths are terminal cells, not bytes.
    let letters: Vec<_> = word.graphemes(true).map(Span::raw).collect();
    let middle = letters.len() / 2;
    let prefix_width: usize = letters[..middle].iter().map(Span::width).sum();
    let Ok(prefix_width) = i64::try_from(prefix_width) else {
        return;
    };
    let mut x = i64::from(area.x) + i64::from(area.width / 2) - prefix_width;
    for (index, letter) in letters.into_iter().enumerate() {
        let Ok(width) = i64::try_from(letter.width()) else {
            return;
        };
        // Clip whole glyphs at either edge without moving the focal letter.
        if width > 0
            && x >= i64::from(area.x)
            && x + width <= i64::from(area.right())
            && let (Ok(column), Ok(cell_width)) = (u16::try_from(x), u16::try_from(width))
        {
            let letter = if index == middle {
                letter.red()
            } else {
                letter
            };
            frame.render_widget(
                Paragraph::new(letter).bold(),
                Rect::new(column, area.y, cell_width, 1),
            );
        }
        x += width;
    }
}

fn run(terminal: &mut DefaultTerminal, app: &mut Reader) -> io::Result<()> {
    let mut deadline = Instant::now() + app.interval();
    loop {
        terminal.draw(|frame| draw(frame, app))?;
        let timeout = if app.paused || app.finished {
            Duration::from_millis(250)
        } else {
            deadline.saturating_duration_since(Instant::now())
        };
        if event::poll(timeout)?
            && let Event::Key(key) = event::read()?
        {
            if key.kind == KeyEventKind::Release {
                continue;
            }
            if key.code == KeyCode::Char('c') && key.modifiers.contains(KeyModifiers::CONTROL) {
                return Ok(());
            }
            let previous = (app.paused, app.delay_ms);
            if app.key(key.code) {
                return Ok(());
            }
            if previous != (app.paused, app.delay_ms)
                || matches!(key.code, KeyCode::Char('r' | 'R'))
            {
                deadline = Instant::now() + app.interval();
            }
        }
        if !app.paused && !app.finished && Instant::now() >= deadline {
            app.tick();
            deadline = Instant::now() + app.interval();
        }
    }
}

fn start(args: &Args) -> io::Result<()> {
    // Validate input before entering raw mode, so errors leave the terminal untouched.
    let text = fs::read_to_string(&args.path).map_err(|error| {
        io::Error::new(error.kind(), format!("{}: {error}", args.path.display()))
    })?;
    let mut app = Reader::new(&text)?;
    let mut terminal = ratatui::try_init()?;
    let result = run(&mut terminal, &mut app);
    ratatui::restore();
    result
}

fn main() -> ExitCode {
    match start(&Args::parse()) {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("read-fast: {error}");
            ExitCode::FAILURE
        }
    }
}

#[cfg(test)]
mod tests;
