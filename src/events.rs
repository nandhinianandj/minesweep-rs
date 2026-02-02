use std::{sync::mpsc, thread, time::Duration};
use crossterm::event::{self, Event as CEvent, KeyEvent};

pub(crate) enum Event<I> {
    Input(I),
    Tick,
}

/// A small event handler that wrap crossterm input and tick events. Each event
/// type is handled in its own thread and returned to a common `Receiver`
pub(crate) struct Events {
    rx: mpsc::Receiver<Event<KeyEvent>>,
    _input_handle: thread::JoinHandle<()>,
    _tick_handle: thread::JoinHandle<()>,
}

#[derive(Debug, Clone, Copy)]
pub(crate) struct Config {
    tick_rate: Duration,
}

impl Config {
    fn new() -> Self {
        Self {
            tick_rate: Duration::from_millis(250),
        }
    }
}

impl Events {
    pub(crate) fn new() -> Self {
        Self::with_config(Config::new())
    }

    pub(crate) fn with_config(config: Config) -> Self {
        let (tx, rx) = mpsc::channel();
        Self {
            rx,
            _input_handle: {
                let tx = tx.clone();
                thread::spawn(move || {
                    loop {
                        if let Ok(event) = event::read() {
                            if let CEvent::Key(key) = event {
                                if let Err(err) = tx.send(Event::Input(key)) {
                                    eprintln!("{err}");
                                    return;
                                }
                            }
                        }
                    }
                })
            },
            _tick_handle: {
                thread::spawn(move || loop {
                    if tx.send(Event::Tick).is_err() {
                        break;
                    }
                    thread::sleep(config.tick_rate);
                })
            },
        }
    }

    pub(crate) fn next(&self) -> Result<Event<KeyEvent>, mpsc::RecvError> {
        self.rx.recv()
    }
}
