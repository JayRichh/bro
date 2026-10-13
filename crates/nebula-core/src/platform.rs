//! Process-level OS seams the daemon, the TUI and the CLI share. Unix needs
//! nothing here: its callers keep their signals, `ps` sweeps and `/proc`
//! reads. Windows has none of those, so this is where its stand-ins live,
//! on the Win32 API directly.

#[cfg(windows)]
pub use self::windows::*;

#[cfg(windows)]
mod windows {
    use windows_sys::Win32::Foundation::{CloseHandle, HANDLE, INVALID_HANDLE_VALUE, STILL_ACTIVE};
    use windows_sys::Win32::System::Diagnostics::ToolHelp::{
        CreateToolhelp32Snapshot, Process32FirstW, Process32NextW, PROCESSENTRY32W,
        TH32CS_SNAPPROCESS,
    };
    use windows_sys::Win32::System::ProcessStatus::{
        K32GetProcessMemoryInfo, PROCESS_MEMORY_COUNTERS,
    };
    use windows_sys::Win32::System::SystemInformation::{GlobalMemoryStatusEx, MEMORYSTATUSEX};
    use windows_sys::Win32::System::Threading::{
        GetExitCodeProcess, OpenProcess, TerminateProcess, CREATE_NO_WINDOW, PROCESS_ACCESS_RIGHTS,
        PROCESS_QUERY_LIMITED_INFORMATION, PROCESS_TERMINATE,
    };

    /// A process handle, closed on drop.
    struct Process(HANDLE);

    impl Process {
        fn open(pid: u32, access: PROCESS_ACCESS_RIGHTS) -> Option<Self> {
            // SAFETY: plain FFI call; a null handle is the failure value.
            let handle = unsafe { OpenProcess(access, 0, pid) };
            (!handle.is_null()).then_some(Self(handle))
        }
    }

    impl Drop for Process {
        fn drop(&mut self) {
            // SAFETY: the handle came from OpenProcess and is closed once.
            unsafe { CloseHandle(self.0) };
        }
    }

    /// Whether `pid` names a process that has yet to exit.
    pub fn process_running(pid: u32) -> bool {
        let Some(process) = Process::open(pid, PROCESS_QUERY_LIMITED_INFORMATION) else {
            return false;
        };
        let mut code = 0u32;
        // SAFETY: `process` is a live handle with query access.
        let ok = unsafe { GetExitCodeProcess(process.0, &mut code) } != 0;
        ok && code == STILL_ACTIVE as u32
    }

    /// End `pid` outright. Windows has no SIGTERM to ask a detached process
    /// politely, so this is the last resort a caller reaches for once its
    /// own channel to the process (the daemon's `Shutdown`) has failed.
    pub fn terminate_process(pid: u32) -> std::io::Result<()> {
        let process =
            Process::open(pid, PROCESS_TERMINATE).ok_or_else(std::io::Error::last_os_error)?;
        // SAFETY: `process` is a live handle with terminate access.
        if unsafe { TerminateProcess(process.0, 1) } == 0 {
            return Err(std::io::Error::last_os_error());
        }
        Ok(())
    }

    /// One process as the toolhelp snapshot reports it.
    #[derive(Debug, Clone, Copy)]
    pub struct ProcessRow {
        pub pid: u32,
        pub ppid: u32,
    }

    /// Every process on the machine with its parent — the Windows stand-in
    /// for a `ps -axo pid=,ppid=` sweep. Empty when the snapshot fails.
    pub fn process_table() -> Vec<ProcessRow> {
        // SAFETY: plain FFI call; INVALID_HANDLE_VALUE is the failure value.
        let snapshot = unsafe { CreateToolhelp32Snapshot(TH32CS_SNAPPROCESS, 0) };
        if snapshot == INVALID_HANDLE_VALUE || snapshot.is_null() {
            return Vec::new();
        }
        let mut rows = Vec::new();
        // SAFETY: an all-zero PROCESSENTRY32W is valid once dwSize is set.
        let mut entry: PROCESSENTRY32W = unsafe { std::mem::zeroed() };
        entry.dwSize = std::mem::size_of::<PROCESSENTRY32W>() as u32;
        // SAFETY: `snapshot` is live and `entry` is sized for the call.
        let mut more = unsafe { Process32FirstW(snapshot, &mut entry) } != 0;
        while more {
            rows.push(ProcessRow {
                pid: entry.th32ProcessID,
                ppid: entry.th32ParentProcessID,
            });
            // SAFETY: as above.
            more = unsafe { Process32NextW(snapshot, &mut entry) } != 0;
        }
        // SAFETY: the snapshot handle is closed once.
        unsafe { CloseHandle(snapshot) };
        rows
    }

    /// Working set of one process, bytes — the RSS of the unix readings.
    pub fn process_rss_bytes(pid: u32) -> Option<u64> {
        let process = Process::open(pid, PROCESS_QUERY_LIMITED_INFORMATION)?;
        // SAFETY: an all-zero counters struct is valid for the call to fill.
        let mut counters: PROCESS_MEMORY_COUNTERS = unsafe { std::mem::zeroed() };
        let size = std::mem::size_of::<PROCESS_MEMORY_COUNTERS>() as u32;
        // SAFETY: `process` is live and `counters` is `size` bytes.
        let ok = unsafe { K32GetProcessMemoryInfo(process.0, &mut counters, size) } != 0;
        ok.then_some(counters.WorkingSetSize as u64)
    }

    /// Physical memory installed on this machine, bytes.
    pub fn system_total_bytes() -> Option<u64> {
        // SAFETY: an all-zero MEMORYSTATUSEX is valid once dwLength is set.
        let mut status: MEMORYSTATUSEX = unsafe { std::mem::zeroed() };
        status.dwLength = std::mem::size_of::<MEMORYSTATUSEX>() as u32;
        // SAFETY: `status` is sized for the call.
        let ok = unsafe { GlobalMemoryStatusEx(&mut status) } != 0;
        ok.then_some(status.ullTotalPhys)
    }

    /// Keep a console child of a process with no console from opening a
    /// window of its own. The daemon runs detached, so every `git`, probe or
    /// helper it starts would otherwise flash a console onto the desktop.
    pub fn no_window(cmd: &mut std::process::Command) -> &mut std::process::Command {
        use std::os::windows::process::CommandExt;
        cmd.creation_flags(CREATE_NO_WINDOW)
    }

    /// The `CREATE_NO_WINDOW` flag itself, for `tokio::process::Command`.
    pub const NO_WINDOW_FLAGS: u32 = CREATE_NO_WINDOW;

    /// Every TCP socket in LISTEN state on the machine, IPv4 and IPv6, as
    /// `(port, owning pid)` — `GetExtendedTcpTable` with the owner-pid
    /// listener class. A dev server bound only to `::1` (Vite and Next.js
    /// often are) shows up through the IPv6 table. Empty when both calls
    /// fail.
    pub fn listening_ports() -> Vec<(u16, u32)> {
        use windows_sys::Win32::NetworkManagement::IpHelper::{
            GetExtendedTcpTable, MIB_TCP6ROW_OWNER_PID, MIB_TCPROW_OWNER_PID,
            TCP_TABLE_OWNER_PID_LISTENER,
        };
        use windows_sys::Win32::Networking::WinSock::{AF_INET, AF_INET6};

        /// One family's table as raw bytes, grown until the call fits.
        fn table(family: u16) -> Option<Vec<u8>> {
            let mut size = 0u32;
            for _ in 0..4 {
                let mut buf = vec![0u8; size.max(4) as usize];
                // SAFETY: `buf` holds `size` writable bytes; the call
                // writes at most that many and reports the size it needs.
                let rc = unsafe {
                    GetExtendedTcpTable(
                        buf.as_mut_ptr().cast(),
                        &mut size,
                        0,
                        u32::from(family),
                        TCP_TABLE_OWNER_PID_LISTENER,
                        0,
                    )
                };
                match rc {
                    0 => return Some(buf),
                    // ERROR_INSUFFICIENT_BUFFER: `size` now says how much.
                    122 => continue,
                    _ => return None,
                }
            }
            None
        }

        /// The rows after the table's leading `dwNumEntries`, read
        /// unaligned out of the byte buffer.
        fn rows<T: Copy>(buf: &[u8]) -> Vec<T> {
            if buf.len() < 4 {
                return Vec::new();
            }
            let count = u32::from_ne_bytes([buf[0], buf[1], buf[2], buf[3]]) as usize;
            // The row array starts at the row type's alignment past the count.
            let start = std::mem::align_of::<T>().max(4);
            let size = std::mem::size_of::<T>();
            (0..count)
                .map(|i| start + i * size)
                .take_while(|at| at + size <= buf.len())
                // SAFETY: each read is in bounds (checked above) and `T` is
                // a plain-data row type, valid for any bit pattern.
                .map(|at| unsafe { std::ptr::read_unaligned(buf[at..].as_ptr().cast::<T>()) })
                .collect()
        }

        // The port is in network byte order in the low 16 bits.
        let port = |raw: u32| u16::from_be(raw as u16);
        let mut out = Vec::new();
        if let Some(buf) = table(AF_INET) {
            out.extend(
                rows::<MIB_TCPROW_OWNER_PID>(&buf)
                    .into_iter()
                    .map(|r| (port(r.dwLocalPort), r.dwOwningPid)),
            );
        }
        if let Some(buf) = table(AF_INET6) {
            out.extend(
                rows::<MIB_TCP6ROW_OWNER_PID>(&buf)
                    .into_iter()
                    .map(|r| (port(r.dwLocalPort), r.dwOwningPid)),
            );
        }
        out.sort_unstable();
        out.dedup();
        out
    }

    #[cfg(test)]
    mod tests {
        use super::*;

        #[test]
        fn this_process_is_seen_running_with_its_parent() {
            let me = std::process::id();
            assert!(process_running(me));
            assert!(process_table().iter().any(|row| row.pid == me));
            assert!(process_rss_bytes(me).is_some_and(|b| b > 0));
            assert!(system_total_bytes().is_some_and(|b| b > 0));
            assert!(!process_running(u32::MAX - 3), "no such pid");
        }

        #[test]
        fn a_listener_of_ours_is_listed_on_both_families() {
            let me = std::process::id();
            let v4 = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
            let v4_port = v4.local_addr().unwrap().port();
            let table = listening_ports();
            assert!(table.contains(&(v4_port, me)), "{v4_port} in {table:?}");
            if let Ok(v6) = std::net::TcpListener::bind("[::1]:0") {
                let v6_port = v6.local_addr().unwrap().port();
                assert!(listening_ports().contains(&(v6_port, me)), "::1 {v6_port}");
            }
        }
    }
}
