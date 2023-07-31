use crossterm::{
    event::{self, DisableMouseCapture, EnableMouseCapture, Event, KeyCode},
    execute,
    terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen},
};
use std::{error::Error, io::self, time::{Duration, Instant}, env};
use tui::{
    backend::{Backend, CrosstermBackend},
    layout::{Constraint, Direction, Layout, Rect, Alignment},
    style::{Color, Modifier, Style},
    text::{Span, Spans, Text},
    widgets::{Block, Borders, Paragraph, Tabs, Cell, Row, Table, TableState, Clear},
    Frame, Terminal,
};
use unicode_width::UnicodeWidthStr;
use reqwest::{blocking::Client, header::COOKIE};
use reqwest::header::{HeaderValue, CONTENT_TYPE, SET_COOKIE};
use serde::{Serialize, Deserialize};

enum InputMode {
    Normal,
    EditingRow1,
    EditingRow2,
}

struct App {
    server_host: String,
    username: String,
    password: String,
    password_input: String,
    user_cookie: Option<String>,
    input_mode: InputMode,
    titles: Vec<String>,
    index: usize,
    state: TableState,
    items: Vec<ClientItem>,
    http_client: Client,
    show_pop: bool,
    pop_message: String,
}

impl Default for App {
    fn default() -> App {
        App {
            server_host: "http://127.0.0.1:8000".to_string(),
            username: String::new(),
            password: String::new(),
            password_input: String::new(),
            user_cookie: None,
            input_mode: InputMode::Normal,
            titles: vec!["Clients".to_string()],
            index: 0,
            state: TableState::default(),
            items: vec![],
            http_client: reqwest::blocking::ClientBuilder::new().danger_accept_invalid_certs(true).build().unwrap(),
            show_pop: false,
            pop_message: String::new(),
        }
    }
}

#[derive(Serialize, Deserialize, Debug)]
struct ClientItem {
    id: u32,
    client_ip: Option<String>,
    name: Option<String>,
    is_online: u8,
    last_online_time: Option<String>,
    is_enable: u8,
    created_at: Option<String>,
    uptime: Option<f64>,
    boot_time: Option<String>,
    cpu_user: Option<f64>,
    cpu_system: Option<f64>,
    cpu_nice: Option<f64>,
    cpu_idle: Option<f64>,
    memory_free: Option<f64>,
    memory_total: Option<f64>,
    system_version: Option<String>,
    package_manager_update_count: u32,
    ssh_address: Option<String>,
    ssh_username: Option<String>,
    ssh_password: Option<String>,
    cpu_temp: Option<f64>,
    disk_avail: Option<f64>,
    disk_total: Option<f64>,
}

impl App {
    pub fn next(&mut self) {
        self.index = (self.index + 1) % self.titles.len();
    }

    pub fn previous(&mut self) {
        if self.index > 0 {
            self.index -= 1;
        } else {
            self.index = self.titles.len() - 1;
        }
    }

    pub fn down(&mut self) {
        let i = match self.state.selected() {
            Some(i) => {
                if i >= self.items.len() - 1 {
                    0
                } else {
                    i + 1
                }
            }
            None => 0,
        };
        self.state.select(Some(i));
    }

    pub fn up(&mut self) {
        let i = match self.state.selected() {
            Some(i) => {
                if i == 0 {
                    self.items.len() - 1
                } else {
                    i - 1
                }
            }
            None => 0,
        };
        self.state.select(Some(i));
    }

    fn get_client_list(&mut self) -> Result<(), Box<dyn Error>> {
        let url = format!("{}/get_statistics", self.server_host);
        let cookie = Box::leak(self.user_cookie.clone().unwrap().into_boxed_str());
        let response = self.http_client
            .post(&url)
            .header(CONTENT_TYPE, HeaderValue::from_static("application/x-www-form-urlencoded"))
            .header(COOKIE, HeaderValue::from_static(cookie))
            .send()?;

        let content = response.json::<Vec<ClientItem>>()?;
        self.items = content;

        Ok(())
    }
}

fn main() -> Result<(), Box<dyn Error>> {
    enable_raw_mode()?;
    let mut stdout = io::stdout();
    execute!(stdout, EnterAlternateScreen, EnableMouseCapture)?;
    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend)?;

    let mut app = App::default();
    if let Some(h) = env::args().skip(1).next() {
        app.server_host = h;        
    }
    let tick_rate = Duration::from_millis(250);
    let res = run_app(&mut terminal, app, tick_rate);

    disable_raw_mode()?;
    execute!(
        terminal.backend_mut(),
        LeaveAlternateScreen,
        DisableMouseCapture
    )?;
    terminal.show_cursor()?;

    if let Err(err) = res {
        println!("{:?}", err)
    }

    Ok(())
}

fn run_app<B: Backend>(terminal: &mut Terminal<B>, mut app: App, tick_rate: Duration) -> Result<App, Box<dyn Error>> {
    loop {
        terminal.draw(|f| ui(f, &app))?;

        if let Event::Key(key) = event::read()? {
            if app.show_pop {
               match key.code {
                    KeyCode::Esc => {
                        app.show_pop = false;
                    }
                    _ => {}
                }
               continue;
            }

            match app.input_mode {
                InputMode::Normal => match key.code {
                    KeyCode::Char('e') => {
                        app.input_mode = InputMode::EditingRow1;
                    }
                    KeyCode::Char('q') => {
                        return Ok(app);
                    }
                    _ => {}
                },
                InputMode::EditingRow1 => match key.code {
                    KeyCode::Char(c) => {
                        app.username.push(c);
                    }
                    KeyCode::Backspace => {
                        app.username.pop();
                    }
                    KeyCode::Esc => {
                        app.input_mode = InputMode::Normal;
                    }
                    KeyCode::Tab => {
                        app.input_mode = InputMode::EditingRow2;
                    }
                    _ => {}
                },
                InputMode::EditingRow2 => match key.code {
                    KeyCode::Enter => {
                        let cookie = match login(&app) {
                            Ok(c) => c,
                            Err(e) => {
                                warning(&mut app, e.to_string());
                                continue;
                            }
                        };
                        let split = cookie.split(";").collect::<Vec<&str>>();
                        app.user_cookie = Some(split.get(0).unwrap().to_string());
                        break;
                    }
                    KeyCode::Char(c) => {
                        app.password_input.push('*');
                        app.password.push(c);
                    }
                    KeyCode::Backspace => {
                        app.password_input.pop();
                        app.password.pop();
                    }
                    KeyCode::Esc => {
                        app.input_mode = InputMode::Normal;
                    }
                    KeyCode::Tab => {
                        app.input_mode = InputMode::EditingRow1;
                    }
                    _ => {}
                },
            }
        }
    }
    
    let mut last_tick = Instant::now();
    loop {
        terminal.draw(|f| statistics_ui(f, &mut app))?;

        let timeout = tick_rate
            .checked_sub(last_tick.elapsed())
            .unwrap_or_else(|| Duration::from_secs(0));
        if crossterm::event::poll(timeout)? {
            if let Event::Key(key) = event::read()? {
                match key.code {
                    KeyCode::Char('q') => return Ok(app),
                    KeyCode::Right => app.next(),
                    KeyCode::Left => app.previous(),
                    KeyCode::Down => app.down(),
                    KeyCode::Up => app.up(),
                    _ => {}
                }
            }
        }
        if last_tick.elapsed() >= tick_rate {
            app.get_client_list()?;
            last_tick = Instant::now();
        }
    }
    Ok(app)
}

fn login(app: &App) -> Result<String, Box<dyn Error>> {
    let url = format!("{}/login", app.server_host);
    let response = app.http_client
        .post(&url)
        .header(CONTENT_TYPE, HeaderValue::from_static("application/x-www-form-urlencoded"))
        .body(format!("username={}&password={}", app.username, app.password))
        .send()?;

    let cookie = match response.headers().get(SET_COOKIE) {
        Some(s) => s.to_str().unwrap().to_string(),
        None => "".to_string(),
    };
    let content = response.json::<serde_json::Value>()?;
    match content.get("ok").unwrap() {
        serde_json::Value::Number(num) => {
            if num.as_i64().unwrap_or(0) == 1 {
                return Ok(cookie);
            } else {
                return Err(content.get("message").unwrap().to_string())?;
            }
        }
       _ => {
           return Err("Connection failure")?;
       }
    }
    Ok("".to_string())
}

fn ui<B: Backend>(f: &mut Frame<B>, app: &App) {
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .margin(2)
        .constraints(
            [
                Constraint::Length(1),
                Constraint::Length(3),
                Constraint::Length(3),
                Constraint::Min(5),
            ]
            .as_ref(),
        )
        .split(f.size());

    let (msg, style) = match app.input_mode {
        InputMode::Normal => (
            vec![
                Span::raw("Press "),
                Span::styled("q", Style::default().add_modifier(Modifier::BOLD)),
                Span::raw(" to exit, "),
                Span::styled("e", Style::default().add_modifier(Modifier::BOLD)),
                Span::raw(" to start editing, "),
                Span::styled("tab", Style::default().add_modifier(Modifier::BOLD)),
                Span::raw(" to change row, "),
                Span::styled("enter", Style::default().add_modifier(Modifier::BOLD)),
                Span::raw(" to submit."),
            ],
            Style::default().add_modifier(Modifier::RAPID_BLINK),
        ),
        InputMode::EditingRow1 | InputMode::EditingRow2 => (
            vec![
                Span::raw("Press "),
                Span::styled("Esc", Style::default().add_modifier(Modifier::BOLD)),
                Span::raw(" to stop editing, "),
                Span::styled("Enter", Style::default().add_modifier(Modifier::BOLD)),
                Span::raw(" to record the message"),
            ],
            Style::default(),
        ),
    };
    let mut text = Text::from(Spans::from(msg));
    text.patch_style(style);
    let help_message = Paragraph::new(text);
    f.render_widget(help_message, chunks[0]);

    let input = Paragraph::new(app.username.as_ref())
        .style(match app.input_mode {
            InputMode::Normal => Style::default(),
            InputMode::EditingRow1 => Style::default().fg(Color::Yellow),
            InputMode::EditingRow2 => Style::default(),
        })
        .block(Block::default().borders(Borders::ALL).title("Username"));
    f.render_widget(input, chunks[1]);

    let passowrd_input = Paragraph::new(app.password_input.as_ref())
        .style(match app.input_mode {
            InputMode::Normal => Style::default(),
            InputMode::EditingRow1 => Style::default(),
            InputMode::EditingRow2 => Style::default().fg(Color::Yellow),
        })
        .block(Block::default().borders(Borders::ALL).title("password"));
    f.render_widget(passowrd_input, chunks[2]);

    if app.show_pop {
        let create_block = |title| {
            Block::default()
                .borders(Borders::ALL)
                .style(Style::default().bg(Color::White).fg(Color::Black))
                .title(Span::styled(
                    title,
                    Style::default().add_modifier(Modifier::BOLD),
                ))
        };

        let text = vec![
            Spans::from(Span::styled(
                &app.pop_message,
                Style::default().fg(Color::Red),
            )),
        ];

        let size = f.size();
        let block = Paragraph::new(text.clone())
        .style(Style::default().bg(Color::White).fg(Color::Black))
        .block(create_block("Warnings"))
        .alignment(Alignment::Left);
        let area = centered_rect(60, 20, size);
        f.render_widget(Clear, area); //this clears out the background
        f.render_widget(block, area);
    }

    match app.input_mode {
        InputMode::Normal =>
            // Hide the cursor. `Frame` does this by default, so we don't need to do anything here
            {}

        InputMode::EditingRow1 => {
            // Make the cursor visible and ask tui-rs to put it at the specified coordinates after rendering
            f.set_cursor(
                // Put cursor past the end of the input text
                chunks[1].x + app.username.width() as u16 + 1,
                // Move one line down, from the border to the input line
                chunks[1].y + 1,
            )
        }

        InputMode::EditingRow2 => {
            // Make the cursor visible and ask tui-rs to put it at the specified coordinates after rendering
            f.set_cursor(
                // Put cursor past the end of the input text
                chunks[2].x + app.password_input.width() as u16 + 1,
                // Move one line down, from the border to the input line
                chunks[2].y + 1,
            )
        }
    }
}

fn statistics_ui<B: Backend>(f: &mut Frame<B>, app: &mut App) {
    let size = f.size();
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .margin(5)
        .constraints([Constraint::Length(3), Constraint::Min(0)].as_ref())
        .split(size);

    let block = Block::default().style(Style::default().bg(Color::White).fg(Color::Black));
    f.render_widget(block, size);
    let titles = app
        .titles
        .iter()
        .map(|t| {
            let (first, rest) = t.split_at(1);
            Spans::from(vec![
                Span::styled(first, Style::default().fg(Color::Yellow)),
                Span::styled(rest, Style::default().fg(Color::Green)),
            ])
        })
        .collect();
    let tabs = Tabs::new(titles)
        .block(Block::default().borders(Borders::ALL).title("Tabs"))
        .select(app.index)
        .style(Style::default().fg(Color::Cyan))
        .highlight_style(
            Style::default()
                .add_modifier(Modifier::BOLD)
                .bg(Color::Black),
        );
    f.render_widget(tabs, chunks[0]);

    let selected_style = Style::default().add_modifier(Modifier::REVERSED);
    let normal_style = Style::default().bg(Color::Blue);
    let header_cells = ["IP", "Name", "CPU", "Memory", "Disk", "Package manager", "Tem", "last updated", "System version"]
        .iter()
        .map(|h| Cell::from(*h).style(Style::default().fg(Color::Red)));
    let header = Row::new(header_cells)
        .style(normal_style)
        .height(1)
        .bottom_margin(1);
    let format_bytes = |bytes:f64| {
        if bytes < 1024.0 {
            return format!("{} B", bytes);
        } else if bytes < 1024.0 * 1024.0 {
            return format!("{:.2} KB", bytes as f64 / 1024.0);
        } else if bytes < 1024.0 * 1024.0 * 1024.0 {
            return format!("{:.2} MB", bytes as f64 / (1024.0 * 1024.0));
        } else {
            return format!("{:.2} GB", bytes as f64 / (1024.0 * 1024.0 * 1024.0));
        }
    };
    let rows = app.items.iter().map(|item| {
        let default = "".to_string();
        let cpu = if item.is_online == 1 {
            format!("{}%", ((item.cpu_user.unwrap_or_default() + item.cpu_system.unwrap_or_default()) * 10000.0).round() / 100.0 )
        } else {
            default.clone()
        };
        let memory = if item.is_online == 1 {
            let memory_total = item.memory_total.unwrap_or_default();
            let memory_free = item.memory_free.unwrap_or_default();
            format!("{}/{}({}%)", format_bytes(memory_total - memory_free), format_bytes(memory_total), ((memory_total - memory_free)/memory_total*10000.0).round() / 100.0 )
        } else {
            default.clone()
        };
        let disk = if item.is_online == 1 && item.disk_total.unwrap_or_default() > 0.0 {
            let disk_total = item.disk_total.unwrap_or_default();
            let disk_avail = item.disk_avail.unwrap_or_default();
            format!("{}/{}({}%)", format_bytes(disk_total - disk_avail), format_bytes(disk_total), ((disk_total - disk_avail) / disk_total * 10000.0).round() / 100.0)
        } else {
            default.clone()
        };
        let cells = vec![
            Cell::from(item.client_ip.clone().unwrap_or_default()),    
            Cell::from(item.name.clone().unwrap_or_default()),    
            Cell::from(cpu),    
            Cell::from(memory),    
            Cell::from(disk),    
            Cell::from(item.package_manager_update_count.to_string()),    
            Cell::from(item.cpu_temp.unwrap_or_default().to_string()),    
            Cell::from(item.last_online_time.clone().unwrap_or_default()),    
            Cell::from(item.system_version.clone().unwrap_or_default()),    
        ];
        let style = match item.is_online {
            0 => Style::default().bg(Color::Gray),
            _ => Style::default(),
        };
        Row::new(cells).style(style).bottom_margin(1)
    });
    let t = Table::new(rows)
        .header(header)
        .block(Block::default().borders(Borders::ALL).title("Table"))
        .highlight_style(selected_style)
        .highlight_symbol("> ")
        .widths(&[
            Constraint::Percentage(10),
            Constraint::Percentage(10),
            Constraint::Percentage(7),
            Constraint::Percentage(15),
            Constraint::Percentage(15),
            Constraint::Percentage(7),
            Constraint::Percentage(7),
            Constraint::Percentage(10),
            Constraint::Percentage(15),
        ]);
    f.render_stateful_widget(t, chunks[1], &mut app.state);
}

fn warning(app:&mut App, msg: String) {
    app.show_pop = true;
    app.pop_message = msg;
}

fn centered_rect(percent_x: u16, percent_y: u16, r: Rect) -> Rect {
    let popup_layout = Layout::default()
        .direction(Direction::Vertical)
        .constraints(
            [
                Constraint::Percentage((100 - percent_y) / 2),
                Constraint::Percentage(percent_y),
                Constraint::Percentage((100 - percent_y) / 2),
            ]
            .as_ref(),
        )
        .split(r);

    Layout::default()
        .direction(Direction::Horizontal)
        .constraints(
            [
                Constraint::Percentage((100 - percent_x) / 2),
                Constraint::Percentage(percent_x),
                Constraint::Percentage((100 - percent_x) / 2),
            ]
            .as_ref(),
        )
        .split(popup_layout[1])[1]
}
