//! Поиск PID, слушающего заданный TCP-порт, через `GetExtendedTcpTable`
//! (раздел IX плана) — первый шаг резолва "наш/чужой" процесс перед тем,
//! как Launcher решит остановить компонент через `/shutdown` или
//! пожаловаться пользователю на занятый порт.

#[cfg(windows)]
pub fn find_pid_listening_on_port(port: u16) -> Option<u32> {
    use windows::Win32::NetworkManagement::IpHelper::{
        GetExtendedTcpTable, MIB_TCPROW_OWNER_PID, MIB_TCPTABLE_OWNER_PID,
        TCP_TABLE_OWNER_PID_LISTENER,
    };
    use windows::Win32::Networking::WinSock::AF_INET;

    let mut size: u32 = 0;

    // Первый вызов — только чтобы узнать требуемый размер буфера
    // (стандартный паттерн GetExtendedTcpTable/GetAdaptersAddresses и т.п.).
    // SAFETY: buffer=None + order=false валидны для запроса размера.
    unsafe {
        let _ = GetExtendedTcpTable(
            None,
            &mut size,
            false,
            AF_INET.0 as u32,
            TCP_TABLE_OWNER_PID_LISTENER,
            0,
        );
    }
    if size == 0 {
        return None;
    }

    let mut buffer = vec![0u8; size as usize];
    // SAFETY: buffer только что выделен под точный требуемый размер.
    let result = unsafe {
        GetExtendedTcpTable(
            Some(buffer.as_mut_ptr().cast()),
            &mut size,
            false,
            AF_INET.0 as u32,
            TCP_TABLE_OWNER_PID_LISTENER,
            0,
        )
    };
    if result != 0 {
        return None;
    }

    // SAFETY: буфер заполнен GetExtendedTcpTable по задокументированному
    // формату: u32 (число строк) + столько же MIB_TCPROW_OWNER_PID подряд.
    // Читаем через указатели вместо полного каста структуры, т.к.
    // `table` в биндинге представлен как массив длины 1 (flexible array
    // member на самом деле).
    let num_entries = unsafe { *(buffer.as_ptr() as *const u32) };
    let rows_ptr = unsafe {
        buffer
            .as_ptr()
            .add(std::mem::size_of::<u32>())
            .cast::<MIB_TCPROW_OWNER_PID>()
    };
    // Убеждаемся, что тип таблицы соответствует ожиданиям (защита от
    // будущих несовпадений layout между версиями Windows/крейта).
    let _ = std::mem::size_of::<MIB_TCPTABLE_OWNER_PID>();

    for i in 0..num_entries as usize {
        // SAFETY: i < num_entries, буфер вмещает ровно num_entries строк
        // согласно контракту GetExtendedTcpTable.
        let row = unsafe { rows_ptr.add(i).read_unaligned() };
        let row_port = u16::from_be(row.dwLocalPort as u16);
        if row_port == port {
            return Some(row.dwOwningPid);
        }
    }
    None
}

#[cfg(all(test, windows))]
mod tests {
    use super::*;
    use std::net::TcpListener;

    #[test]
    fn finds_own_process_listening_on_bound_port() {
        let listener = TcpListener::bind("127.0.0.1:0").expect("bind should succeed");
        let port = listener.local_addr().unwrap().port();

        let found_pid = find_pid_listening_on_port(port);
        assert_eq!(found_pid, Some(std::process::id()));

        drop(listener);
    }

    #[test]
    fn returns_none_for_unbound_port() {
        // Bind to get a port, then immediately drop the listener so nothing
        // is listening on it anymore.
        let listener = TcpListener::bind("127.0.0.1:0").expect("bind should succeed");
        let port = listener.local_addr().unwrap().port();
        drop(listener);

        assert_eq!(find_pid_listening_on_port(port), None);
    }
}
