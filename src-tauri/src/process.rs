#[cfg(windows)]
pub fn image_running(name: &str) -> bool {
    use std::os::windows::process::CommandExt;
    const CREATE_NO_WINDOW: u32 = 0x0800_0000;
    let output = std::process::Command::new("tasklist")
        .args(["/FI", &format!("IMAGENAME eq {name}"), "/NH"])
        .creation_flags(CREATE_NO_WINDOW)
        .output();
    match output {
        Ok(output) => String::from_utf8_lossy(&output.stdout)
            .to_ascii_lowercase()
            .contains(&name.to_ascii_lowercase()),
        Err(_) => false,
    }
}

#[cfg(not(windows))]
pub fn image_running(_name: &str) -> bool {
    false
}
