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

// impl Into<sesh_proto::

impl From<&Size> for libc::winsize {
    fn from(val: &Size) -> Self {
        libc::winsize {
            ws_row: val.rows,
            ws_col: val.cols,
            ws_xpixel: 0,
            ws_ypixel: 0,
        }
    }
}
