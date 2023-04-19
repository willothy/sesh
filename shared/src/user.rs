use anyhow::Result;
use std::{env, ffi};

use crate::error::CResult;

struct Passwd {
    pub shell: String,
}

fn get_passwd() -> Result<Passwd> {
    unsafe {
        let passwd = libc::getpwuid(libc::getuid()).to_result()?;
        let shell = ffi::CStr::from_ptr(passwd.pw_shell).to_str()?.to_string();
        Ok(Passwd { shell })
    }
}

pub fn get_shell() -> String {
    env::var("SHELL")
        .or_else(|_| get_passwd().map(|passwd| passwd.shell))
        .unwrap_or_else(|_| "/bin/sh".to_string())
}
