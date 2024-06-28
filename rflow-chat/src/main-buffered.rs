use std::collections::VecDeque;
use std::thread;
use std::time::Duration;

use clap::Parser;
use cursive::theme::{BaseColor, Color, Palette, PaletteColor, Theme};
use cursive::utils::span::SpannedString;
use cursive::view::{Nameable, Resizable, Scrollable};
use cursive::views::{Dialog, EditView, LinearLayout, NamedView, ScrollView};
use cursive::CursiveExt;
use cursive::{Cursive, Printer, View};

enum LineStyle {
    Incoming,
    Outgoing,
    Error,
}

struct LineData {
    style: LineStyle,
    text: String,
}

struct BufferView {
    buffer: VecDeque<LineData>,
    max_lines: usize,
}

impl BufferView {
    fn new(max_lines: usize) -> Self {
        Self {
            buffer: VecDeque::new(),
            max_lines,
        }
    }
    fn append(&mut self, style: LineStyle, text: String) {
        if self.buffer.len() >= self.max_lines {
            self.buffer.pop_front();
        }
        self.buffer.push_back(LineData { style, text });
    }
}

impl View for BufferView {
    fn draw(&self, printer: &Printer) {
        dbg!(printer.size, printer.offset, printer.content_offset);
        for (i, line) in self.buffer.iter().enumerate() {
            let color = match line.style {
                LineStyle::Incoming => Color::Light(BaseColor::Blue),
                LineStyle::Outgoing => Color::Dark(BaseColor::White),
                LineStyle::Error => Color::Light(BaseColor::Red),
            };
            printer.print_styled(
                (0, printer.size.y - 1 + i),
                &SpannedString::styled(&line.text, color),
            );
        }
    }
}

#[derive(Parser)]
struct Args {
    #[clap()]
    server: String,
    #[clap(long, default_value = "5")]
    timeout: u16,
    #[clap(long, default_value = "5")]
    buffer: usize,
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args = Args::parse();

    let buffer_len = args.buffer;

    let (client, rx) = rflow::Client::connect_with_options(
        &args.server,
        &rflow::ConnectionOptions::new().timeout(Duration::from_secs(args.timeout.into())),
    )?;
    let mut siv = Cursive::default();
    let mut palette = Palette::default();
    palette[PaletteColor::Background] = Color::Dark(BaseColor::Black);
    palette[PaletteColor::View] = Color::Dark(BaseColor::Black);
    palette[PaletteColor::Primary] = Color::Dark(BaseColor::White);
    palette[PaletteColor::Secondary] = Color::Dark(BaseColor::White);
    palette[PaletteColor::Tertiary] = Color::Dark(BaseColor::White);
    palette[PaletteColor::TitlePrimary] = Color::Light(BaseColor::White);
    palette[PaletteColor::TitleSecondary] = Color::Light(BaseColor::White);
    palette[PaletteColor::Highlight] = Color::Dark(BaseColor::Blue);
    palette[PaletteColor::HighlightInactive] = Color::Dark(BaseColor::Blue);

    siv.set_theme(Theme {
        shadow: false,
        borders: cursive::theme::BorderStyle::Simple,
        palette,
    });

    macro_rules! append_chat_msg {
        ($siv: expr, $style: expr, $msg: expr) => {
            $siv.call_on_name("chat", |view: &mut BufferView| {
                view.append($style, $msg);
            })
            .unwrap();
            //$siv.call_on_name(
                //"chat-scroll",
                //|view: &mut ScrollView<NamedView<BufferView>>| {
                    //view.scroll_to_bottom();
                //},
            //)
            //.unwrap();
        };
    }

    let chat_log = BufferView::new(buffer_len);
    let input = EditView::new()
        .on_submit(move |s, text| {
            if !text.is_empty() {
                if let Some(internal_command) = text.strip_prefix(':') {
                    match internal_command.trim_end() {
                        "q" => {
                            s.quit();
                        }
                        "c" => {
                            s.call_on_name("chat", |view: &mut BufferView| {
                                //view.("");
                            });
                        }
                        _ => {
                            append_chat_msg!(
                                s,
                                LineStyle::Error,
                                format!("Unknown command: {}\n", internal_command)
                            );
                        }
                    }
                } else if let Err(e) = client.try_send(text) {
                    append_chat_msg!(s, LineStyle::Error, format!("{}\n", e));
                }
                s.call_on_name("input", |view: &mut EditView| {
                    view.set_content("");
                });
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

    siv.add_fullscreen_layer(Dialog::around(chat_layout).title(format!("{} - rflow", args.server)));

    let cb_sink = siv.cb_sink().clone();

    thread::spawn(move || {
        macro_rules! append_msg {
            ($style: expr, $msg: expr) => {
                cb_sink
                    .send(Box::new(move |s| {
                        append_chat_msg!(s, $style, $msg);
                    }))
                    .unwrap();
            };
        }
        for (direction, msg) in rx {
            let style = match direction {
                rflow::Direction::Incoming => LineStyle::Incoming,
                rflow::Direction::Outgoing => LineStyle::Outgoing,
                _ => unreachable!(),
            };
            append_msg!(style, format!("{} {}\n", direction.as_char(), msg));
        }
        append_msg!(LineStyle::Error, "Server connection closed\n".to_owned());
    });

    siv.run();
    Ok(())
}
