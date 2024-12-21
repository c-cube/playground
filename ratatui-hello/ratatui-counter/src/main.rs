use std::io;

use color_eyre::{eyre::WrapErr, Result};
use crossterm::event::{self, Event, KeyCode, KeyEvent, KeyEventKind};
use ratatui::{
    buffer::Buffer,
    layout::Rect,
    style::Stylize,
    symbols::border,
    text::{Line, Text},
    widgets::{Block, Paragraph, Widget},
    DefaultTerminal, Frame,
};

#[derive(Default, Debug)]
struct App {
    counter: u32,
    exit: bool,
}

impl Widget for &App {
    fn render(self, area: Rect, buf: &mut Buffer) {
        let title = Line::from("mytuto".bold());
        let instructions = Line::from(vec![
            " Decrement".into(),
            "<Left>".blue().bold(),
            " Increment".into(),
            "<Right>".blue().bold(),
            " Quit".into(),
            "<Q>".blue().bold(),
        ]);
        let block = Block::bordered()
            .title(title.centered())
            .title_bottom(instructions.centered())
            .border_set(border::THICK);

        let counter_text = Text::from(vec![Line::from(vec![
            "Value: ".into(),
            self.counter.to_string().yellow(),
        ])]);

        Paragraph::new(counter_text)
            .centered()
            .block(block)
            .render(area, buf);
    }
}

impl App {
    fn run(&mut self, terminal: &mut DefaultTerminal) -> io::Result<()> {
        while !self.exit {
            terminal.draw(|frame| self.draw(frame))?;
            self.handle_events()?
        }
        Ok(())
    }

    fn draw(&self, frame: &mut Frame) {
        frame.render_widget(self, frame.area())
    }

    fn handle_events(&mut self) -> io::Result<()> {
        match event::read()? {
            Event::Key(kev) if kev.kind == KeyEventKind::Press => self.handle_key_event(kev),
            _ => (),
        };
        Ok(())
    }

    fn handle_key_event(&mut self, kev: KeyEvent) {
        match kev.code {
            KeyCode::Char('q') => self.exit = true,
            KeyCode::Left => self.counter -= 1,
            KeyCode::Right => self.counter += 1,
            _ => {}
        }
    }
}

fn main() -> Result<()> {
    color_eyre::install()?;
    let mut terminal = ratatui::init();
    let app_res = App::default().run(&mut terminal);
    if let Err(err) = ratatui::try_restore() {
        eprintln!("failed to restore term. Use `reset` to repair it: {}", err);
    }
    Ok(app_res?)
}
