//! Shared host-test helpers: scripted `MockI2c` transactions.

extern crate std;

use std::boxed::Box;
use std::vec::Vec;

use tamer::mock::Transaction;

use crate::command::{set_page_column, CONTROL_DATA};
use crate::Controller;

pub(crate) const ADDR: u8 = 0x3C;

/// A scripted write to the panel (the bytes are leaked; tests are short-lived).
pub(crate) fn write(bytes: &[u8]) -> Transaction<'static> {
    Transaction::Write {
        address: ADDR,
        bytes: Box::leak(bytes.to_vec().into_boxed_slice()),
    }
}

pub(crate) fn probe_txn() -> Transaction<'static> {
    write(&[0x00, 0xE3])
}

pub(crate) fn detect_script(c: Controller) -> Vec<Transaction<'static>> {
    let select = set_page_column(0, 130);
    let mut s = Vec::new();
    for v in [0xA5u8, 0x5A] {
        s.push(write(&select));
        s.push(write(&[CONTROL_DATA, v]));
        s.push(write(&select));
        let response: &'static [u8] = match (c, v) {
            (Controller::Sh1106, 0xA5) => &[0xFF, 0xA5],
            (Controller::Sh1106, _) => &[0xFF, 0x5A],
            (Controller::Ssd1306, _) => &[0x45, 0x45],
        };
        s.push(Transaction::WriteRead {
            address: ADDR,
            write: &[CONTROL_DATA],
            response,
        });
    }
    s
}

pub(crate) fn init_script(c: Controller) -> Vec<Transaction<'static>> {
    c.init_commands()
        .iter()
        .map(|cmd| {
            let mut bytes = Vec::from([0x00u8]);
            bytes.extend_from_slice(cmd);
            write(&bytes)
        })
        .collect()
}

pub(crate) fn page_script(c: Controller, page: u8, data: &[u8; 128]) -> Vec<Transaction<'static>> {
    let mut s = Vec::from([write(&set_page_column(page, c.column_offset()))]);
    for chunk in data.chunks(30) {
        let mut bytes = Vec::from([CONTROL_DATA]);
        bytes.extend_from_slice(chunk);
        s.push(write(&bytes));
    }
    s
}

/// All eight blank pages, as a full flush of an untouched framebuffer.
pub(crate) fn blank_flush(c: Controller) -> Vec<Transaction<'static>> {
    (0..8)
        .flat_map(|p| page_script(c, p, &[0u8; 128]))
        .collect()
}

/// Probe, detect, init, a blank full flush and display-on: a clean attach.
pub(crate) fn attach_script(c: Controller) -> Vec<Transaction<'static>> {
    let mut s = Vec::from([probe_txn()]);
    s.extend(detect_script(c));
    s.extend(init_script(c));
    s.extend(blank_flush(c));
    s.push(write(&[0x00, 0xAF]));
    s
}
