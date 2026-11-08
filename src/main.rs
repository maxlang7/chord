mod api;
mod app;
mod gui;
mod images;
mod mcp;
mod model;
mod ui;

use anyhow::Result;
use clap::Parser;
use crossterm::{
    event::{
        self, DisableBracketedPaste, EnableBracketedPaste, Event, KeyEventKind,
        KeyboardEnhancementFlags, PopKeyboardEnhancementFlags, PushKeyboardEnhancementFlags,
    },
    execute,
};
use std::{io::Write, path::PathBuf, time::Duration};

#[derive(Parser)]
#[command(version, about = "Independent chat and email, in a native Rust window")]
struct Args {
    /// Use the legacy terminal interface. The native UI is the default.
    #[arg(long)]
    tui: bool,
    #[arg(long, default_value = "http://127.0.0.1:4789")]
    api: String,
    #[arg(long)]
    config: Option<PathBuf>,
    #[arg(long)]
    demo: bool,
    #[arg(long)]
    mcp: bool,
    #[arg(long)]
    snapshot: Option<PathBuf>,
    #[arg(long, default_value_t = 160)]
    width: u16,
    #[arg(long, default_value_t = 50)]
    height: u16,
    #[arg(long)]
    connect: Option<String>,
}
struct Restore;
impl Drop for Restore {
    fn drop(&mut self) {
        let _ = execute!(
            std::io::stdout(),
            PopKeyboardEnhancementFlags,
            DisableBracketedPaste
        );
        ratatui::restore();
    }
}
#[tokio::main]
async fn main() -> Result<()> {
    let args = Args::parse();
    let config = args.config.unwrap_or_else(|| {
        std::env::var_os("CHORD_CONFIG")
            .map(PathBuf::from)
            .unwrap_or_else(|| {
                PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("private/config.json")
            })
    });
    let api = if args.demo {
        None
    } else {
        Some(api::Api::new(&args.api, &config)?)
    };
    if args.mcp {
        return mcp::run(api.expect("MCP requires a live API")).await;
    }
    let mut app = app::App::new(api, args.demo);
    if let Some(id) = args.connect {
        app.start_login(id, None)
    }
    if let Some(file) = args.snapshot {
        if !args.demo {
            for _ in 0..600 {
                app.poll();
                if !app.loading {
                    break;
                }
                tokio::time::sleep(Duration::from_millis(100)).await;
            }
            if let Some(c) = app.selected() {
                app.open(c);
                for _ in 0..600 {
                    app.poll();
                    if !app.message_loading {
                        break;
                    }
                    tokio::time::sleep(Duration::from_millis(100)).await;
                }
            }
        }
        let backend = ratatui::backend::TestBackend::new(args.width, args.height);
        let mut terminal = ratatui::Terminal::new(backend)?;
        terminal.draw(|f| ui::render(f, &app))?;
        let buffer = terminal.backend().buffer();
        let mut text = String::new();
        for y in 0..args.height {
            for x in 0..args.width {
                text.push_str(buffer[(x, y)].symbol());
            }
            text.push('\n');
        }
        let mut options = std::fs::OpenOptions::new();
        options.write(true).create_new(true);
        #[cfg(unix)]
        {
            use std::os::unix::fs::OpenOptionsExt;
            options.mode(0o600);
        }
        options.open(file)?.write_all(text.as_bytes())?;
        return Ok(());
    }
    if !args.tui {
        return gui::run(app);
    }
    let mut terminal = ratatui::init();
    let _restore = Restore;
    app.images.picker = ratatui_image::picker::Picker::from_query_stdio()
        .unwrap_or_else(|_| ratatui_image::picker::Picker::halfblocks());
    if std::env::var("CHORD_IMAGE_PROTOCOL").as_deref() == Ok("halfblocks") {
        app.images
            .picker
            .set_protocol_type(ratatui_image::picker::ProtocolType::Halfblocks);
    }
    execute!(
        std::io::stdout(),
        EnableBracketedPaste,
        PushKeyboardEnhancementFlags(KeyboardEnhancementFlags::DISAMBIGUATE_ESCAPE_CODES)
    )?;
    crossterm::style::force_color_output(true);
    loop {
        app.poll();
        terminal.draw(|f| ui::render(f, &app))?;
        if app.quit {
            break;
        }
        if event::poll(Duration::from_millis(40))? {
            match event::read()? {
                Event::Key(k) if k.kind == KeyEventKind::Press => app.key(k),
                Event::Paste(s) => app.paste(&s),
                _ => {}
            }
        }
    }
    Ok(())
}
