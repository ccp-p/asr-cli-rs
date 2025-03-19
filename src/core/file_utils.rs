use chrono::Duration;

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
    // 获取总秒数
    let total_secs = duration.num_seconds();
    
    // 计算时分秒
    let hours = total_secs / 3600;
    let minutes = (total_secs % 3600) / 60;
    let seconds = total_secs % 60;
    
    // 获取毫秒，chrono::Duration没有直接获取毫秒的方法
    // 可以通过获取总毫秒数然后取模来计算
    let millis = (duration.num_milliseconds() % 1000) as u32;
    
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