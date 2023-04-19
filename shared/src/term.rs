use anyhow::Result;
use termion;

pub struct Size {
    /// Number of columns
    pub cols: u16,
    /// Number of rows
    pub rows: u16,
}

impl Size {
    pub fn term_size() -> Result<Size> {
        let (cols, rows) = termion::terminal_size()?;
        Ok(Size { cols, rows })
    }
}

impl Into<libc::winsize> for &Size {
    fn into(self) -> libc::winsize {
        libc::winsize {
            ws_row: self.rows,
            ws_col: self.cols,
            ws_xpixel: 0,
            ws_ypixel: 0,
        }
    }
}
