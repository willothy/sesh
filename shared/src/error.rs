#![allow(clippy::not_unsafe_ptr_arg_deref)]

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
        if self.is_null() {
            Err(anyhow::anyhow!("Could not get passwd entry"))
        } else {
            Ok(unsafe { *self })
        }
    }
}
