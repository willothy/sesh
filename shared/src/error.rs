pub trait CResult<T, E>: Sized {
    fn to_result(self) -> Result<T, E>;
}

impl CResult<libc::c_int, anyhow::Error> for libc::c_int {
    fn to_result(self) -> Result<libc::c_int, anyhow::Error> {
        match self {
            -1 => Err(anyhow::anyhow!("C Error")),
            res => Ok(res),
        }
    }
}

impl CResult<libc::passwd, anyhow::Error> for *mut libc::passwd {
    fn to_result(self) -> Result<libc::passwd, anyhow::Error> {
        if self == std::ptr::null_mut() {
            return Err(anyhow::anyhow!("Could not get passwd entry"));
        } else {
            unsafe { Ok(*self) }
        }
    }
}
