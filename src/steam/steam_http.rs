//! Small synchronous HTTPS transport for background metadata tasks on Windows.
#[cfg(windows)]
pub fn request(url: &str, body: Option<&str>, limit: usize) -> Result<Vec<u8>, String> {
    use std::{
        ffi::c_void,
        ptr::{null, null_mut},
        time::{Duration, Instant},
    };
    use windows_sys::Win32::Networking::WinHttp::*;
    struct Handle(*mut c_void);
    impl Drop for Handle {
        fn drop(&mut self) {
            unsafe {
                WinHttpCloseHandle(self.0);
            }
        }
    }
    fn handle(ptr: *mut c_void) -> Result<Handle, String> {
        if ptr.is_null() {
            Err(std::io::Error::last_os_error().to_string())
        } else {
            Ok(Handle(ptr))
        }
    }
    fn check(value: i32) -> Result<(), String> {
        if value == 0 {
            Err(std::io::Error::last_os_error().to_string())
        } else {
            Ok(())
        }
    }
    fn wide(s: &str) -> Vec<u16> {
        s.encode_utf16().chain(Some(0)).collect()
    }
    let url = url::Url::parse(url).map_err(|e| e.to_string())?;
    if url.scheme() != "https" || !url.username().is_empty() || url.password().is_some() {
        return Err("仅支持 HTTPS 元数据地址".into());
    }
    let host = wide(url.host_str().ok_or("地址缺少主机")?);
    let resource = wide(&format!(
        "{}{}",
        url.path(),
        url.query().map(|q| format!("?{q}")).unwrap_or_default()
    ));
    let agent = wide("AddonWrench/0.1 metadata");
    // Respect explicit environment proxies, including restricted development environments.
    let proxy = std::env::var("HTTPS_PROXY")
        .or_else(|_| std::env::var("https_proxy"))
        .ok()
        .filter(|s| !s.is_empty())
        .map(|s| {
            let p = url::Url::parse(&s).map_err(|e| e.to_string())?;
            Ok::<_, String>(wide(&format!(
                "{}:{}",
                p.host_str().ok_or("代理缺少主机")?,
                p.port_or_known_default().unwrap_or(80)
            )))
        })
        .transpose()?;
    // All handles and buffers remain owned for the duration of each synchronous WinHTTP call.
    unsafe {
        let session = handle(WinHttpOpen(
            agent.as_ptr(),
            if proxy.is_some() {
                WINHTTP_ACCESS_TYPE_NAMED_PROXY
            } else {
                WINHTTP_ACCESS_TYPE_AUTOMATIC_PROXY
            },
            proxy.as_ref().map_or(null(), |p| p.as_ptr()),
            null(),
            0,
        ))?;
        check(WinHttpSetTimeouts(session.0, 4000, 4000, 5000, 5000))?;
        let connection = handle(WinHttpConnect(
            session.0,
            host.as_ptr(),
            url.port_or_known_default().unwrap_or(443),
            0,
        ))?;
        let verb = wide(if body.is_some() { "POST" } else { "GET" });
        let request = handle(WinHttpOpenRequest(
            connection.0,
            verb.as_ptr(),
            resource.as_ptr(),
            null(),
            null(),
            null(),
            WINHTTP_FLAG_SECURE,
        ))?;
        let redirect = WINHTTP_OPTION_REDIRECT_POLICY_NEVER;
        check(WinHttpSetOption(
            request.0,
            WINHTTP_OPTION_REDIRECT_POLICY,
            &redirect as *const _ as *const c_void,
            4,
        ))?;
        let headers = wide("Content-Type: application/x-www-form-urlencoded\r\n");
        let bytes = body.unwrap_or("").as_bytes();
        check(WinHttpSendRequest(
            request.0,
            headers.as_ptr(),
            (headers.len() - 1) as u32,
            bytes.as_ptr() as *const c_void,
            bytes.len() as u32,
            bytes.len() as u32,
            0,
        ))?;
        check(WinHttpReceiveResponse(request.0, null_mut()))?;
        let mut status = 0u32;
        let mut size = 4;
        check(WinHttpQueryHeaders(
            request.0,
            WINHTTP_QUERY_STATUS_CODE | WINHTTP_QUERY_FLAG_NUMBER,
            null(),
            &mut status as *mut _ as *mut c_void,
            &mut size,
            null_mut(),
        ))?;
        if status != 200 {
            return Err(match status {
                429 => "Steam 请求过于频繁，请稍后重试".into(),
                401 | 403 => "Steam 拒绝访问，工坊资料暂不可用".into(),
                500..=599 => format!("Steam 服务暂不可用（HTTP {status}）"),
                _ => format!("Steam HTTP {status}"),
            });
        }
        let mut result = Vec::new();
        let start = Instant::now();
        loop {
            if start.elapsed() > Duration::from_secs(15) {
                return Err("Steam 响应超时".into());
            }
            let mut chunk = [0u8; 16384];
            let mut read = 0;
            check(WinHttpReadData(
                request.0,
                chunk.as_mut_ptr() as *mut c_void,
                chunk.len() as u32,
                &mut read,
            ))?;
            if read == 0 {
                break;
            }
            if result.len() + read as usize > limit {
                return Err("Steam 响应超过大小限制".into());
            }
            result.extend_from_slice(&chunk[..read as usize]);
        }
        Ok(result)
    }
}

#[cfg(not(windows))]
pub fn request(_: &str, _: Option<&str>, _: usize) -> Result<Vec<u8>, String> {
    Err("工坊在线元数据当前仅支持 Windows".into())
}
