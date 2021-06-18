use crossterm::cursor::MoveTo;
use crossterm::event::read;
use crossterm::event::{Event, KeyCode};
use crossterm::terminal::{Clear, ClearType};
use crossterm::QueueableCommand;
use std::cell::RefCell;
use std::cmp::min;
use std::io::prelude::*;
use std::io::{stdin, stdout};
use unicode_width::UnicodeWidthChar;

pub enum MoveUnit {
    Line,
    HalfPage,
    Entire,
}

fn main() -> anyhow::Result<()> {
    // Read entire input
    let mut input = String::new();
    stdin().read_to_string(&mut input).unwrap();

    let (w, h) = match term_size::dimensions_stdout() {
        Some((w, h)) => (w, h),
        None => {
            eprintln!("(error: Failed to get dimension)");
            println!("{}", input);
            return Ok(());
        }
    };

    let mut scr = Screen::new(w, h, input);
    loop {
        scr.draw();

        match read()? {
            Event::Resize(width, height) => scr.update_size(width as usize, height as usize),
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
                            Event::Resize(width, height) => {
                                scr.update_size(width as usize, height as usize)
                            }
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
        };
        scr.recalc_lines();

        scr
    }

    pub fn update_size(&mut self, width: usize, height: usize) {
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

    pub fn draw(&self) {
        let mut stdout = stdout();
        stdout.queue(MoveTo(0, 0)).unwrap();
        let start = self.current_top as usize;
        let end = min(self.lines.len(), start + self.contents_height());
        debug_assert!(end <= self.lines.len());
        for line in &self.lines[start..end] {
            stdout.queue(Clear(ClearType::CurrentLine)).unwrap();
            println!("{}", line);
        }

        let message = self
            .message
            .borrow()
            .as_ref()
            .cloned()
            .unwrap_or_else(|| self.query.clone());
        stdout.queue(Clear(ClearType::CurrentLine)).unwrap();
        print!("{}{}", if self.query_mode { '/' } else { ':' }, message);
        *self.message.borrow_mut() = None;
        stdout.flush().unwrap();
    }

    fn contents_height(&self) -> usize {
        // The last line is `:`
        self.height.saturating_sub(1)
    }

    fn recalc_lines(&mut self) {
        self.lines = LineBreaker::new(self.width, &self.contents).collect();
    }

    fn scroll(&mut self, amount: isize) {
        self.current_top = self.current_top.saturating_add(amount);
        self.fix_current_top();
    }

    fn fix_current_top(&mut self) {
        self.current_top = self.current_top.clamp(
            0,
            (self.lines.len().saturating_sub(self.contents_height())) as isize,
        );
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
