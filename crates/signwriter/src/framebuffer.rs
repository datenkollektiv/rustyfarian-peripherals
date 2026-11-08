//! I/O-free frame buffer with dirty-page tracking.

use core::convert::Infallible;

use embedded_graphics_core::draw_target::DrawTarget;
use embedded_graphics_core::geometry::{OriginDimensions, Size};
use embedded_graphics_core::pixelcolor::BinaryColor;
use embedded_graphics_core::Pixel;

use crate::{HEIGHT, PAGES, WIDTH};

const COLUMNS: usize = WIDTH as usize;
const PAGE_COUNT: usize = PAGES as usize;

/// A 128x64 monochrome buffer in the controllers' native layout.
///
/// Page-major: byte `page * 128 + x` holds the pixels `y = page * 8 .. page * 8 + 8`
/// of column `x`, least significant bit at the top.
/// Each page has a dirty bit that is set only when a byte actually changes, so
/// redrawing identical content costs no bus traffic.
///
/// It implements [`DrawTarget`] with [`BinaryColor`]; pixels outside the panel
/// are clipped.
///
/// ```
/// use embedded_graphics_core::{pixelcolor::BinaryColor, prelude::*};
/// use signwriter::Framebuffer;
///
/// let mut fb = Framebuffer::new();
/// Pixel(Point::new(0, 9), BinaryColor::On).draw(&mut fb).unwrap();
/// assert_eq!(fb.page(1)[0], 0b0000_0010);
/// assert_eq!(fb.dirty_pages(), 0b0000_0010);
/// ```
#[derive(Debug, Clone)]
pub struct Framebuffer {
    pages: [[u8; COLUMNS]; PAGE_COUNT],
    dirty: u8,
}

impl Framebuffer {
    /// Creates a blank buffer with no dirty pages.
    #[must_use]
    pub const fn new() -> Self {
        Self {
            pages: [[0; COLUMNS]; PAGE_COUNT],
            dirty: 0,
        }
    }

    /// Turns every pixel off, marking the pages that changed.
    pub fn clear_buffer(&mut self) {
        self.fill(0x00);
    }

    /// Marks every page dirty, forcing the next flush to send the whole buffer.
    pub fn mark_all_dirty(&mut self) {
        self.dirty = 0xFF;
    }

    /// Returns the dirty mask: bit `p` is set when page `p` needs sending.
    #[must_use]
    pub const fn dirty_pages(&self) -> u8 {
        self.dirty
    }

    /// Returns the 128 bytes of `page`.
    ///
    /// `page` is reduced modulo 8, so every input returns a valid page.
    #[must_use]
    pub fn page(&self, page: u8) -> &[u8; COLUMNS] {
        &self.pages[usize::from(page & 0x07)]
    }

    pub(crate) fn clear_dirty(&mut self, page: u8) {
        self.dirty &= !(1 << (page & 0x07));
    }

    fn fill(&mut self, value: u8) {
        for (index, page) in self.pages.iter_mut().enumerate() {
            if page.iter().any(|&byte| byte != value) {
                *page = [value; COLUMNS];
                self.dirty |= 1 << index;
            }
        }
    }

    fn set_pixel(&mut self, x: i32, y: i32, on: bool) {
        let (Ok(x), Ok(y)) = (usize::try_from(x), usize::try_from(y)) else {
            return;
        };
        if x >= COLUMNS || y >= HEIGHT as usize {
            return;
        }
        let page = y / 8;
        let mask = 1u8 << (y % 8);
        let byte = &mut self.pages[page][x];
        let new = if on { *byte | mask } else { *byte & !mask };
        if new != *byte {
            *byte = new;
            self.dirty |= 1 << page;
        }
    }
}

impl Default for Framebuffer {
    fn default() -> Self {
        Self::new()
    }
}

impl OriginDimensions for Framebuffer {
    fn size(&self) -> Size {
        Size::new(WIDTH, HEIGHT)
    }
}

impl DrawTarget for Framebuffer {
    type Color = BinaryColor;
    type Error = Infallible;

    fn draw_iter<I>(&mut self, pixels: I) -> Result<(), Self::Error>
    where
        I: IntoIterator<Item = Pixel<Self::Color>>,
    {
        for Pixel(point, color) in pixels {
            self.set_pixel(point.x, point.y, color.is_on());
        }
        Ok(())
    }

    fn clear(&mut self, color: Self::Color) -> Result<(), Self::Error> {
        self.fill(if color.is_on() { 0xFF } else { 0x00 });
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use embedded_graphics_core::geometry::Point;

    fn px(fb: &mut Framebuffer, x: i32, y: i32, on: bool) {
        let c = if on {
            BinaryColor::On
        } else {
            BinaryColor::Off
        };
        fb.draw_iter([Pixel(Point::new(x, y), c)]).unwrap();
    }

    #[test]
    fn pixel_bit_mapping() {
        let mut fb = Framebuffer::new();
        px(&mut fb, 0, 0, true);
        px(&mut fb, 5, 7, true);
        px(&mut fb, 127, 63, true);
        assert_eq!(fb.page(0)[0], 0b0000_0001);
        assert_eq!(fb.page(0)[5], 0b1000_0000);
        assert_eq!(fb.page(7)[127], 0b1000_0000);
        assert_eq!(fb.dirty_pages(), 0b1000_0001);
    }

    #[test]
    fn pixel_clipping() {
        let mut fb = Framebuffer::new();
        for (x, y) in [(-1, 0), (0, -1), (128, 0), (0, 64), (i32::MIN, i32::MAX)] {
            px(&mut fb, x, y, true);
        }
        assert_eq!(fb.dirty_pages(), 0);
        assert!((0..8).all(|p| fb.page(p).iter().all(|&b| b == 0)));
    }

    #[test]
    fn dirty_only_on_byte_change() {
        let mut fb = Framebuffer::new();
        px(&mut fb, 3, 17, false);
        assert_eq!(fb.dirty_pages(), 0);
        px(&mut fb, 3, 17, true);
        assert_eq!(fb.dirty_pages(), 0b100);
        fb.clear_dirty(2);
        px(&mut fb, 3, 17, true);
        assert_eq!(fb.dirty_pages(), 0);
        px(&mut fb, 3, 17, false);
        assert_eq!(fb.dirty_pages(), 0b100);
    }

    #[test]
    fn clear_marks_only_changed_pages() {
        let mut fb = Framebuffer::new();
        px(&mut fb, 0, 40, true);
        fb.clear_dirty(5);
        fb.clear_buffer();
        assert_eq!(fb.dirty_pages(), 1 << 5);
        fb.clear_dirty(5);
        fb.clear(BinaryColor::On).unwrap();
        assert_eq!(fb.dirty_pages(), 0xFF);
        assert!(fb.page(3).iter().all(|&b| b == 0xFF));
    }

    #[test]
    fn mark_all_dirty_sets_every_page() {
        let mut fb = Framebuffer::new();
        fb.mark_all_dirty();
        assert_eq!(fb.dirty_pages(), 0xFF);
    }

    #[test]
    fn size_is_panel_size() {
        assert_eq!(Framebuffer::new().size(), Size::new(128, 64));
    }
}
