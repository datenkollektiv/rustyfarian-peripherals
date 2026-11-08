//! Command tables and byte builders for the SSD1306 and SH1106.
//!
//! Every entry is one command with its arguments; the bus layer sends each as
//! a single write behind the command control byte.
//! Neither table contains display-on (`0xAF`): the runner sends it after the
//! first full flush so the first visible frame is the application's buffer.

/// Control byte announcing that command bytes follow.
pub(crate) const CONTROL_COMMAND: u8 = 0x00;
/// Control byte announcing that display-RAM data bytes follow.
pub(crate) const CONTROL_DATA: u8 = 0x40;
/// Display on (resume from sleep).
pub(crate) const DISPLAY_ON: u8 = 0xAF;
/// No-operation command, used as a bus liveness probe.
pub(crate) const NOP: u8 = 0xE3;

/// SSD1306 128x64 init: page addressing mode and the internal charge pump.
pub(crate) const SSD1306_INIT: &[&[u8]] = &[
    &[0xAE],
    &[0xD5, 0x80],
    &[0xA8, 0x3F],
    &[0xD3, 0x00],
    &[0x40],
    &[0x8D, 0x14],
    &[0x20, 0x02],
    &[0xA1],
    &[0xC8],
    &[0xDA, 0x12],
    &[0x81, 0xCF],
    &[0xD9, 0xF1],
    &[0xDB, 0x40],
    &[0xA4],
    &[0xA6],
];

/// SH1106 128x64 init: internal DC-DC converter on; page addressing is the only mode.
pub(crate) const SH1106_INIT: &[&[u8]] = &[
    &[0xAE],
    &[0xD5, 0x80],
    &[0xA8, 0x3F],
    &[0xD3, 0x00],
    &[0x40],
    &[0xAD, 0x8B],
    &[0xA1],
    &[0xC8],
    &[0xDA, 0x12],
    &[0x81, 0xCF],
    &[0xD9, 0x22],
    &[0xDB, 0x35],
    &[0xA4],
    &[0xA6],
];

/// Builds the four-byte write that selects `page` and the start `column`.
///
/// `page` is masked to three bits, so no input can build an invalid command.
pub(crate) const fn set_page_column(page: u8, column: u8) -> [u8; 4] {
    [
        CONTROL_COMMAND,
        0xB0 | (page & 0x07),
        column & 0x0F,
        0x10 | ((column >> 4) & 0x0F),
    ]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn page_column_bytes() {
        assert_eq!(set_page_column(0, 0), [0x00, 0xB0, 0x00, 0x10]);
        assert_eq!(set_page_column(3, 2), [0x00, 0xB3, 0x02, 0x10]);
        assert_eq!(set_page_column(0, 130), [0x00, 0xB0, 0x02, 0x18]);
    }

    #[test]
    fn page_is_masked() {
        assert_eq!(set_page_column(9, 0)[1], 0xB1);
    }

    #[test]
    fn tables_start_with_display_off_and_never_turn_on() {
        for table in [SSD1306_INIT, SH1106_INIT] {
            assert_eq!(table[0], &[0xAE]);
            assert!(table.iter().all(|c| c[0] != DISPLAY_ON));
        }
    }
}
