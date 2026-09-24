//! Inline list pickers (multi and single selection) drawn in place in the
//! terminal: live selection total, filtering, bulk actions and scrolling.

use std::io;

use console::{Key, Term, style};

use crate::ui::{self, ARROW, MARGIN};

pub struct Row {
    /// Pre-rendered (styled) line content, already fitted to the row width.
    pub line: String,
    /// Plain text matched against the filter.
    pub search: String,
    pub size: u64,
    /// Candidate for the "select stale" shortcut.
    pub stale: bool,
    /// Initially selected.
    pub checked: bool,
}

/// Width available to `Row::line` (the picker adds its own gutter).
pub fn row_width() -> usize {
    ui::width() - MARGIN.len() - 5
}

const RADIO_ON: &str = "◉";
const RADIO_OFF: &str = "○";
const POINTER: &str = "▸";

struct Picker<'a> {
    title: &'a str,
    rows: &'a [Row],
    multi: bool,
    checked: Vec<bool>,
    cursor: usize,
    offset: usize,
    filter: String,
    filtering: bool,
}

impl Picker<'_> {
    fn visible(&self) -> Vec<usize> {
        let needle = self.filter.to_lowercase();
        (0..self.rows.len())
            .filter(|&i| needle.is_empty() || self.rows[i].search.to_lowercase().contains(&needle))
            .collect()
    }

    fn page_size() -> usize {
        ui::height().saturating_sub(9).clamp(4, 16)
    }

    fn selection(&self) -> (usize, u64) {
        self.checked
            .iter()
            .zip(self.rows)
            .filter(|(c, _)| **c)
            .fold((0, 0), |(n, s), (_, r)| (n + 1, s + r.size))
    }

    fn summary(&self) -> String {
        let (n, bytes) = self.selection();
        if n == 0 {
            style("nothing selected").dim().to_string()
        } else {
            format!(
                "{} {} {}",
                style(format!("{n} of {}", self.rows.len())).bold(),
                style(ui::DOT).dim(),
                ui::size_style(bytes).bold().apply_to(ui::human_size(bytes))
            )
        }
    }

    fn render(&mut self, visible: &[usize]) -> Vec<String> {
        let page = Self::page_size();
        self.cursor = self.cursor.min(visible.len().saturating_sub(1));
        if self.cursor < self.offset {
            self.offset = self.cursor;
        } else if self.cursor >= self.offset + page {
            self.offset = self.cursor + 1 - page;
        }
        self.offset = self.offset.min(visible.len().saturating_sub(page));

        let mut lines = Vec::new();
        let status = if self.multi {
            format!("   {}", self.summary())
        } else {
            String::new()
        };
        lines.push(format!(
            "{MARGIN}{} {}{status}",
            ui::accent().bold().apply_to(ui::ASK),
            style(self.title).bold()
        ));

        if self.filtering || !self.filter.is_empty() {
            let caret = if self.filtering {
                ui::accent().apply_to("▏").to_string()
            } else {
                String::new()
            };
            lines.push(format!(
                "{MARGIN}  {} {}{caret}   {}",
                style("filter").dim(),
                ui::accent().apply_to(&self.filter),
                style(format!("{} match", visible.len())).dim()
            ));
        }

        let more = |n: usize, arrow: &str| {
            style(format!("{MARGIN}    {arrow} {n} more"))
                .dim()
                .to_string()
        };
        lines.push(if self.offset > 0 {
            more(self.offset, "↑")
        } else {
            String::new()
        });

        if visible.is_empty() {
            lines.push(style(format!("{MARGIN}    no match")).dim().to_string());
        }
        for (pos, &i) in visible.iter().enumerate().skip(self.offset).take(page) {
            let active = pos == self.cursor;
            let pointer = if active {
                ui::accent().bold().apply_to(POINTER).to_string()
            } else {
                " ".to_string()
            };
            let mark = if !self.multi {
                String::new()
            } else if self.checked[i] {
                format!("{} ", style(RADIO_ON).green().bold())
            } else {
                format!("{} ", style(RADIO_OFF).dim())
            };
            lines.push(format!("{MARGIN}{pointer} {mark}{}", self.rows[i].line));
        }

        let below = visible.len().saturating_sub(self.offset + page);
        lines.push(if below > 0 {
            more(below, "↓")
        } else {
            String::new()
        });

        lines.push(self.help());
        lines
    }

    fn help(&self) -> String {
        let key = |k: &str, d: &str| format!("{} {}", style(k).bold(), style(d).dim());
        let keys: Vec<String> = if self.filtering {
            vec![
                key("type", "to filter"),
                key("enter", "apply"),
                key("esc", "clear"),
            ]
        } else if self.multi {
            vec![
                key("↑↓", "move"),
                key("space", "toggle"),
                key("a", "all"),
                key("n", "none"),
                key("s", "stale"),
                key("/", "filter"),
                key("enter", "confirm"),
                key("esc", "cancel"),
            ]
        } else {
            vec![
                key("↑↓", "move"),
                key("/", "filter"),
                key("enter", "select"),
                key("esc", "cancel"),
            ]
        };
        let line = format!("{MARGIN}  {}", keys.join("   "));
        ui::fit(&line, ui::width() - 1)
    }

    fn set_visible(&mut self, visible: &[usize], f: impl Fn(&Row, bool) -> bool) {
        for &i in visible {
            self.checked[i] = f(&self.rows[i], self.checked[i]);
        }
    }

    /// Returns `Some(done)` when the interaction ends.
    fn handle(&mut self, key: Key, visible: &[usize]) -> Option<bool> {
        let len = visible.len();
        let page = Self::page_size();
        if self.filtering {
            match key {
                Key::Char(c) if !c.is_control() => {
                    self.filter.push(c);
                    self.cursor = 0;
                }
                Key::Backspace => {
                    self.filter.pop();
                }
                Key::Enter => self.filtering = false,
                Key::Escape => {
                    self.filter.clear();
                    self.filtering = false;
                }
                Key::CtrlC => return Some(false),
                Key::ArrowUp => self.cursor = self.cursor.saturating_sub(1),
                Key::ArrowDown => self.cursor = (self.cursor + 1).min(len.saturating_sub(1)),
                _ => {}
            }
            return None;
        }
        match key {
            Key::ArrowUp | Key::Char('k') if len > 0 => {
                self.cursor = (self.cursor + len - 1) % len;
            }
            Key::ArrowDown | Key::Char('j') | Key::Tab if len > 0 => {
                self.cursor = (self.cursor + 1) % len;
            }
            Key::PageUp => self.cursor = self.cursor.saturating_sub(page),
            Key::PageDown => self.cursor = (self.cursor + page).min(len.saturating_sub(1)),
            Key::Home => self.cursor = 0,
            Key::End => self.cursor = len.saturating_sub(1),
            Key::Char(' ') if self.multi && len > 0 => {
                let i = visible[self.cursor];
                self.checked[i] = !self.checked[i];
            }
            Key::Char('a') if self.multi => self.set_visible(visible, |_, _| true),
            Key::Char('n') if self.multi => self.set_visible(visible, |_, _| false),
            Key::Char('i') if self.multi => self.set_visible(visible, |_, c| !c),
            Key::Char('s') if self.multi => self.set_visible(visible, |r, c| c || r.stale),
            Key::Char('/') => self.filtering = true,
            Key::Enter if self.multi || len > 0 => return Some(true),
            Key::Escape | Key::Char('q') | Key::CtrlC => return Some(false),
            _ => {}
        }
        None
    }
}

fn run(picker: &mut Picker) -> io::Result<bool> {
    let term = Term::stderr();
    if !term.is_term() {
        return Err(io::Error::other(
            "interactive selection needs a terminal (use --all to skip it)",
        ));
    }
    term.hide_cursor()?;
    let mut drawn = 0;
    let result = loop {
        let visible = picker.visible();
        let lines = picker.render(&visible);
        if drawn > 0 {
            term.clear_last_lines(drawn)?;
        }
        for line in &lines {
            term.write_line(ui::fit(line, ui::width() - 1).trim_end())?;
        }
        drawn = lines.len();
        match term.read_key_raw() {
            Ok(key) => {
                if let Some(done) = picker.handle(key, &visible) {
                    break Ok(done);
                }
            }
            Err(e) => break Err(e),
        }
    };
    term.clear_last_lines(drawn)?;
    term.show_cursor()?;
    result
}

/// Multi-selection. Returns `None` when cancelled.
pub fn multi_select(title: &str, rows: &[Row]) -> io::Result<Option<Vec<usize>>> {
    let mut picker = Picker {
        title,
        rows,
        multi: true,
        checked: rows.iter().map(|r| r.checked).collect(),
        cursor: 0,
        offset: 0,
        filter: String::new(),
        filtering: false,
    };
    let confirmed = run(&mut picker)?;
    let chosen: Vec<usize> = (0..rows.len()).filter(|&i| picker.checked[i]).collect();
    let outcome = if confirmed {
        picker.summary()
    } else {
        style("cancelled").dim().to_string()
    };
    eprintln!(
        "{MARGIN}{} {}   {outcome}",
        ui::accent().bold().apply_to(ui::ASK),
        style(title).bold()
    );
    Ok(confirmed.then_some(chosen))
}

/// Single selection. Returns `None` when cancelled.
pub fn select(title: &str, rows: &[Row]) -> io::Result<Option<usize>> {
    let mut picker = Picker {
        title,
        rows,
        multi: false,
        checked: vec![false; rows.len()],
        cursor: 0,
        offset: 0,
        filter: String::new(),
        filtering: false,
    };
    let confirmed = run(&mut picker)?;
    let visible = picker.visible();
    let chosen = visible.get(picker.cursor).copied().filter(|_| confirmed);
    let outcome = match chosen {
        Some(i) => format!("{} {}", style(ARROW).dim(), rows[i].search),
        None => style("cancelled").dim().to_string(),
    };
    eprintln!(
        "{MARGIN}{} {}   {outcome}",
        ui::accent().bold().apply_to(ui::ASK),
        style(title).bold()
    );
    Ok(chosen)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn rows() -> Vec<Row> {
        ["alpha", "beta", "gamma"]
            .iter()
            .enumerate()
            .map(|(i, s)| Row {
                line: s.to_string(),
                search: s.to_string(),
                size: 10 * (i as u64 + 1),
                stale: i == 2,
                checked: false,
            })
            .collect()
    }

    fn picker(rows: &[Row]) -> Picker<'_> {
        Picker {
            title: "t",
            rows,
            multi: true,
            checked: vec![false; rows.len()],
            cursor: 0,
            offset: 0,
            filter: String::new(),
            filtering: false,
        }
    }

    #[test]
    fn toggles_and_bulk_actions() {
        let rows = rows();
        let mut p = picker(&rows);
        let all = p.visible();
        p.handle(Key::Char(' '), &all);
        assert_eq!(p.selection(), (1, 10));
        p.handle(Key::Char('a'), &all);
        assert_eq!(p.selection(), (3, 60));
        p.handle(Key::Char('n'), &all);
        p.handle(Key::Char('s'), &all);
        assert_eq!(p.selection(), (1, 30));
    }

    #[test]
    fn filter_narrows_visible_rows() {
        let rows = rows();
        let mut p = picker(&rows);
        let all = p.visible();
        p.handle(Key::Char('/'), &all);
        for c in "ta".chars() {
            p.handle(Key::Char(c), &all);
        }
        assert_eq!(p.visible(), vec![1]);
        p.handle(Key::Escape, &all);
        assert_eq!(p.visible().len(), 3);
    }

    #[test]
    fn cursor_wraps_around() {
        let rows = rows();
        let mut p = picker(&rows);
        let all = p.visible();
        p.handle(Key::ArrowUp, &all);
        assert_eq!(p.cursor, 2);
        p.handle(Key::ArrowDown, &all);
        assert_eq!(p.cursor, 0);
        assert_eq!(p.handle(Key::Escape, &all), Some(false));
    }
}
