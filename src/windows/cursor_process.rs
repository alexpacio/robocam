//! A hidden cursor recovery process with Windows startup feedback suppressed.
//! A GUI child without a message loop otherwise forces the busy cursor while
//! waiting on its pipe, overriding the native cursor being tested or displayed.
use super::*;
use std::{
    fs::File,
    io::Write,
    os::windows::{
        ffi::OsStrExt,
        io::{AsRawHandle, FromRawHandle},
    },
};
use windows_sys::Win32::{Security::SECURITY_ATTRIBUTES, System::Pipes::CreatePipe};

pub(super) struct CursorProcess {
    process: HANDLE,
    pipe: Option<File>,
}
impl CursorProcess {
    pub unsafe fn start(argument: &str) -> Option<Self> {
        let exe = std::env::current_exe().ok()?;
        let mut application: Vec<_> = exe.as_os_str().encode_wide().collect();
        let mut command = vec![34u16];
        command.extend(&application);
        command.extend(format!("\" {argument}\0").encode_utf16());
        application.push(0);
        let attributes = SECURITY_ATTRIBUTES {
            nLength: size_of::<SECURITY_ATTRIBUTES>() as u32,
            lpSecurityDescriptor: null_mut(),
            bInheritHandle: 1,
        };
        let (mut read, mut write) = (null_mut(), null_mut());
        if CreatePipe(&mut read, &mut write, &attributes, 0) == 0 {
            return None;
        }
        let read = File::from_raw_handle(read);
        let write = File::from_raw_handle(write);
        if SetHandleInformation(write.as_raw_handle(), HANDLE_FLAG_INHERIT, 0) == 0 {
            return None;
        }
        let output = File::options().write(true).open("NUL").ok()?;
        if SetHandleInformation(
            output.as_raw_handle(),
            HANDLE_FLAG_INHERIT,
            HANDLE_FLAG_INHERIT,
        ) == 0
        {
            return None;
        }
        let startup = STARTUPINFOW {
            cb: size_of::<STARTUPINFOW>() as u32,
            dwFlags: STARTF_USESTDHANDLES | STARTF_FORCEOFFFEEDBACK,
            hStdInput: read.as_raw_handle(),
            hStdOutput: output.as_raw_handle(),
            hStdError: output.as_raw_handle(),
            ..zeroed()
        };
        let mut information: PROCESS_INFORMATION = zeroed();
        if CreateProcessW(
            application.as_ptr(),
            command.as_mut_ptr(),
            null(),
            null(),
            1,
            CREATE_NO_WINDOW,
            null(),
            null(),
            &startup,
            &mut information,
        ) == 0
        {
            return None;
        }
        CloseHandle(information.hThread);
        Some(Self {
            process: information.hProcess,
            pipe: Some(write),
        })
    }
    pub fn signal(&mut self, armed: bool) -> bool {
        self.pipe
            .as_mut()
            .is_some_and(|pipe| pipe.write_all(&[u8::from(armed)]).is_ok())
    }
    pub unsafe fn exit_code(&self) -> Option<u32> {
        if WaitForSingleObject(self.process, 0) != WAIT_OBJECT_0 {
            return None;
        }
        let mut code = 0;
        (GetExitCodeProcess(self.process, &mut code) != 0).then_some(code)
    }
}
impl Drop for CursorProcess {
    fn drop(&mut self) {
        unsafe {
            drop(self.pipe.take());
            WaitForSingleObject(self.process, 10000);
            CloseHandle(self.process);
        }
    }
}
