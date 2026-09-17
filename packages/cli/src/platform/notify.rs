//! 桌面通知。后台守护进程的 stdout 进了日志文件，终端横幅用户看不到，
//! 所以升级提醒这类需要用户动手的事走系统通知中心。工具不存在时静默。

pub fn notify_desktop(version: &str, hint: &str) {
    let title = format!("chatbot — 新版本 v{version}");
    let body = format!("运行 {hint} 升级");

    #[cfg(target_os = "macos")]
    {
        let script = format!(
            r#"display notification "{}" with title "{}""#,
            body.replace('"', "\\\""),
            title.replace('"', "\\\"")
        );
        let _ = std::process::Command::new("osascript")
            .args(["-e", &script])
            .output();
    }

    #[cfg(target_os = "linux")]
    {
        let _ = std::process::Command::new("notify-send")
            .args([&title, &body])
            .output();
    }

    #[cfg(not(any(target_os = "macos", target_os = "linux")))]
    {
        let _ = (title, body);
    }
}
