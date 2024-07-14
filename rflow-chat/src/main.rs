use std::io::Write;
use std::thread;
use std::time::Duration;

use clap::Parser;
use cursive::theme::{BaseColor, Color, Palette, PaletteColor, Theme};
use cursive::utils::span::SpannedString;
use cursive::view::{Nameable, Resizable, Scrollable};
use cursive::views::{Dialog, EditView, LinearLayout, NamedView, ScrollView, TextView};
use cursive::Cursive;
use cursive::CursiveExt;

const COLOR_CLIENT_TO_SERVER: Color = Color::Light(BaseColor::Blue);
const COLOR_SERVER_TO_CLIENT: Color = Color::TerminalDefault;
const COLOR_ERROR: Color = Color::Light(BaseColor::Red);
const COLOR_OK: Color = Color::Dark(BaseColor::Green);

#[derive(Parser)]
struct Args {
    #[clap(help = "HOST[:PORT], the default port is 4001")]
    server: String,
    #[clap(long, default_value = "5")]
    timeout: u16,
    #[clap(long, default_value = ":")]
    command_prefix: String,
}

macro_rules! append_chat_msg {
    ($siv: expr, $msg: expr, $color: expr) => {
        $siv.call_on_name("chat", |view: &mut TextView| {
            view.append(SpannedString::styled($msg, $color));
        })
        .unwrap();
        $siv.call_on_name(
            "chat-scroll",
            |view: &mut ScrollView<NamedView<TextView>>| {
                if (view.is_at_bottom()) {
                    view.scroll_to_bottom();
                }
            },
        )
        .unwrap();
        $siv.call_on_name("chat", |view: &mut TextView| {
            view.append("");
        })
        .unwrap();
    };
}

fn handle_input(siv: &mut Cursive, text: &str, command_prefix: &str, client: &rflow::Client) {
    if let Some(internal_command) = text.strip_prefix(command_prefix) {
        let mut sp = internal_command.split_whitespace();
        let mut error_msg: Option<String> = None;
        let mut ok_msg: Option<String> = None;
        match sp.next().unwrap() {
            "w" => {
                if let Some(file_name) = sp.next() {
                    siv.call_on_name(
                        "chat",
                        |view: &mut TextView| match std::fs::OpenOptions::new()
                            .append(false)
                            .create(true)
                            .write(true)
                            .truncate(true)
                            .open(file_name)
                        {
                            Ok(mut file) => {
                                for span in view.get_content().spans() {
                                    if !span.content.is_empty() {
                                        if let Err(e) = file.write_all(span.content.as_bytes()) {
                                            error_msg =
                                                Some(format!("Error writing to file: {}\n", e));
                                            break;
                                        }
                                    }
                                }
                                if error_msg.is_none() {
                                    ok_msg = Some(format!(
                                        "Chat buffer has been saved to: {}\n",
                                        file_name
                                    ));
                                }
                            }
                            Err(e) => {
                                error_msg = Some(format!("Error opening file: {}\n", e));
                            }
                        },
                    );
                } else {
                    append_chat_msg!(siv, "No file name specified\n", COLOR_ERROR);
                }
            }
            "q" => {
                siv.quit();
            }
            "c" => {
                siv.call_on_name("chat", |view: &mut TextView| {
                    view.set_content("");
                });
            }
            _ => {
                append_chat_msg!(
                    siv,
                    format!("Unknown command: {}\n", internal_command),
                    COLOR_ERROR
                );
            }
        }
        if let Some(msg) = ok_msg {
            append_chat_msg!(siv, msg, COLOR_OK);
        }
        if let Some(msg) = error_msg {
            append_chat_msg!(siv, msg, COLOR_ERROR);
        }
    } else if let Err(e) = client.try_send(text) {
        append_chat_msg!(siv, format!("{}\n", e), COLOR_ERROR);
    }
    siv.call_on_name("input", |view: &mut EditView| {
        view.set_content("");
    });
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args = Args::parse();
    let mut server = args.server;
    if !server.contains(':') {
        server = format!("{}:4001", server);
    }

    let (client, rx) = rflow::Client::connect_with_options(
        &server,
        &rflow::ConnectionOptions::new().timeout(Duration::from_secs(args.timeout.into())),
    )?;
    let mut siv = Cursive::default();
    let mut palette = Palette::default();
    palette[PaletteColor::Background] = Color::TerminalDefault;
    palette[PaletteColor::View] = Color::TerminalDefault;
    palette[PaletteColor::Primary] = Color::TerminalDefault;
    palette[PaletteColor::Secondary] = Color::TerminalDefault;
    palette[PaletteColor::Tertiary] = Color::TerminalDefault;
    palette[PaletteColor::TitlePrimary] = Color::TerminalDefault;
    palette[PaletteColor::TitleSecondary] = Color::TerminalDefault;
    palette[PaletteColor::Highlight] = Color::TerminalDefault;
    palette[PaletteColor::HighlightInactive] = Color::TerminalDefault;

    siv.set_theme(Theme {
        shadow: false,
        borders: cursive::theme::BorderStyle::Simple,
        palette,
    });

    let chat_log = TextView::new("");
    let input = EditView::new()
        .on_submit(move |s, text| {
            if !text.is_empty() {
                handle_input(s, text, &args.command_prefix, &client);
            }
        })
        .with_name("input");

    let chat_layout = LinearLayout::vertical()
        .child(
            chat_log
                .with_name("chat")
                .scrollable()
                .with_name("chat-scroll")
                .full_height(),
        )
        .child(input)
        .full_screen();

    siv.add_fullscreen_layer(Dialog::around(chat_layout).title(format!("{} - rflow", server)));

    let cb_sink = siv.cb_sink().clone();

    thread::spawn(move || {
        macro_rules! append_msg {
            ($msg: expr, $color: expr) => {
                cb_sink
                    .send(Box::new(move |s| {
                        append_chat_msg!(s, $msg, $color);
                    }))
                    .unwrap();
            };
        }
        for (direction, msg) in rx {
            let color = match direction {
                rflow::Direction::ClientToServer => COLOR_CLIENT_TO_SERVER,
                rflow::Direction::ServerToClient => COLOR_SERVER_TO_CLIENT,
                rflow::Direction::Last => unreachable!(),
            };
            append_msg!(format!("{} {}\n", direction.as_char(), msg), color);
        }
        append_msg!("Server connection closed\n", COLOR_ERROR);
    });

    siv.run();
    Ok(())
}
