use crate::wasi::logging::logging::log;

use crate::exports::betty_blocks::logging::logger;
use simd_json::prelude::Writable;

// with: { "wasi:logging/logging@0.1.0-draft": generate, }
wit_bindgen::generate!({ generate_all });

struct Logger;

impl logger::Guest for Logger {
    fn log(severity: logger::Level, mut variables: logger::JsonString) -> Result<(), String> {
        let mut var_bytes = unsafe { variables.as_bytes_mut() };
        let tape = simd_json::to_tape(&mut var_bytes).map_err(|e| e.to_string())?;
        let value = tape.as_value();

        let map = value
            .as_object()
            .ok_or_else(|| "expected log variables to be an object/map".to_string())?;
        for (key, item) in map.iter() {
            let item = item.encode();
            log(severity, "stdout", &format!("{key} : {item}"));
        }

        Ok(())
    }
}

// impl From<logger::Level> for tracing::Level {
//     fn from(level: logger::Level) -> tracing::Level {
//         match level {
//             logger::Level::Trace => tracing::Level::TRACE,
//             logger::Level::Debug => tracing::Level::DEBUG,
//             logger::Level::Info => tracing::Level::INFO,
//             logger::Level::Warn => tracing::Level::WARN,
//             logger::Level::Error => tracing::Level::ERROR,
//             logger::Level::Critical => tracing::Level::ERROR,
//         }
//     }
// }

// impl Guest for Logger {
//     fn log(severity: logger::Level, mut variables: JsonString) -> Result<(), String> {
//         // let level = tracing::Level::from(severity);
//         println!("hallo");

//         let mut var_bytes = unsafe { variables.as_bytes_mut() };
//         let tape = simd_json::to_tape(&mut var_bytes).map_err(|e| e.to_string())?;
//         let value = tape.as_value();

//         let map = value
//             .as_object()
//             .ok_or_else(|| "expected log variables to be an object/map".to_string())?;
//         for (key, item) in map.iter() {
//             let item = item.into_string().expect("we just decoded this from json");
//             // event!(severity, &format!("{key} : {item}"));
//             inner_log(severity, &format!("{key} : {item}"));
//         }

//         Ok(())
//     }
// }

// fn inner_log(severity: logger::Level, data: &str) {
//     match severity {
//         logger::Level::Trace => tracing::trace!(data),
//         logger::Level::Debug => tracing::debug!(data),
//         logger::Level::Info => tracing::info!(data),
//         logger::Level::Warn => tracing::warn!(data),
//         logger::Level::Error | logger::Level::Critical => tracing::error!(data),
//     }
// }

export! {Logger}
