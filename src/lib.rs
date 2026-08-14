#[macro_use]
extern crate tracing;

#[cfg(feature = "dbus")]
pub mod a11y;
pub mod animation;
pub mod backend;
pub mod cli;
pub mod cursor;
pub mod cursor_scale;
#[cfg(feature = "dbus")]
pub mod dbus;
pub mod frame_clock;
pub mod handlers;
pub mod input;
pub mod ipc;
pub mod layer;
pub mod layout;
pub mod niri;
pub mod protocols;
pub mod render_helpers;
pub mod rubber_band;
#[cfg(feature = "xdp-gnome-screencast")]
pub mod screencasting;
pub mod ui;
pub mod utils;
pub mod window;

pub mod debug_logger {
    use std::io::{BufWriter, Write};
    use std::os::unix::net::{UnixListener, UnixStream};
    use std::sync::{Arc, Mutex};
    use std::{fs, io, thread};
    use std::fs::OpenOptions;
    use std::path::Path;
    use std::sync::mpsc::{channel, Sender};
    use once_cell::sync::Lazy;

    pub static GENERAL: Lazy<AsyncDebugLogger> =
        Lazy::new(|| AsyncDebugLogger::new("/tmp/niri-debug-general.socket").unwrap());
    pub static EVENT: Lazy<AsyncDebugLogger> =
        Lazy::new(|| AsyncDebugLogger::new("/tmp/niri-debug-event.socket").unwrap());


    pub fn init_all() {
        GENERAL.log("init");
        EVENT.log("init");
    }

    pub struct AsyncDebugLogger {
        client: Arc<Mutex<Option<UnixStream>>>,
    }

    impl AsyncDebugLogger {
        pub fn new(path: &str) -> io::Result<Self> {
            let _ = fs::remove_file(path);
            let listener = UnixListener::bind(path)?;
            let client_arc = Arc::new(Mutex::new(None));
            let thread_client = client_arc.clone();

            thread::spawn(move || {
                for stream in listener.incoming() {
                    match stream {
                        Ok(s) => {
                            let _ = s.set_nonblocking(true);
                            let mut lock = thread_client.lock().unwrap();
                            *lock = Some(s);
                            println!("[Debug] New debugger attached.");
                        }
                        Err(e) => eprintln!("[Debug] Accept error: {}", e),
                    }
                }
            });

            Ok(AsyncDebugLogger { client: client_arc })
        }

        fn write_bytes(&self, bytes: &[u8]) {
            let mut client_lock = self.client.lock().unwrap();

            if let Some(ref mut stream) = *client_lock {
                let result = stream.write_all(bytes);
                match result {
                    Ok(_) => {}
                    Err(ref e) if e.kind() == io::ErrorKind::WouldBlock => {}
                    Err(_) => {
                        *client_lock = None;
                        println!("[Debug] Debugger detached.");
                    }
                }
            }
        }

        pub fn log(&self, msg: impl AsRef<str>) {
            self.write_bytes(msg.as_ref().as_bytes());
            self.write_bytes(b"\n");
        }
    }

    const EVENT_OUTPUT_PATH: &str = "/home/bczhc/.local/state/bczhc/niri-window-activity/niri-event.log";
    pub static NIRI_EVENT_FILE_LOGGER: Lazy<FileLogger> = Lazy::new(|| FileLogger::new(EVENT_OUTPUT_PATH));

    pub struct FileLogger {
        handle: thread::JoinHandle<()>,
        tx: Sender<String>,
    }

    impl FileLogger {
        pub fn new(path: impl AsRef<Path>) -> Self {
            let path = path.as_ref();
            let output = OpenOptions::new()
                .read(true)
                .write(true)
                .truncate(false)
                .append(true)
                .create(true)
                .open(path).unwrap();
            let mut output = BufWriter::new(output);
            let (tx, rx) = channel();

            let handle = thread::spawn(move || {
                for line in rx {
                    use io::Write;
                    if let Err(e) = writeln!(&mut output, "{} {}", time_now_iso_string(), line) {
                        error!("FileLogger write error: {:?}", e);
                    }
                    if let Err(e) = output.flush() {
                        error!("FileLogger write error: {:?}", e);
                    }
                }
            });
            Self { handle, tx }
        }

        #[inline(always)]
        pub fn log(&self, line: impl Into<String>) {
            self.tx.send(line.into()).unwrap();
        }
    }

    fn time_now_iso_string() -> String {
        chrono::Local::now().format("%Y-%m-%dT%H:%M:%S.%f").to_string()
    }
}

#[cfg(test)]
mod tests;
