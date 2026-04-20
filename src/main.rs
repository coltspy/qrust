use std::io;

use crossterm::event::{self, Event, KeyCode, KeyEvent, KeyEventKind};
use ratatui::{
    DefaultTerminal, Frame,
    layout::{Constraint, Direction, Layout},
    style::{Color, Modifier, Style, Stylize},
    text::{Line, Span},
    widgets::{Block, Borders, Paragraph, Wrap},
};

use qrust::{Cell, Circuit, State};

const QUBITS: usize = 4;
const COLS: usize = 12;

struct App {
    circuit: Circuit,
    cursor_col: usize,
    cursor_q: usize,
    pending_cnot: Option<(usize, usize)>,
    result: Option<State>,
    message: String,
    exit: bool,
}

impl Default for App {
    fn default() -> Self {
        Self {
            circuit: Circuit::new(QUBITS, COLS),
            cursor_col: 0,
            cursor_q: 0,
            pending_cnot: None,
            result: None,
            message: String::new(),
            exit: false,
        }
    }
}

impl App {
    fn run(&mut self, terminal: &mut DefaultTerminal) -> io::Result<()> {
        while !self.exit {
            terminal.draw(|f| self.draw(f))?;
            self.handle_events()?;
        }
        Ok(())
    }

    fn draw(&self, frame: &mut Frame) {
        let area = frame.area();
        let chunks = Layout::default()
            .direction(Direction::Vertical)
            .constraints([
                Constraint::Length(3),
                Constraint::Length((QUBITS as u16) + 2),
                Constraint::Min(6),
                Constraint::Length(4),
            ])
            .split(area);

        frame.render_widget(header(), chunks[0]);
        frame.render_widget(self.grid_widget(), chunks[1]);
        frame.render_widget(self.results_widget(), chunks[2]);
        frame.render_widget(self.help_widget(), chunks[3]);
    }

    fn grid_widget(&self) -> Paragraph<'_> {
        let mut lines = Vec::with_capacity(self.circuit.qubits);
        for q in (0..self.circuit.qubits).rev() {
            let mut spans = vec![Span::raw(format!(" q{q} "))];
            for col in 0..self.circuit.cols.len() {
                let cell = self.circuit.cols[col][q];
                let glyph = match cell {
                    None => '─',
                    Some(c) => c.symbol(),
                };
                let is_cursor = col == self.cursor_col && q == self.cursor_q;
                let is_pending = self.pending_cnot == Some((col, q));
                let style = if is_cursor {
                    Style::default()
                        .bg(Color::Blue)
                        .fg(Color::White)
                        .add_modifier(Modifier::BOLD)
                } else if is_pending {
                    Style::default()
                        .fg(Color::Yellow)
                        .add_modifier(Modifier::BOLD)
                } else if cell.is_some() {
                    Style::default()
                        .fg(Color::Cyan)
                        .add_modifier(Modifier::BOLD)
                } else {
                    Style::default()
                };
                spans.push(Span::raw("─"));
                spans.push(Span::styled(glyph.to_string(), style));
                spans.push(Span::raw("─"));
            }
            lines.push(Line::from(spans));
        }
        Paragraph::new(lines).block(
            Block::default()
                .title(Line::from(" circuit ").bold())
                .borders(Borders::ALL),
        )
    }

    fn results_widget(&self) -> Paragraph<'_> {
        let lines: Vec<Line> = match &self.result {
            None => vec![Line::from("press r to run the circuit".italic())],
            Some(s) => {
                let mut v: Vec<Line> = vec![Line::from("per-qubit P(|1⟩):".bold())];
                for q in 0..s.qubits {
                    let p = s.qubit_prob_one(q);
                    let bar = prob_bar(p, 32);
                    v.push(Line::from(format!("  q{q}  {p:.4}  {bar}")));
                }
                v.push(Line::from(""));
                v.push(Line::from("top basis states:".bold()));
                let mut probs: Vec<(usize, f64)> = s
                    .amps
                    .iter()
                    .enumerate()
                    .map(|(i, a)| (i, a.norm_sqr()))
                    .collect();
                probs.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap());
                for (i, p) in probs.into_iter().take(6) {
                    if p < 1e-8 {
                        break;
                    }
                    let bits = format_bits(i, s.qubits);
                    let bar = prob_bar(p, 32);
                    v.push(Line::from(format!("  |{bits}⟩  {p:.4}  {bar}")));
                }
                v
            }
        };
        Paragraph::new(lines).wrap(Wrap { trim: false }).block(
            Block::default()
                .title(Line::from(" results ").bold())
                .borders(Borders::ALL),
        )
    }

    fn help_widget(&self) -> Paragraph<'_> {
        let hint = if self.pending_cnot.is_some() {
            "cnot: move to target qubit, Enter confirms  ·  Esc cancels"
        } else {
            "arrows move  ·  h x y z s t place  ·  c cnot  ·  ⌫ clear  ·  r run  ·  q quit"
        };
        let mut lines = vec![Line::from(hint)];
        if !self.message.is_empty() {
            lines.push(Line::from(self.message.clone().yellow()));
        }
        Paragraph::new(lines).block(Block::default().borders(Borders::ALL))
    }

    fn handle_events(&mut self) -> io::Result<()> {
        if let Event::Key(k) = event::read()? {
            if k.kind == KeyEventKind::Press {
                self.on_key(k);
            }
        }
        Ok(())
    }

    fn on_key(&mut self, k: KeyEvent) {
        self.message.clear();
        match k.code {
            KeyCode::Esc => {
                if self.pending_cnot.take().is_none() {
                    self.exit = true;
                }
            }
            KeyCode::Char('q') if self.pending_cnot.is_none() => self.exit = true,
            KeyCode::Left => self.cursor_col = self.cursor_col.saturating_sub(1),
            KeyCode::Right => {
                self.cursor_col = (self.cursor_col + 1).min(self.circuit.cols.len() - 1);
            }
            KeyCode::Up => {
                self.cursor_q = (self.cursor_q + 1).min(self.circuit.qubits - 1);
            }
            KeyCode::Down => self.cursor_q = self.cursor_q.saturating_sub(1),
            KeyCode::Char('h') => self.place(Cell::H),
            KeyCode::Char('x') => self.place(Cell::X),
            KeyCode::Char('y') => self.place(Cell::Y),
            KeyCode::Char('z') => self.place(Cell::Z),
            KeyCode::Char('s') => self.place(Cell::S),
            KeyCode::Char('t') => self.place(Cell::T),
            KeyCode::Char('c') => self.start_cnot(),
            KeyCode::Enter => self.confirm_cnot(),
            KeyCode::Backspace | KeyCode::Delete => {
                self.circuit.clear(self.cursor_col, self.cursor_q);
                self.pending_cnot = None;
            }
            KeyCode::Char('r') => self.result = Some(self.circuit.run()),
            _ => {}
        }
    }

    fn place(&mut self, cell: Cell) {
        self.circuit
            .place_single(self.cursor_col, self.cursor_q, cell);
        self.pending_cnot = None;
    }

    fn start_cnot(&mut self) {
        self.pending_cnot = Some((self.cursor_col, self.cursor_q));
        self.message = "cnot: move to target qubit (same column), Enter to confirm".into();
    }

    fn confirm_cnot(&mut self) {
        let Some((col, ctrl)) = self.pending_cnot.take() else {
            return;
        };
        if col != self.cursor_col {
            self.message = "cnot cancelled: target must be in the same column".into();
            return;
        }
        if !self.circuit.place_cnot(col, ctrl, self.cursor_q) {
            self.message = "cnot cancelled: pick a different qubit".into();
        }
    }
}

fn header() -> Paragraph<'static> {
    Paragraph::new(Line::from(vec![
        " qrust ".bold().magenta(),
        "— quantum circuit simulator".into(),
    ]))
    .block(Block::default().borders(Borders::ALL))
}

fn prob_bar(p: f64, width: usize) -> String {
    let filled = ((p * width as f64).round() as usize).min(width);
    let mut s = String::with_capacity(width);
    for _ in 0..filled {
        s.push('█');
    }
    for _ in filled..width {
        s.push('░');
    }
    s
}

fn format_bits(i: usize, n: usize) -> String {
    (0..n)
        .rev()
        .map(|q| if i & (1 << q) != 0 { '1' } else { '0' })
        .collect()
}

fn main() -> io::Result<()> {
    ratatui::run(|terminal| App::default().run(terminal))
}
