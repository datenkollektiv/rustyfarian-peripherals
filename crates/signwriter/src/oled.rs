//! The blocking runner: bus, framebuffer and hot-plug in one owner.

use core::convert::Infallible;

use embedded_graphics_core::draw_target::DrawTarget;
use embedded_graphics_core::geometry::{OriginDimensions, Size};
use embedded_graphics_core::pixelcolor::BinaryColor;
use embedded_graphics_core::Pixel;
use embedded_hal::i2c::I2c;
use tamer::hotplug::{Hotplug, Transition};

use crate::bus;
use crate::{Controller, Error, Event, Framebuffer, HEIGHT, PAGES, WIDTH};

/// A hot-pluggable 128x64 OLED over an owned I2C bus.
///
/// `Oled` owns the bus `I2C`, a [`Framebuffer`] and a [`Hotplug`] machine.
/// Draw into it as an [`embedded-graphics`](embedded_graphics_core)
/// `DrawTarget`, then call [`tick`](Oled::tick) regularly with a millisecond
/// clock.
/// Drawing is always accepted; while no panel is attached the content waits in
/// the buffer and appears in full on the next attach.
///
/// `tick` works as follows.
///
/// - **Detached:** every `probe_every_ms` it sends a NOP probe and feeds the
///   outcome to [`Hotplug`].
///   When the attach window completes it detects the controller, initialises
///   it, sends the whole buffer and only then turns the display on.
///   If any step fails the attach is dropped and retried from scratch.
/// - **Attached:** it sends the dirty pages and feeds each page's outcome to
///   [`Hotplug`]; on a static screen it sends a NOP probe every
///   `probe_every_ms` so an unplug is noticed.
///   `detach_errors` consecutive failures detach.
///
/// A panel swapped for a different controller between two probes is not
/// noticed; call [`redetect`](Oled::redetect) after such a swap.
///
/// Share a bus with `embedded-hal-bus` if other devices sit on it.
///
/// ```
/// use embedded_graphics_core::{pixelcolor::BinaryColor, prelude::*};
/// use signwriter::{Controller, Event, Oled};
/// use tamer::mock::{MockI2c, MockI2cError, Transaction};
///
/// // No panel: the probe is NACKed and nothing is reported.
/// let script = [Transaction::Error(MockI2cError::nack())];
/// let mut oled = Oled::new(MockI2c::new(&script), 0x3C, 0, 3, 100);
/// assert_eq!(oled.tick(0), None);
/// assert_eq!(oled.controller(), None::<Controller>);
/// assert_eq!(oled.last_error(), Some(&MockI2cError::nack()));
/// oled.release().done();
/// # let _ = (Event::Detached, BinaryColor::On);
/// ```
#[derive(Debug)]
pub struct Oled<I2C: I2c> {
    i2c: I2C,
    address: u8,
    framebuffer: Framebuffer,
    hotplug: Hotplug,
    attach_debounce_ms: u64,
    detach_errors: u32,
    probe_every_ms: u64,
    fixed_controller: Option<Controller>,
    last_bus_ms: Option<u64>,
    controller: Option<Controller>,
    /// A transfer failed while attached; the panel may have lost power.
    needs_reinit: bool,
    last_error: Option<I2C::Error>,
}

impl<I2C: I2c> Oled<I2C> {
    /// Creates a detached `Oled`; nothing is sent until [`tick`](Oled::tick)
    /// or [`redetect`](Oled::redetect).
    ///
    /// - `address`: the panel's 7-bit address, usually [`ADDRESS_PRIMARY`](crate::ADDRESS_PRIMARY).
    /// - `attach_debounce_ms`: how long probes must succeed without a gap
    ///   before the panel counts as attached; `0` attaches on the first success.
    /// - `detach_errors`: consecutive failures that detach an attached panel.
    /// - `probe_every_ms`: the probe interval; `0` probes on every tick.
    ///   Keep it well below `attach_debounce_ms`.
    ///
    /// # Panics
    ///
    /// Panics if `detach_errors == 0`.
    #[must_use]
    pub fn new(
        i2c: I2C,
        address: u8,
        attach_debounce_ms: u64,
        detach_errors: u32,
        probe_every_ms: u64,
    ) -> Self {
        Self::build(
            i2c,
            address,
            None,
            attach_debounce_ms,
            detach_errors,
            probe_every_ms,
        )
    }

    /// Like [`new`](Oled::new), but pins the controller and never detects.
    ///
    /// Use it for a panel that is known in advance, or one whose RAM read-back
    /// misleads [`detect`](crate::detect).
    /// Attach and [`redetect`](Oled::redetect) go straight to init.
    ///
    /// # Panics
    ///
    /// Panics if `detach_errors == 0`.
    #[must_use]
    pub fn with_controller(
        i2c: I2C,
        address: u8,
        controller: Controller,
        attach_debounce_ms: u64,
        detach_errors: u32,
        probe_every_ms: u64,
    ) -> Self {
        Self::build(
            i2c,
            address,
            Some(controller),
            attach_debounce_ms,
            detach_errors,
            probe_every_ms,
        )
    }

    fn build(
        i2c: I2C,
        address: u8,
        fixed_controller: Option<Controller>,
        attach_debounce_ms: u64,
        detach_errors: u32,
        probe_every_ms: u64,
    ) -> Self {
        Self {
            i2c,
            address,
            framebuffer: Framebuffer::new(),
            hotplug: Hotplug::new(attach_debounce_ms, detach_errors),
            attach_debounce_ms,
            detach_errors,
            probe_every_ms,
            fixed_controller,
            last_bus_ms: None,
            controller: None,
            needs_reinit: false,
            last_error: None,
        }
    }

    /// Advances the state machine; call it regularly with a monotonic
    /// millisecond clock.
    ///
    /// Returns an [`Event`] only when attachment changes.
    /// It never fails: bus errors feed the hot-plug machine and are available
    /// through [`last_error`](Oled::last_error).
    #[must_use]
    pub fn tick(&mut self, now_ms: u64) -> Option<Event> {
        match self.controller {
            None => self.tick_detached(now_ms),
            Some(controller) => self.tick_attached(controller, now_ms),
        }
    }

    fn probe_due(&self, now_ms: u64) -> bool {
        self.last_bus_ms
            .is_none_or(|last| now_ms.saturating_sub(last) >= self.probe_every_ms)
    }

    fn tick_detached(&mut self, now_ms: u64) -> Option<Event> {
        if !self.probe_due(now_ms) {
            return None;
        }
        self.last_bus_ms = Some(now_ms);
        let result = bus::probe(&mut self.i2c, self.address);
        let ok = self.record(result);
        if self.hotplug.observe(ok, now_ms) != Some(Transition::Attached) {
            return None;
        }
        match self.attach(now_ms) {
            Ok(controller) => Some(Event::Attached(controller)),
            Err(error) => {
                self.record_error(error);
                self.hotplug.reset();
                None
            }
        }
    }

    fn tick_attached(&mut self, controller: Controller, now_ms: u64) -> Option<Event> {
        if self.needs_reinit {
            return self.tick_reinit(controller, now_ms);
        }
        let mut transition = None;
        if self.framebuffer.dirty_pages() != 0 {
            self.last_bus_ms = Some(now_ms);
            for page in 0..PAGES {
                if self.framebuffer.dirty_pages() & (1 << page) == 0 {
                    continue;
                }
                let result = self.flush_page(controller, page);
                let ok = self.record(result);
                transition = self.hotplug.observe(ok, now_ms);
                if !ok {
                    self.needs_reinit = true;
                    break;
                }
            }
        } else if self.probe_due(now_ms) {
            self.last_bus_ms = Some(now_ms);
            let result = bus::probe(&mut self.i2c, self.address);
            let ok = self.record(result);
            transition = self.hotplug.observe(ok, now_ms);
            self.needs_reinit |= !ok;
        }
        self.detach_if(transition)
    }

    /// After a failed transfer: once the bus answers again, repeat the attach
    /// sequence, because the panel may have power-cycled in the gap.
    fn tick_reinit(&mut self, previous: Controller, now_ms: u64) -> Option<Event> {
        if !self.probe_due(now_ms) {
            return None;
        }
        self.last_bus_ms = Some(now_ms);
        match self.attach(now_ms) {
            Ok(controller) => {
                self.needs_reinit = false;
                self.hotplug.observe(true, now_ms);
                (controller != previous).then_some(Event::Attached(controller))
            }
            Err(error) => {
                self.record_error(error);
                let transition = self.hotplug.observe(false, now_ms);
                self.detach_if(transition)
            }
        }
    }

    fn detach_if(&mut self, transition: Option<Transition>) -> Option<Event> {
        if transition != Some(Transition::Detached) {
            return None;
        }
        self.controller = None;
        self.needs_reinit = false;
        // A forced attach (`redetect`) may have replaced the machine with a
        // zero-debounce one; restore the configured attach debounce.
        self.hotplug = Hotplug::new(self.attach_debounce_ms, self.detach_errors);
        Some(Event::Detached)
    }

    /// Sends every dirty page now.
    ///
    /// For fixed-panel use without [`tick`](Oled::tick); the panel must have
    /// been attached by [`redetect`](Oled::redetect) first.
    /// A page that fails stays dirty, and the next flush resends it in full.
    /// The outcome is not fed to the hot-plug machine.
    ///
    /// # Errors
    ///
    /// Returns [`Error::Detached`], without touching the bus, when no panel is
    /// attached, or [`Error::Bus`] for the first failing page.
    pub fn flush(&mut self) -> Result<(), Error<I2C::Error>> {
        let controller = self.controller.ok_or(Error::Detached)?;
        for page in 0..PAGES {
            if self.framebuffer.dirty_pages() & (1 << page) != 0 {
                self.flush_page(controller, page)?;
            }
        }
        Ok(())
    }

    /// Forces detection, init, a full flush and display-on, whatever
    /// the hot-plug machine currently believes.
    ///
    /// Use it after swapping a panel faster than the probe interval, or to
    /// attach a fixed panel without `tick`.
    /// Returns [`Event::Attached`] when the controller changed or the panel was
    /// detached before, and `None` when the same controller is still there.
    /// If the attempt fails the panel counts as detached: the result is
    /// [`Event::Detached`] if it was attached before and `None` otherwise, and
    /// the cause is in [`last_error`](Oled::last_error).
    #[must_use]
    pub fn redetect(&mut self, now_ms: u64) -> Option<Event> {
        let previous = self.controller;
        self.last_bus_ms = Some(now_ms);
        self.needs_reinit = false;
        match self.attach(now_ms) {
            Ok(controller) => {
                if !self.hotplug.is_attached() {
                    // Attached by force, without the debounce window; the
                    // configured machine returns on the next detach.
                    self.hotplug = Hotplug::new(0, self.detach_errors);
                    let _ = self.hotplug.observe(true, now_ms);
                }
                (previous != Some(controller)).then_some(Event::Attached(controller))
            }
            Err(error) => {
                self.record_error(error);
                self.controller = None;
                self.hotplug = Hotplug::new(self.attach_debounce_ms, self.detach_errors);
                previous.map(|_| Event::Detached)
            }
        }
    }

    /// Detect (unless pinned), init, send the whole buffer, then display-on.
    fn attach(&mut self, now_ms: u64) -> Result<Controller, Error<I2C::Error>> {
        let controller = match self.fixed_controller {
            Some(controller) => controller,
            None => bus::detect(&mut self.i2c, self.address)?,
        };
        bus::init(&mut self.i2c, self.address, controller)?;
        self.framebuffer.mark_all_dirty();
        for page in 0..PAGES {
            self.flush_page(controller, page)?;
        }
        bus::display_on(&mut self.i2c, self.address)?;
        self.controller = Some(controller);
        self.last_error = None;
        self.last_bus_ms = Some(now_ms);
        Ok(controller)
    }

    fn flush_page(&mut self, controller: Controller, page: u8) -> Result<(), Error<I2C::Error>> {
        bus::write_page(
            &mut self.i2c,
            self.address,
            controller,
            page,
            self.framebuffer.page(page),
        )?;
        self.framebuffer.clear_dirty(page);
        Ok(())
    }

    /// Stores a failure and returns whether the result was a success.
    fn record(&mut self, result: Result<(), Error<I2C::Error>>) -> bool {
        match result {
            Ok(()) => true,
            Err(error) => {
                self.record_error(error);
                false
            }
        }
    }

    fn record_error(&mut self, error: Error<I2C::Error>) {
        if let Error::Bus(e) = error {
            self.last_error = Some(e);
        }
    }

    /// Returns the frame buffer, for drawing without going through [`DrawTarget`].
    pub fn framebuffer(&mut self) -> &mut Framebuffer {
        &mut self.framebuffer
    }

    /// The attached controller, or `None` while detached.
    #[must_use]
    pub fn controller(&self) -> Option<Controller> {
        self.controller
    }

    /// Returns `true` while a panel is attached.
    #[must_use]
    pub fn is_attached(&self) -> bool {
        self.controller.is_some()
    }

    /// The most recent bus error, kept until the next successful attach.
    #[must_use]
    pub fn last_error(&self) -> Option<&I2C::Error> {
        self.last_error.as_ref()
    }

    /// Gives the bus back.
    #[must_use]
    pub fn release(self) -> I2C {
        self.i2c
    }
}

impl<I2C: I2c> OriginDimensions for Oled<I2C> {
    fn size(&self) -> Size {
        Size::new(WIDTH, HEIGHT)
    }
}

impl<I2C: I2c> DrawTarget for Oled<I2C> {
    type Color = BinaryColor;
    type Error = Infallible;

    fn draw_iter<I>(&mut self, pixels: I) -> Result<(), Self::Error>
    where
        I: IntoIterator<Item = Pixel<Self::Color>>,
    {
        self.framebuffer.draw_iter(pixels)
    }

    fn clear(&mut self, color: Self::Color) -> Result<(), Self::Error> {
        self.framebuffer.clear(color)
    }
}

#[cfg(test)]
mod tests {
    extern crate std;
    use std::vec::Vec;

    use super::*;
    use crate::testutil::*;
    use embedded_graphics_core::geometry::Point;
    use tamer::mock::{MockI2c, MockI2cError, Transaction};

    fn nack() -> Transaction<'static> {
        Transaction::Error(MockI2cError::nack())
    }

    fn pixel<I: I2c>(oled: &mut Oled<I>, x: i32, y: i32) {
        oled.draw_iter([Pixel(Point::new(x, y), BinaryColor::On)])
            .unwrap();
    }

    /// Page `p` of a framebuffer with only pixel (x, y) lit.
    fn lit_page(x: usize, y: usize) -> [u8; 128] {
        let mut d = [0u8; 128];
        d[x] = 1 << (y % 8);
        d
    }

    #[test]
    fn tick_attach_after_debounce_full_flush_then_af() {
        let mut script = Vec::new();
        script.push(probe_txn());
        script.push(probe_txn());
        script.extend(attach_script(Controller::Sh1106));
        let mut oled = Oled::new(MockI2c::new(&script), ADDR, 100, 3, 50);
        assert_eq!(oled.tick(0), None);
        assert_eq!(oled.tick(50), None);
        assert!(!oled.is_attached());
        assert_eq!(oled.tick(100), Some(Event::Attached(Controller::Sh1106)));
        assert_eq!(oled.controller(), Some(Controller::Sh1106));
        assert_eq!(oled.framebuffer().dirty_pages(), 0);
        oled.release().done();
    }

    #[test]
    fn tick_detach_after_n_errors() {
        let mut script = attach_script(Controller::Ssd1306);
        script.extend([nack(), nack(), nack()]);
        let mut oled = Oled::new(MockI2c::new(&script), ADDR, 0, 3, 10);
        assert_eq!(oled.tick(0), Some(Event::Attached(Controller::Ssd1306)));
        assert_eq!(oled.tick(10), None);
        assert_eq!(oled.tick(20), None);
        assert!(oled.is_attached());
        assert_eq!(oled.tick(30), Some(Event::Detached));
        assert!(!oled.is_attached());
        assert_eq!(oled.controller(), None);
        oled.release().done();
    }

    #[test]
    fn last_error_set_on_detach() {
        let mut script = attach_script(Controller::Ssd1306);
        script.push(Transaction::Error(MockI2cError::bus()));
        let mut oled = Oled::new(MockI2c::new(&script), ADDR, 0, 1, 10);
        assert!(oled.tick(0).is_some());
        assert_eq!(oled.last_error(), None);
        assert_eq!(oled.tick(10), Some(Event::Detached));
        assert_eq!(oled.last_error(), Some(&MockI2cError::bus()));
        oled.release().done();
    }

    #[test]
    fn tick_draws_while_detached_appear_on_reattach() {
        let mut script = Vec::from([probe_txn()]);
        script.extend(detect_script(Controller::Ssd1306));
        script.extend(init_script(Controller::Ssd1306));
        script.extend(page_script(Controller::Ssd1306, 0, &[0; 128]));
        script.extend(page_script(Controller::Ssd1306, 1, &lit_page(4, 9)));
        for p in 2..8 {
            script.extend(page_script(Controller::Ssd1306, p, &[0; 128]));
        }
        script.push(write(&[0x00, 0xAF]));
        let mut oled = Oled::new(MockI2c::new(&script), ADDR, 0, 3, 10);
        pixel(&mut oled, 4, 9);
        assert_eq!(oled.tick(0), Some(Event::Attached(Controller::Ssd1306)));
        oled.release().done();
    }

    #[test]
    fn tick_flushes_only_dirty_pages() {
        let mut script = attach_script(Controller::Ssd1306);
        script.extend(page_script(Controller::Ssd1306, 2, &lit_page(7, 17)));
        let mut oled = Oled::new(MockI2c::new(&script), ADDR, 0, 3, 10);
        assert!(oled.tick(0).is_some());
        pixel(&mut oled, 7, 17);
        assert_eq!(oled.tick(1), None);
        assert_eq!(oled.framebuffer().dirty_pages(), 0);
        oled.release().done();
    }

    #[test]
    fn tick_swap_sh1106_to_ssd1306_uses_offset_0() {
        let mut script = attach_script(Controller::Sh1106);
        script.push(nack());
        script.push(probe_txn()); // reattach: probe ok (debounce 0)
        script.extend(detect_script(Controller::Ssd1306));
        script.extend(init_script(Controller::Ssd1306));
        script.extend(blank_flush(Controller::Ssd1306));
        script.push(write(&[0x00, 0xAF]));
        let mut oled = Oled::new(MockI2c::new(&script), ADDR, 0, 1, 10);
        assert_eq!(oled.tick(0), Some(Event::Attached(Controller::Sh1106)));
        assert_eq!(oled.tick(10), Some(Event::Detached));
        assert_eq!(oled.tick(20), Some(Event::Attached(Controller::Ssd1306)));
        assert!(script.contains(&write(&[0x00, 0xB0, 0x00, 0x10])));
        oled.release().done();
    }

    #[test]
    fn tick_idle_probe_respects_interval() {
        let mut script = attach_script(Controller::Ssd1306);
        script.push(probe_txn());
        script.push(probe_txn());
        let mut oled = Oled::new(MockI2c::new(&script), ADDR, 0, 3, 50);
        assert!(oled.tick(100).is_some());
        assert_eq!(oled.tick(110), None);
        assert_eq!(oled.tick(149), None);
        assert_eq!(oled.tick(150), None); // probe 1
        assert_eq!(oled.tick(199), None);
        assert_eq!(oled.tick(200), None); // probe 2
        oled.release().done();
    }

    #[test]
    fn tick_attach_failure_retries() {
        let mut script = Vec::from([probe_txn(), nack(), probe_txn()]);
        script.extend(detect_script(Controller::Sh1106));
        script.extend(init_script(Controller::Sh1106));
        script.extend(blank_flush(Controller::Sh1106));
        script.push(write(&[0x00, 0xAF]));
        let mut oled = Oled::new(MockI2c::new(&script), ADDR, 0, 3, 50);
        assert_eq!(oled.tick(0), None); // probe ok, detect NACKs
        assert!(!oled.is_attached());
        assert!(oled.last_error().is_some());
        assert_eq!(oled.tick(50), Some(Event::Attached(Controller::Sh1106)));
        assert_eq!(oled.last_error(), None);
        oled.release().done();
    }

    #[test]
    fn flush_only_dirty_pages() {
        let mut script = attach_script(Controller::Sh1106);
        script.extend(page_script(Controller::Sh1106, 0, &lit_page(0, 0)));
        script.extend(page_script(Controller::Sh1106, 7, &lit_page(127, 63)));
        let mut oled = Oled::new(MockI2c::new(&script), ADDR, 0, 3, 50);
        assert!(oled.tick(0).is_some());
        pixel(&mut oled, 0, 0);
        pixel(&mut oled, 127, 63);
        assert_eq!(oled.flush(), Ok(()));
        assert_eq!(oled.flush(), Ok(())); // nothing dirty: no bus traffic
        oled.release().done();
    }

    #[test]
    fn flush_midpage_nack_keeps_page_dirty_and_resends() {
        let page = lit_page(1, 16);
        let mut script = attach_script(Controller::Ssd1306);
        let first = page_script(Controller::Ssd1306, 2, &page);
        script.extend(first[..2].iter().copied());
        script.push(nack());
        script.extend(first);
        let mut oled = Oled::new(MockI2c::new(&script), ADDR, 0, 3, 50);
        assert!(oled.tick(0).is_some());
        pixel(&mut oled, 1, 16);
        assert!(oled.flush().unwrap_err().is_nack());
        assert_eq!(oled.framebuffer().dirty_pages(), 1 << 2);
        assert_eq!(oled.flush(), Ok(()));
        assert_eq!(oled.framebuffer().dirty_pages(), 0);
        oled.release().done();
    }

    #[test]
    fn flush_when_detached_errs_without_bus() {
        let mut oled = Oled::new(MockI2c::new(&[]), ADDR, 0, 3, 50);
        pixel(&mut oled, 1, 1);
        assert_eq!(oled.flush(), Err(Error::Detached));
        oled.release().done();
    }

    #[test]
    fn redetect_reports_changed_controller() {
        let mut script = attach_script(Controller::Sh1106);
        script.extend(detect_script(Controller::Ssd1306));
        script.extend(init_script(Controller::Ssd1306));
        script.extend(blank_flush(Controller::Ssd1306));
        script.push(write(&[0x00, 0xAF]));
        script.extend(detect_script(Controller::Ssd1306));
        script.extend(init_script(Controller::Ssd1306));
        script.extend(blank_flush(Controller::Ssd1306));
        script.push(write(&[0x00, 0xAF]));
        let mut oled = Oled::new(MockI2c::new(&script), ADDR, 0, 3, 50);
        assert_eq!(oled.tick(0), Some(Event::Attached(Controller::Sh1106)));
        assert_eq!(oled.redetect(5), Some(Event::Attached(Controller::Ssd1306)));
        assert_eq!(oled.controller(), Some(Controller::Ssd1306));
        assert_eq!(oled.redetect(6), None); // same controller
        oled.release().done();
    }

    #[test]
    fn redetect_attaches_without_tick_and_failure_detaches() {
        let mut script = Vec::new();
        script.extend(detect_script(Controller::Ssd1306));
        script.extend(init_script(Controller::Ssd1306));
        script.extend(blank_flush(Controller::Ssd1306));
        script.push(write(&[0x00, 0xAF]));
        script.push(nack());
        let mut oled = Oled::new(MockI2c::new(&script), ADDR, 500, 3, 50);
        assert_eq!(oled.redetect(0), Some(Event::Attached(Controller::Ssd1306)));
        assert_eq!(oled.redetect(1), Some(Event::Detached));
        assert!(!oled.is_attached());
        oled.release().done();
    }

    #[test]
    fn tick_attaches_ssd1306_that_rejects_reads() {
        let select = crate::command::set_page_column(0, 130);
        let mut script = Vec::from([
            probe_txn(),
            write(&select),
            write(&[0x40, 0xA5]),
            write(&select),
            nack(),
        ]);
        script.extend(init_script(Controller::Ssd1306));
        script.extend(blank_flush(Controller::Ssd1306));
        script.push(write(&[0x00, 0xAF]));
        let mut oled = Oled::new(MockI2c::new(&script), ADDR, 0, 3, 10);
        assert_eq!(oled.tick(0), Some(Event::Attached(Controller::Ssd1306)));
        oled.release().done();
    }

    #[test]
    fn fixed_controller_skips_detect() {
        let mut script = Vec::from([probe_txn()]);
        script.extend(init_script(Controller::Sh1106));
        script.extend(blank_flush(Controller::Sh1106));
        script.push(write(&[0x00, 0xAF]));
        let mut oled =
            Oled::with_controller(MockI2c::new(&script), ADDR, Controller::Sh1106, 0, 3, 10);
        assert_eq!(oled.tick(0), Some(Event::Attached(Controller::Sh1106)));
        oled.release().done();
    }

    #[test]
    fn fixed_controller_redetect_reinits_without_detect() {
        let mut script = Vec::new();
        for _ in 0..2 {
            script.extend(init_script(Controller::Ssd1306));
            script.extend(blank_flush(Controller::Ssd1306));
            script.push(write(&[0x00, 0xAF]));
        }
        let mut oled =
            Oled::with_controller(MockI2c::new(&script), ADDR, Controller::Ssd1306, 0, 3, 10);
        assert_eq!(oled.redetect(0), Some(Event::Attached(Controller::Ssd1306)));
        assert_eq!(oled.redetect(5), None);
        assert_eq!(oled.controller(), Some(Controller::Ssd1306));
        oled.release().done();
    }

    #[test]
    fn flaky_contact_reinits_after_recovery() {
        let page = lit_page(1, 16);
        let mut script = attach_script(Controller::Ssd1306);
        let first = page_script(Controller::Ssd1306, 2, &page);
        script.extend(first[..2].iter().copied());
        script.push(nack());
        script.extend(detect_script(Controller::Ssd1306));
        script.extend(init_script(Controller::Ssd1306));
        for p in 0..8 {
            let data = if p == 2 { page } else { [0u8; 128] };
            script.extend(page_script(Controller::Ssd1306, p, &data));
        }
        script.push(write(&[0x00, 0xAF]));
        let mut oled = Oled::new(MockI2c::new(&script), ADDR, 0, 3, 10);
        assert!(oled.tick(0).is_some());
        pixel(&mut oled, 1, 16);
        assert_eq!(oled.tick(10), None); // page write fails, still attached
        assert!(oled.is_attached());
        assert_eq!(oled.tick(20), None); // recovered: full re-init and flush
        assert_eq!(oled.framebuffer().dirty_pages(), 0);
        assert!(oled.is_attached());
        oled.release().done();
    }

    #[test]
    fn flaky_contact_with_swap_reports_new_controller() {
        let mut script = attach_script(Controller::Sh1106);
        script.push(nack());
        script.extend(detect_script(Controller::Ssd1306));
        script.extend(init_script(Controller::Ssd1306));
        script.extend(blank_flush(Controller::Ssd1306));
        script.push(write(&[0x00, 0xAF]));
        let mut oled = Oled::new(MockI2c::new(&script), ADDR, 0, 3, 10);
        assert_eq!(oled.tick(0), Some(Event::Attached(Controller::Sh1106)));
        assert_eq!(oled.tick(10), None); // idle probe fails
        assert_eq!(oled.tick(20), Some(Event::Attached(Controller::Ssd1306)));
        assert_eq!(oled.controller(), Some(Controller::Ssd1306));
        oled.release().done();
    }

    #[test]
    fn redetect_keeps_configured_attach_debounce() {
        let mut script = Vec::new();
        script.extend(detect_script(Controller::Ssd1306));
        script.extend(init_script(Controller::Ssd1306));
        script.extend(blank_flush(Controller::Ssd1306));
        script.push(write(&[0x00, 0xAF]));
        script.push(nack()); // detach (detach_errors = 1)
        script.extend([probe_txn(), probe_txn(), probe_txn()]);
        script.extend(detect_script(Controller::Ssd1306));
        script.extend(init_script(Controller::Ssd1306));
        script.extend(blank_flush(Controller::Ssd1306));
        script.push(write(&[0x00, 0xAF]));
        let mut oled = Oled::new(MockI2c::new(&script), ADDR, 100, 1, 10);
        assert_eq!(oled.redetect(0), Some(Event::Attached(Controller::Ssd1306)));
        assert_eq!(oled.tick(10), Some(Event::Detached));
        assert_eq!(oled.tick(20), None); // window opens
        assert_eq!(oled.tick(30), None);
        assert!(!oled.is_attached());
        assert_eq!(oled.tick(120), Some(Event::Attached(Controller::Ssd1306)));
        oled.release().done();
    }

    #[test]
    #[should_panic(expected = "detach_errors must be non-zero")]
    fn zero_detach_errors_panics() {
        let _ = Oled::new(MockI2c::new(&[]), ADDR, 0, 0, 50);
    }
}
