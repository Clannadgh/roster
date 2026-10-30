#[cfg(windows)]
pub fn protect(data: &[u8]) -> Result<Vec<u8>, String> {
    use windows_sys::Win32::Security::Cryptography::{
        CryptProtectData, CRYPT_INTEGER_BLOB, CRYPTPROTECT_UI_FORBIDDEN,
    };

    let input = CRYPT_INTEGER_BLOB {
        cbData: data.len() as u32,
        pbData: data.as_ptr() as *mut u8,
    };
    let mut output = CRYPT_INTEGER_BLOB {
        cbData: 0,
        pbData: std::ptr::null_mut(),
    };
    let ok = unsafe {
        CryptProtectData(
            &input,
            std::ptr::null(),
            std::ptr::null(),
            std::ptr::null(),
            std::ptr::null(),
            CRYPTPROTECT_UI_FORBIDDEN,
            &mut output,
        )
    };
    if ok == 0 {
        return Err(format!(
            "DPAPI 加密失败：{}",
            std::io::Error::last_os_error()
        ));
    }
    Ok(unsafe { take_blob(output) })
}

#[cfg(windows)]
pub fn unprotect(data: &[u8]) -> Result<Vec<u8>, String> {
    use windows_sys::Win32::Security::Cryptography::{
        CryptUnprotectData, CRYPT_INTEGER_BLOB, CRYPTPROTECT_UI_FORBIDDEN,
    };

    let input = CRYPT_INTEGER_BLOB {
        cbData: data.len() as u32,
        pbData: data.as_ptr() as *mut u8,
    };
    let mut output = CRYPT_INTEGER_BLOB {
        cbData: 0,
        pbData: std::ptr::null_mut(),
    };
    let ok = unsafe {
        CryptUnprotectData(
            &input,
            std::ptr::null_mut(),
            std::ptr::null(),
            std::ptr::null(),
            std::ptr::null(),
            CRYPTPROTECT_UI_FORBIDDEN,
            &mut output,
        )
    };
    if ok == 0 {
        return Err(format!(
            "DPAPI 解密失败：{}",
            std::io::Error::last_os_error()
        ));
    }
    Ok(unsafe { take_blob(output) })
}

#[cfg(windows)]
unsafe fn take_blob(
    blob: windows_sys::Win32::Security::Cryptography::CRYPT_INTEGER_BLOB,
) -> Vec<u8> {
    let bytes = std::slice::from_raw_parts(blob.pbData, blob.cbData as usize).to_vec();
    windows_sys::Win32::Foundation::LocalFree(blob.pbData as windows_sys::Win32::Foundation::HLOCAL);
    bytes
}

#[cfg(not(windows))]
pub fn protect(data: &[u8]) -> Result<Vec<u8>, String> {
    Ok(data.to_vec())
}

#[cfg(not(windows))]
pub fn unprotect(data: &[u8]) -> Result<Vec<u8>, String> {
    Ok(data.to_vec())
}
