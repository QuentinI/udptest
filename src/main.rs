#![warn(missing_debug_implementations, missing_docs, rust_2018_idioms)]
//! A small application capable of sending some bogus
//! data read from sqlite database through UDP, while
//! presenting a basic GUI.

/// GUI and piecing it all together
mod app;
/// Data format and DB transactions
mod record;
/// UDP transmission
mod udp;

fn main() {
    app::run();
}
