use std::fs::OpenOptions;
use std::io::Write;
use log::{Level, LevelFilter, Metadata, Record, SetLoggerError};

pub struct SimpleLogger {
    file: std::fs::File,
}

impl SimpleLogger {
    pub fn new(file_path: &str) -> SimpleLogger {
        let file = OpenOptions::new()
            .write(true)
            .create(true)
            .open(file_path)
            .unwrap();
        SimpleLogger { file }
    }
}


impl log::Log for SimpleLogger {
    
    fn enabled(&self, metadata: &Metadata) -> bool {
        metadata.level() <= Level::Debug
    }

    fn log(&self, record: &Record) {
        if self.enabled(record.metadata()) {
            println!("{} - {}", record.level(), record.args());
            writeln!(&self.file, "{} - {}", record.level(), record.args()).unwrap();
        }
    }

    fn flush(&self) {}
}

pub fn init_logging(file_path: &str) -> Result<(), SetLoggerError> {
    log::set_boxed_logger(Box::new(SimpleLogger::new(file_path))).map(|()| log::set_max_level(LevelFilter::Info))
}
