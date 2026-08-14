use std::io::{self, stdout};
use std::process::ExitCode;
use std::time::Duration;

use clap::Parser;
use crossterm::event::{self, Event, KeyEventKind};
use crossterm::execute;
use crossterm::terminal::{
    EnterAlternateScreen, LeaveAlternateScreen, disable_raw_mode, enable_raw_mode,
};
use ratatui::prelude::*;

use music_player::app::App;
use music_player::cli::{Cli, validate_directory};
use music_player::config::{AppConfig, AppPaths};
use music_player::ui;

fn main() -> ExitCode {
    match run_application() {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("music-player: {error}");
            ExitCode::FAILURE
        }
    }
}

fn run_application() -> Result<(), String> {
    let cli = Cli::parse();
    let paths = AppPaths::discover().map_err(|error| format!("无法确定用户数据目录: {error}"))?;
    let (mut config, mut warning) = AppConfig::load(&paths.config_file)
        .map_err(|error| format!("无法读取配置 {}: {error}", paths.config_file.display()))?;
    let mut save_config_on_exit = warning.is_none();

    let library_dir = if let Some(path) = cli.set_library {
        let path = validate_directory(&path).map_err(|error| error.to_string())?;
        config.library_dir = Some(path.clone());
        config
            .save(&paths.config_file)
            .map_err(|error| format!("无法保存主音乐库设置: {error}"))?;
        warning = Some(format!("主音乐库已设置为 {}", path.display()));
        save_config_on_exit = true;
        path
    } else if let Some(path) = cli.directory {
        validate_directory(&path).map_err(|error| error.to_string())?
    } else if let Some(path) = config.library_dir.as_ref() {
        validate_directory(path).map_err(|error| {
            format!("配置的主音乐库不可用: {error}。请使用 --set-library PATH 设置新目录")
        })?
    } else {
        let path = paths.default_music_dir.as_ref().ok_or_else(|| {
            "系统没有提供 XDG Music 目录，请使用 --set-library PATH 设置主音乐库".to_owned()
        })?;
        validate_directory(path).map_err(|error| {
            format!("默认 XDG Music 目录不可用: {error}。请使用 --set-library PATH 设置主音乐库")
        })?
    };

    // GStreamer、播放列表目录及工作线程都在切换终端模式前初始化。
    // 这样启动失败时错误仍是普通、可复制的终端文本。
    let mut app = App::new(library_dir, paths, config, warning, save_config_on_exit)?;
    let mut terminal = setup_terminal().map_err(|error| format!("无法初始化终端: {error}"))?;
    let run_result = run(&mut terminal, &mut app).map_err(|error| format!("终端运行失败: {error}"));
    let restore_result =
        restore_terminal(&mut terminal).map_err(|error| format!("无法恢复终端状态: {error}"));
    let save_result = app.save_settings();

    run_result?;
    restore_result?;
    save_result?;
    Ok(())
}

fn run(terminal: &mut Terminal<CrosstermBackend<io::Stdout>>, app: &mut App) -> io::Result<()> {
    while !app.should_quit {
        terminal.draw(|frame| ui::draw(frame, app))?;
        if event::poll(Duration::from_millis(50))?
            && let Event::Key(key) = event::read()?
            && key.kind == KeyEventKind::Press
        {
            app.handle_key(key);
        }
        app.on_tick();
    }
    Ok(())
}

fn setup_terminal() -> io::Result<Terminal<CrosstermBackend<io::Stdout>>> {
    let original_hook = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |info| {
        let _ = disable_raw_mode();
        let _ = execute!(stdout(), LeaveAlternateScreen);
        original_hook(info);
    }));

    enable_raw_mode()?;
    let mut output = stdout();
    execute!(output, EnterAlternateScreen)?;
    Terminal::new(CrosstermBackend::new(output))
}

fn restore_terminal(terminal: &mut Terminal<CrosstermBackend<io::Stdout>>) -> io::Result<()> {
    disable_raw_mode()?;
    execute!(terminal.backend_mut(), LeaveAlternateScreen)?;
    terminal.show_cursor()
}
