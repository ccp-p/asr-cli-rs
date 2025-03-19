use std::time::Duration;    

pub fn get_file_extension(path: &str) -> Option<&str> {
    // File utility functions
    None
}
pub fn check_ffmpeg_available() -> bool {
    // Check if FFmpeg is available in the system PATH
    false
}
/// 格式化时间间隔为友好字符串
pub fn format_time_duration(duration: &Duration) -> String {
    // 获取总秒数 - 使用 std::time::Duration 的 as_secs 方法
    let total_secs = duration.as_secs();
    
    // 计算时分秒
    let hours = total_secs / 3600;
    let minutes = (total_secs % 3600) / 60;
    let seconds = total_secs % 60;
    
    // 获取毫秒 - 使用 std::time::Duration 的 subsec_millis 方法
    let millis = duration.subsec_millis();
    
    if hours > 0 {
        format!("{}小时 {}分钟 {}秒", hours, minutes, seconds)
    } else if minutes > 0 {
        format!("{}分钟 {}秒", minutes, seconds)
    } else if seconds > 0 {
        format!("{}秒 {}毫秒", seconds, millis)
    } else {
        format!("{}毫秒", millis)
    }
}