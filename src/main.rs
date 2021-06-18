use crossterm::cursor::MoveTo;
use crossterm::event::read;
use crossterm::event::{Event, KeyCode};
use crossterm::terminal::{Clear, ClearType};
use crossterm::QueueableCommand;
use once_cell::sync::Lazy;
use std::cell::Cell;
use std::cell::RefCell;
use std::cmp::min;
use std::io::prelude::*;
use std::io::stdin;
use std::sync::Mutex;
use termcolor::{Color, ColorChoice, ColorSpec, StandardStream, WriteColor};
use unicode_width::UnicodeWidthChar;

static STDOUT: Lazy<Mutex<StandardStream>> =
    Lazy::new(|| Mutex::new(StandardStream::stdout(ColorChoice::Auto)));

pub enum MoveUnit {
    Line,
    HalfPage,
    Entire,
}

fn main() -> anyhow::Result<()> {
    // Read entire input
    let mut input = String::new();
    stdin().read_to_string(&mut input).unwrap();

    let (width, height) = match term_size::dimensions_stdout() {
        Some((w, h)) => (w, h),
        None => {
            eprintln!("(error: Failed to get dimension)");
            println!("{}", input);
            return Ok(());
        }
    };

    let mut scr = Screen::new(width, height, input);
    loop {
        scr.draw();

        match read()? {
            Event::Resize(_, _) => scr.resized(),
            Event::Key(key) => match key.code {
                KeyCode::Enter | KeyCode::Down | KeyCode::Char('j') => scr.down_by(MoveUnit::Line),
                KeyCode::Up | KeyCode::Char('k') => scr.up_by(MoveUnit::Line),
                KeyCode::Char(' ') | KeyCode::Char('f') | KeyCode::Char('d') => {
                    scr.down_by(MoveUnit::HalfPage)
                }
                KeyCode::Char('b') | KeyCode::Char('u') => scr.up_by(MoveUnit::HalfPage),
                KeyCode::Char('g') => scr.up_by(MoveUnit::Entire),
                KeyCode::Char('G') => scr.down_by(MoveUnit::Entire),
                KeyCode::Char('q') => break,
                KeyCode::Char('/') => {
                    let orig_query = scr.query.clone();
                    let mut query = String::new();
                    scr.set_query_mode(true);
                    scr.update_query(query.clone());
                    loop {
                        match read()? {
                            Event::Resize(_, _) => scr.resized(),
                            Event::Key(key) => match key.code {
                                KeyCode::Enter => {
                                    break;
                                }
                                KeyCode::Esc => {
                                    // restore original query
                                    scr.update_query(orig_query);
                                    break;
                                }
                                KeyCode::Backspace => {
                                    let _ = query.pop();
                                    scr.update_query(query.clone());
                                }
                                KeyCode::Char(ch) => {
                                    query.push(ch);
                                    scr.update_query(query.clone());
                                }
                                _ => {}
                            },
                            _ => {}
                        }
                    }
                    scr.set_query_mode(false);
                }
                KeyCode::Char('n') => scr.next(),
                KeyCode::Char('N') => scr.prev(),
                _ => {}
            },
            _ => {}
        }
    }

    scr.clear();
    Ok(())
}

pub struct Screen {
    width: usize,
    height: usize,
    contents: String,
    lines: Vec<String>,
    current_top: isize,
    query_mode: bool,
    query: String,
    message: RefCell<Option<String>>,
    needs_update: Cell<bool>,
}

impl Screen {
    pub fn new(width: usize, height: usize, contents: String) -> Self {
        let mut scr = Self {
            width,
            height,
            contents,
            lines: vec![],
            current_top: 0,
            query_mode: false,
            query: String::new(),
            message: RefCell::new(None),
            needs_update: Cell::new(true),
        };
        scr.recalc_lines();

        scr
    }

    pub fn resized(&mut self) {
        let (width, height) = term_size::dimensions_stdout().unwrap();
        self.update_size(width as usize, height as usize)
    }

    pub fn update_size(&mut self, width: usize, height: usize) {
        if self.width == width && self.height == height {
            return;
        }

        self.width = width;
        self.height = height;
        self.recalc_lines();
        self.fix_current_top();
        self.draw();
    }

    pub fn get_query(&self) -> &str {
        &self.query
    }

    pub fn set_query_mode(&mut self, mode: bool) {
        self.query_mode = mode;
    }

    pub fn update_query(&mut self, query: String) {
        self.query = query;
        self.draw();
    }

    pub fn up_by(&mut self, unit: MoveUnit) {
        match unit {
            MoveUnit::Line => self.scroll(-1),
            MoveUnit::HalfPage => self.scroll(-(self.height as isize) / 2),
            MoveUnit::Entire => self.scroll(-isize::MAX),
        }
    }

    pub fn down_by(&mut self, unit: MoveUnit) {
        match unit {
            MoveUnit::Line => self.scroll(1),
            MoveUnit::HalfPage => self.scroll((self.height as isize) / 2),
            MoveUnit::Entire => self.scroll(isize::MAX),
        }
    }

    pub fn prev(&mut self) {
        if self.query.is_empty() {
            *self.message.borrow_mut() = Some("search query is not set".to_string());
            return;
        }

        match self
            .lines
            .iter()
            .enumerate()
            .take(self.current_top as usize)
            .rev()
            .find(|(_, line)| line.contains(&self.query))
        {
            Some((line, _)) => {
                self.current_top = line as isize;
                self.fix_current_top();
            }
            None => {
                *self.message.borrow_mut() = Some(format!("failed to find `{}`", self.query));
            }
        }
    }

    pub fn next(&mut self) {
        if self.query.is_empty() {
            *self.message.borrow_mut() = Some("search query is not set".to_string());
            return;
        }

        match self
            .lines
            .iter()
            .enumerate()
            .skip(self.current_top as usize + 1)
            .find(|(_, line)| line.contains(&self.query))
        {
            Some((line, _)) => {
                self.current_top = line as isize;
                self.fix_current_top();
            }
            None => {
                *self.message.borrow_mut() = Some(format!("failed to find `{}`", self.query));
            }
        }
    }

    pub fn clear(&self) {
        let mut stdout = STDOUT.lock().unwrap();
        stdout.queue(Clear(ClearType::All)).unwrap();
        stdout.queue(MoveTo(0, 0)).unwrap();
    }

    pub fn draw(&self) {
        if !self.needs_update.get() {
            return;
        }

        let mut stdout = STDOUT.lock().unwrap();
        stdout.queue(MoveTo(0, 0)).unwrap();
        let start = self.current_top as usize;
        let end = min(self.lines.len(), start + self.contents_height());
        debug_assert!(end <= self.lines.len());
        for line in &self.lines[start..end] {
            let mut segments = vec![];
            if self.query.is_empty() {
                segments.push((ColorSpec::new(), &**line));
            } else {
                let mut curr_idx = 0;
                for (next_idx, substr) in line.match_indices(&self.query) {
                    let normal = &line[curr_idx..next_idx];
                    let mut match_color = ColorSpec::new();
                    match_color.set_fg(Some(Color::Red));
                    segments.push((ColorSpec::new(), normal));
                    segments.push((match_color, substr));
                    curr_idx = next_idx + substr.len();
                }
                segments.push((ColorSpec::new(), &line[curr_idx..]));
            };
            stdout.queue(Clear(ClearType::CurrentLine)).unwrap();
            for (spec, segment) in segments {
                stdout.set_color(&spec).unwrap();
                write!(stdout, "{}", segment).unwrap();
                stdout.set_color(&ColorSpec::new()).unwrap();
            }
            writeln!(stdout).unwrap();
        }

        let message = self
            .message
            .borrow()
            .as_ref()
            .cloned()
            .unwrap_or_else(|| self.query.clone());
        stdout
            .queue(MoveTo(0, self.contents_height() as u16))
            .unwrap();
        stdout.queue(Clear(ClearType::CurrentLine)).unwrap();
        print!("{}{}", if self.query_mode { '/' } else { ':' }, message);
        *self.message.borrow_mut() = None;
        self.needs_update.set(false);
        stdout.flush().unwrap();
    }

    fn contents_height(&self) -> usize {
        // The last line is for prompt `:`
        self.height.saturating_sub(1)
    }

    fn recalc_lines(&mut self) {
        self.lines = LineBreaker::new(self.width, &self.contents).collect();
        self.needs_update.set(true);
    }

    fn scroll(&mut self, amount: isize) {
        self.current_top = self.current_top.saturating_add(amount);
        self.fix_current_top();
        self.needs_update.set(true);
    }

    fn fix_current_top(&mut self) {
        let max_top = self.lines.len().saturating_sub(self.contents_height());
        self.current_top = self.current_top.clamp(0, max_top as isize);
        self.needs_update.set(true);
    }
}

struct LineBreaker {
    contents: Vec<char>,
    curr_idx: usize,
    width: usize,
}

impl LineBreaker {
    pub fn new(width: usize, contents: &str) -> Self {
        Self {
            contents: contents.chars().collect(),
            curr_idx: 0,
            width,
        }
    }
}

impl Iterator for LineBreaker {
    type Item = String;

    fn next(&mut self) -> Option<Self::Item> {
        let mut line = String::new();
        let mut curr_width = 0;
        while self.curr_idx < self.contents.len() {
            let ch = self.contents[self.curr_idx];
            self.curr_idx += 1;

            if ch == '\r' {
                continue;
            }

            if ch == '\n' {
                return Some(line);
            }

            let ch_width = ch.width().unwrap_or(1);
            if curr_width + ch_width > self.width {
                self.curr_idx -= 1;
                return Some(line);
            }

            curr_width += ch_width;
            line.push(ch);
        }

        Some(line).filter(|s| !s.is_empty())
    }
}
